//! Image chain: `base` → one layer per enabled toolchain → global user layer →
//! project layer → Claude Code. Every image carries a `claude_here.hash`
//! label; a layer is rebuilt when its inputs (Dockerfile text, build args,
//! parent image id) change. Hashing the parent's id rather than its inputs
//! means a rebuilt parent (`update --base`) makes every child stale.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

use crate::docker::{Build, Docker};
use crate::paths::HostPaths;
use crate::run::CA_BUNDLE;
use crate::toolchain::Toolchain;

/// Image repository name.
pub const REPO: &str = "claude_here";
/// Label holding the input hash.
pub const HASH_LABEL: &str = "claude_here.hash";
/// Label holding the tool version that built the image.
pub const VERSION_LABEL: &str = "claude_here.version";
/// Label holding the Claude Code version of the Claude layer.
pub const CLAUDE_LABEL: &str = "claude_here.claude_version";

/// Tag of the base image for a host uid. Images embed uid/gid/user name, so
/// every host user gets an own chain on a shared docker daemon.
pub fn base_tag(uid: &u32) -> String {
    format!("{REPO}:base-u{uid}")
}

/// Tag of the image with `names` layered on the base, in build order. Each
/// prefix of a chain is itself a valid tag, so `--go` and `--go --jvm` share
/// the `base-go` layer.
pub fn chain_tag(uid: u32, names: &[String]) -> String {
    let base = base_tag(&uid);
    let mut tag = base.clone();
    for n in names {
        tag.push('-');
        tag.push_str(n);
    }
    if tag.len() > MAX_STEM {
        // Only a chain of nearly every toolchain gets here; its prefixes keep
        // their readable tags, the full chain gets a hash of its names.
        return format!("{base}-{}", short_hash(&names.join("-")));
    }
    tag
}

/// Longest toolchain chain tag that still leaves room for `-user-<sha8>` and
/// `-claude` within docker's 128 character tag limit.
const MAX_STEM: usize = 128 - "-user-".len() - 8 - "-claude".len();

/// Tag of the Claude layer on top of `parent`; the image a session runs.
pub fn claude_tag(parent: &str) -> String {
    format!("{parent}-claude")
}

const DOCKERFILE_BASE: &str = include_str!("../images/Dockerfile.base");
const DOCKERFILE_CLAUDE: &str = include_str!("../images/claude.dockerfile");
const ENTRYPOINT: &str = include_str!("../images/entrypoint.sh");
const NET_SUMMARY: &str = include_str!("../images/net-summary.sh");
const GIT_SHIM: &str = include_str!("../images/git-shim.sh");
const CLOUD_SHIM: &str = include_str!("../images/cloud-shim.sh");
const NET_ALLOWLIST: &str = include_str!("../images/net-allowlist.sh");
const CA_JVM: &str = include_str!("../images/ca-jvm.sh");

/// Template written to `~/.config/claude_here/Dockerfile` on init.
pub const USER_DOCKERFILE_TEMPLATE: &str = "\
# claude_here global user layer.
#
# Add instructions that should be part of every claude_here image on this
# machine. Do NOT write a FROM line: the tool builds this on top of the enabled
# toolchains. The build runs as root; you do not need USER lines. Changes
# trigger an automatic rebuild on next start.
#
# The shipped toolchains have their own switches (`toolchains = [\"rust\"]`, or
# --rust, --jvm, --python, ...); use this file for everything else. Examples:
# RUN apt-get update && apt-get install -y --no-install-recommends shellcheck && rm -rf /var/lib/apt/lists/*
# RUN curl -fsSL https://example.com/tool.tar.gz | tar -xz -C /usr/local/bin
";

/// Template for `.claude_here/Dockerfile` (documented, not auto-created).
pub const PROJECT_DOCKERFILE_TEMPLATE: &str = "\
# claude_here project layer. Same rules as the global layer: no FROM, root
# build context. Built on top of the global user layer.
";

/// Identity parameters that go into the base image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub user: String,
    pub uid: u32,
    pub gid: u32,
    /// Claude Code version for the Claude layer: an exact version, or a
    /// channel name when none could be resolved.
    pub claude_version: String,
}

/// Build policy flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildPolicy {
    /// Rebuild every layer regardless of hash.
    pub force: bool,
    /// Never build; fail when an image is missing or stale.
    pub no_build: bool,
    /// Pass `--no-cache` and `--pull` to the base build (`update --base`).
    pub refresh_base: bool,
}

/// Returns `sha256(inputs...)` as hex.
pub fn hash_inputs(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0u8]);
    }
    hex::encode(h.finalize())
}

/// Short hash used in project layer tags.
pub fn short_hash(input: &str) -> String {
    hash_inputs(&[input]).chars().take(8).collect()
}

/// Does a user-supplied Dockerfile contain anything besides comments/blank lines?
pub fn has_instructions(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .any(|l| !l.is_empty() && !l.starts_with('#'))
}

/// Wrap a user snippet into a complete Dockerfile on top of `parent`.
pub fn wrap_user_layer(parent: &str, user: &str, snippet: &str) -> String {
    format!(
        "FROM {parent}\nARG CH_USER={user}\nUSER root\n{snippet}\nUSER root\nENTRYPOINT [\"/usr/local/lib/claude_here/entrypoint.sh\"]\n"
    )
}

/// Compute the tag for a project layer on top of `stem` (the toolchain chain tag).
pub fn project_tag(stem: &str, has_user_layer: bool, project_dir: &Path) -> String {
    let suffix = if has_user_layer { "user" } else { "p" };
    format!(
        "{stem}-{suffix}-{}",
        short_hash(&project_dir.display().to_string())
    )
}

/// BuildKit secret id under which the base build sees the company CA.
pub const CA_SECRET: &str = "claude_here_ca";

/// Hash of everything the base layer is built from: the files, the identity
/// and the company CA, so a changed or removed CA rebuilds the chain.
pub fn base_hash(id: &Identity, ca: Option<&str>) -> String {
    hash_inputs(&[
        DOCKERFILE_BASE,
        ENTRYPOINT,
        NET_SUMMARY,
        GIT_SHIM,
        CLOUD_SHIM,
        NET_ALLOWLIST,
        CA_JVM,
        &id.user,
        &id.uid.to_string(),
        &id.gid.to_string(),
        ca.unwrap_or(""),
    ])
}

/// Orchestrates building the image chain.
pub struct Builder<'a> {
    pub docker: &'a Docker,
    pub paths: &'a HostPaths,
    pub identity: Identity,
    /// Toolchain layers in build order, as returned by `toolchain::resolve`.
    pub toolchains: Vec<&'static Toolchain>,
    pub policy: BuildPolicy,
    /// `[tls] ca`, expanded: baked into the base layer's bundle so every
    /// download of every layer works behind a TLS-intercepting proxy
    /// (ADR 0009). Its content is part of the base hash.
    pub ca: Option<PathBuf>,
}

impl Builder<'_> {
    /// Make sure the chain exists and is current; returns the tag to run. A
    /// custom image bypasses the chain entirely and is only checked for
    /// existence.
    pub fn ensure(&self, custom_image: Option<&str>, project_dir: &Path) -> Result<String> {
        if let Some(image) = custom_image {
            if self.docker.image_label(image, "").is_none()
                && self.docker.output(&["image", "inspect", image]).is_err()
            {
                bail!("custom image '{image}' not found locally");
            }
            return Ok(image.to_string());
        }
        let mut tag = self.ensure_base()?;
        let mut names: Vec<String> = Vec::new();
        for t in &self.toolchains {
            names.push(t.name.to_string());
            let layer_tag = chain_tag(self.identity.uid, &names);
            tag = self.ensure_toolchain(&layer_tag, t, &tag)?;
        }
        for (next_tag, snippet) in self.snippet_layers(project_dir)? {
            tag = self.ensure_layer(&next_tag, &tag, &snippet)?;
        }
        self.ensure_claude(&tag)
    }

    /// The tag `ensure` returns, without building anything.
    pub fn final_tag(&self, custom_image: Option<&str>, project_dir: &Path) -> Result<String> {
        if let Some(image) = custom_image {
            return Ok(image.to_string());
        }
        let top = self.snippet_layers(project_dir)?.pop().map(|(t, _)| t);
        Ok(claude_tag(&top.unwrap_or_else(|| self.stem())))
    }

    fn stem(&self) -> String {
        let names: Vec<String> = self.toolchains.iter().map(|t| t.name.to_string()).collect();
        chain_tag(self.identity.uid, &names)
    }

    /// `(tag, snippet)` of the global user layer and the project layer, in
    /// build order, for those that have instructions.
    fn snippet_layers(&self, project_dir: &Path) -> Result<Vec<(String, String)>> {
        let stem = self.stem();
        let mut layers = Vec::new();
        let user_snippet = read_snippet(&self.paths.user_dockerfile())?;
        let has_user = user_snippet.as_deref().is_some_and(has_instructions);
        if let Some(snippet) = user_snippet.filter(|s| has_instructions(s)) {
            layers.push((format!("{stem}-user"), snippet));
        }
        let project_file = project_dir.join("Dockerfile");
        if let Some(snippet) = read_snippet(&project_file)?.filter(|s| has_instructions(s)) {
            layers.push((project_tag(&stem, has_user, project_dir), snippet));
        }
        Ok(layers)
    }

    fn ensure_base(&self) -> Result<String> {
        let id = &self.identity;
        let tag = base_tag(&id.uid);
        let ca = match &self.ca {
            Some(p) => Some(
                fs::read_to_string(p)
                    .with_context(|| format!("reading the company CA {}", p.display()))?,
            ),
            None => None,
        };
        let hash = base_hash(id, ca.as_deref());
        if self.is_current(&tag, &hash) && !self.policy.refresh_base {
            return Ok(tag);
        }
        self.refuse_if_no_build(&tag)?;
        eprintln!("claude_here: building {tag}");
        let ctx = BuildContext::new("base")?;
        ctx.write("Dockerfile", DOCKERFILE_BASE)?;
        ctx.write("entrypoint.sh", ENTRYPOINT)?;
        ctx.write("net-summary.sh", NET_SUMMARY)?;
        ctx.write("git-shim.sh", GIT_SHIM)?;
        ctx.write("cloud-shim.sh", CLOUD_SHIM)?;
        ctx.write("net-allowlist.sh", NET_ALLOWLIST)?;
        ctx.write("ca-jvm.sh", CA_JVM)?;
        let mut build = Build {
            context: ctx.dir.clone(),
            tag: tag.clone(),
            build_args: vec![
                ("CH_USER".to_string(), id.user.clone()),
                ("CH_UID".to_string(), id.uid.to_string()),
                ("CH_GID".to_string(), id.gid.to_string()),
            ],
            labels: Self::labels(&hash),
            secrets: Vec::new(),
            refresh: self.policy.refresh_base,
        };
        if let Some(p) = &self.ca {
            build
                .build_args
                .push(("CH_CA_BUNDLE".to_string(), CA_BUNDLE.to_string()));
            build.secrets.push((CA_SECRET.to_string(), p.clone()));
        }
        self.docker.build(&build)?;
        Ok(tag)
    }

    fn ensure_toolchain(&self, tag: &str, toolchain: &Toolchain, parent: &str) -> Result<String> {
        let hash = hash_inputs(&[&self.parent_id(parent)?, toolchain.dockerfile]);
        if self.is_current(tag, &hash) {
            return Ok(tag.to_string());
        }
        self.refuse_if_no_build(tag)?;
        eprintln!("claude_here: building {tag}");
        let ctx = BuildContext::new(toolchain.name)?;
        ctx.write("Dockerfile", toolchain.dockerfile)?;
        self.docker.build(&Build {
            context: ctx.dir.clone(),
            tag: tag.to_string(),
            build_args: vec![
                ("BASE".to_string(), parent.to_string()),
                ("CH_USER".to_string(), self.identity.user.clone()),
            ],
            labels: Self::labels(&hash),
            ..Default::default()
        })?;
        Ok(tag.to_string())
    }

    fn ensure_layer(&self, tag: &str, parent: &str, snippet: &str) -> Result<String> {
        let text = wrap_user_layer(parent, &self.identity.user, snippet);
        let hash = hash_inputs(&[&self.parent_id(parent)?, &text]);
        if self.is_current(tag, &hash) {
            return Ok(tag.to_string());
        }
        self.refuse_if_no_build(tag)?;
        eprintln!("claude_here: building {tag}");
        let ctx = BuildContext::new("layer")?;
        ctx.write("Dockerfile", &text)?;
        self.docker.build(&Build {
            context: ctx.dir.clone(),
            tag: tag.to_string(),
            labels: Self::labels(&hash),
            ..Default::default()
        })?;
        Ok(tag.to_string())
    }

    /// The Claude layer on top of `parent`. Under `--no-build` an existing
    /// layer with an older Claude is used with a note: the version moved, the
    /// image below did not.
    fn ensure_claude(&self, parent: &str) -> Result<String> {
        let tag = claude_tag(parent);
        let version = &self.identity.claude_version;
        let hash = hash_inputs(&[
            &self.parent_id(parent)?,
            DOCKERFILE_CLAUDE,
            &self.identity.user,
            version,
        ]);
        if self.is_current(&tag, &hash) {
            return Ok(tag);
        }
        if self.policy.no_build && self.docker.image_id(&tag).is_some() {
            let have = self
                .docker
                .image_label(&tag, CLAUDE_LABEL)
                .unwrap_or_else(|| "unknown".into());
            eprintln!(
                "claude_here: note: {tag} has Claude Code {have}, {version} is accepted; kept because of --no-build"
            );
            return Ok(tag);
        }
        self.refuse_if_no_build(&tag)?;
        eprintln!("claude_here: building {tag} (Claude Code {version})");
        let ctx = BuildContext::new("claude")?;
        ctx.write("Dockerfile", DOCKERFILE_CLAUDE)?;
        let mut labels = Self::labels(&hash);
        labels.push((CLAUDE_LABEL.to_string(), version.clone()));
        self.docker.build(&Build {
            context: ctx.dir.clone(),
            tag: tag.clone(),
            build_args: vec![
                ("BASE".to_string(), parent.to_string()),
                ("CH_USER".to_string(), self.identity.user.clone()),
                ("CLAUDE_VERSION".to_string(), version.clone()),
            ],
            labels,
            ..Default::default()
        })?;
        Ok(tag)
    }

    /// Id of an image the chain just ensured; its absence is a bug upstream.
    fn parent_id(&self, parent: &str) -> Result<String> {
        self.docker
            .image_id(parent)
            .ok_or_else(|| anyhow!("image {parent} is missing"))
    }

    fn is_current(&self, tag: &str, hash: &str) -> bool {
        !self.policy.force
            && self
                .docker
                .image_label(tag, HASH_LABEL)
                .is_some_and(|h| h == hash)
    }

    fn refuse_if_no_build(&self, tag: &str) -> Result<()> {
        if self.policy.no_build {
            bail!("image {tag} is missing or stale and --no-build was given");
        }
        Ok(())
    }

    fn labels(hash: &str) -> Vec<(String, String)> {
        vec![
            (HASH_LABEL.to_string(), hash.to_string()),
            (
                VERSION_LABEL.to_string(),
                env!("CARGO_PKG_VERSION").to_string(),
            ),
        ]
    }
}

fn read_snippet(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Temporary build context, removed on drop.
struct BuildContext {
    dir: PathBuf,
}

impl BuildContext {
    fn new(name: &str) -> Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!(
            "claude_here-build-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(Self { dir })
    }

    fn write(&self, name: &str, content: &str) -> Result<()> {
        let p = self.dir.join(name);
        fs::write(&p, content).with_context(|| format!("writing {}", p.display()))
    }
}

impl Drop for BuildContext {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolchain;

    /// Every file `Dockerfile.base` copies must be written into the build
    /// context and go into the base hash, or the build fails with
    /// "failed to compute cache key".
    #[test]
    fn base_context_has_every_copied_file() {
        let copied: Vec<String> = DOCKERFILE_BASE
            .lines()
            .filter_map(|l| l.trim().strip_prefix("COPY "))
            .flat_map(|rest| {
                rest.split_whitespace()
                    .filter(|w| {
                        std::path::Path::new(w)
                            .extension()
                            .is_some_and(|e| e == "sh")
                    })
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(copied.len() >= 4, "found {copied:?}");
        let written = [
            "entrypoint.sh",
            "net-summary.sh",
            "git-shim.sh",
            "cloud-shim.sh",
            "net-allowlist.sh",
            "ca-jvm.sh",
        ];
        for f in &copied {
            assert!(
                written.contains(&f.as_str()),
                "{f} is copied but never written"
            );
        }
        for f in written {
            let text = match f {
                "entrypoint.sh" => ENTRYPOINT,
                "net-summary.sh" => NET_SUMMARY,
                "git-shim.sh" => GIT_SHIM,
                "cloud-shim.sh" => CLOUD_SHIM,
                "net-allowlist.sh" => NET_ALLOWLIST,
                _ => CA_JVM,
            };
            assert!(!text.is_empty(), "{f} is empty");
            assert!(
                copied.contains(&f.to_string()),
                "{f} is written but never copied"
            );
        }
    }

    /// A toolchain build is long; one transient DNS or TLS failure must not
    /// discard it. Every download therefore retries.
    #[test]
    fn every_download_retries() {
        let mut files: Vec<(&str, &str)> = vec![
            ("Dockerfile.base", DOCKERFILE_BASE),
            ("claude.dockerfile", DOCKERFILE_CLAUDE),
            ("net-allowlist.sh", NET_ALLOWLIST),
        ];
        for t in crate::toolchain::TOOLCHAINS {
            files.push((t.name, t.dockerfile));
        }
        for (name, text) in files {
            for (n, line) in text.lines().enumerate() {
                let line = line.trim();
                // Only actual invocations, not the apt package named "curl".
                if !line.contains("curl -") || line.starts_with('#') {
                    continue;
                }
                assert!(
                    line.contains("--retry") || line.contains("${CH_CURL}"),
                    "{name}:{} downloads without retrying: {line}",
                    n + 1
                );
            }
        }
    }

    /// `RUN --mount` is a BuildKit feature and needs the syntax directive on
    /// the very first line, or the build fails with a parse error.
    #[test]
    fn cache_mounts_come_with_the_syntax_directive() {
        let mut files: Vec<(&str, &str)> = vec![
            ("Dockerfile.base", DOCKERFILE_BASE),
            ("claude.dockerfile", DOCKERFILE_CLAUDE),
        ];
        for t in crate::toolchain::TOOLCHAINS {
            files.push((t.name, t.dockerfile));
        }
        for (name, text) in files {
            if !text.contains("--mount=type=") {
                continue;
            }
            assert!(
                text.starts_with("# syntax=docker/dockerfile:"),
                "{name} uses a cache mount without the syntax directive"
            );
        }
    }

    /// Ownership must be established before a tree is filled, never after: a
    /// `chown -R` over a checkout copies every file into a new layer.
    #[test]
    fn large_trees_are_not_chowned_after_the_fact() {
        for t in crate::toolchain::TOOLCHAINS {
            for line in t.dockerfile.lines() {
                let line = line.trim();
                assert!(
                    line.starts_with('#') || !line.contains("chown -R"),
                    "{}: {line}",
                    t.name
                );
            }
        }
    }

    /// Claude Code lives in its own last layer, so a new release rebuilds
    /// that layer alone and never the base below it.
    #[test]
    fn claude_is_installed_by_its_own_layer_only() {
        assert!(!DOCKERFILE_BASE.contains("install.sh"));
        assert!(!DOCKERFILE_BASE.contains("CLAUDE_VERSION"));
        assert!(DOCKERFILE_CLAUDE.contains("claude.ai/install.sh"));
        // ARGs before FROM are out of scope after it; these must follow it.
        let (_, after_from) = DOCKERFILE_CLAUDE
            .split_once("FROM ${BASE}")
            .unwrap_or_default();
        assert!(after_from.contains("ARG CLAUDE_VERSION"));
        assert!(after_from.contains("ARG CH_USER"));
        assert!(after_from.contains("bash -s -- \"${CLAUDE_VERSION}\""));
        // Installed as the sandbox user; the image ends as root for the entrypoint.
        assert!(after_from.contains("USER ${CH_USER}"));
        assert_eq!(
            DOCKERFILE_CLAUDE
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty()),
            Some("USER root")
        );
    }

    #[test]
    fn claude_tag_extends_its_parent() {
        assert_eq!(
            claude_tag("claude_here:base-u1000-rust"),
            "claude_here:base-u1000-rust-claude"
        );
    }

    #[test]
    fn hash_is_stable_and_sensitive() {
        let a = hash_inputs(&["x", "y"]);
        let b = hash_inputs(&["x", "y"]);
        let c = hash_inputs(&["xy", ""]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    /// The company CA is baked into the base layer, so its content is part
    /// of the base hash: setting, changing or removing it rebuilds the chain.
    #[test]
    fn base_hash_follows_the_company_ca() {
        let id = Identity {
            user: "ni".into(),
            uid: 1000,
            gid: 1000,
            claude_version: "stable".into(),
        };
        let none = base_hash(&id, None);
        let one = base_hash(&id, Some("-----BEGIN CERTIFICATE-----\nA\n"));
        let two = base_hash(&id, Some("-----BEGIN CERTIFICATE-----\nB\n"));
        assert_eq!(none, base_hash(&id, None));
        assert_ne!(none, one);
        assert_ne!(one, two);
    }

    /// The contract between `ensure_base` and `Dockerfile.base`: the secret id
    /// and the build argument it passes, the path the session mounts the CA
    /// at, and what the step leaves behind for pip. The jvm layer imports
    /// the baked CA into its own store through the shared script.
    #[test]
    fn base_image_bakes_the_company_ca_from_the_build_secret() {
        let crt = crate::run::CA_CRT;
        assert!(DOCKERFILE_BASE.contains(&format!("--mount=type=secret,id={CA_SECRET}")));
        assert!(DOCKERFILE_BASE.contains(&format!("/run/secrets/{CA_SECRET}")));
        assert!(DOCKERFILE_BASE.contains(&format!("cp /run/secrets/{CA_SECRET} {crt}")));
        assert!(DOCKERFILE_BASE.contains("update-ca-certificates"));
        assert!(DOCKERFILE_BASE.contains(&format!("cert = {CA_BUNDLE}\\n' > /etc/pip.conf")));
        assert!(
            DOCKERFILE_BASE.contains("ARG CH_CA_BUNDLE=\nENV NODE_EXTRA_CA_CERTS=${CH_CA_BUNDLE}")
        );
        // The CA step must come before the first download of the base.
        let ca_step = DOCKERFILE_BASE.find("/run/secrets/").unwrap_or(usize::MAX);
        let first_curl = DOCKERFILE_BASE.find("curl -fsSL").unwrap_or(0);
        assert!(ca_step < first_curl);
        let jvm = crate::toolchain::TOOLCHAINS
            .iter()
            .find(|t| t.name == "jvm")
            .map(|t| t.dockerfile)
            .unwrap_or_default();
        assert!(jvm.contains(&format!("/usr/local/lib/claude_here/ca-jvm.sh {crt}")));
        let import = jvm.find("ca-jvm.sh").unwrap_or(0);
        let graalvm = jvm.find("JAVA_HOME=/opt/graalvm").unwrap_or(usize::MAX);
        assert!(graalvm < import, "the import needs the JDK");
    }

    #[test]
    fn detects_instructions() {
        assert!(!has_instructions("# only comment\n\n   \n"));
        assert!(has_instructions("# c\nRUN apt-get install -y x\n"));
    }

    /// The contract between `run::add_containers` and the root phase: the
    /// switch it sets, the socket path it announces, the kernel floor of
    /// ADR 0010.
    #[test]
    fn entrypoint_starts_the_podman_socket_on_the_containers_switch() {
        assert!(ENTRYPOINT.contains("CH_CONTAINERS=\"${CH_CONTAINERS:-0}\""));
        assert!(ENTRYPOINT.contains("podman system service --time=0 \"unix://${sock}\""));
        assert!(ENTRYPOINT.contains("/podman/podman.sock"));
        assert!(ENTRYPOINT.contains("kernel 5.11 or newer"));
        assert!(ENTRYPOINT.contains("kill -TERM \"${PODMAN_PID}\""));
    }

    /// `keytool -importcert` takes one certificate per call; the script
    /// splits a chain and imports each certificate under its own alias,
    /// replacing an alias that exists already. The entrypoint and the jvm
    /// layer both go through it.
    #[test]
    fn ca_jvm_script_imports_every_certificate_of_a_chain() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = dir.path().join("ca-jvm.sh");
        fs::write(&script, CA_JVM)?;
        let log = dir.path().join("keytool.log");
        let keytool = dir.path().join("keytool");
        fs::write(
            &keytool,
            format!(
                "#!/bin/sh\necho \"$@\" >> {}\ncase \"$1\" in -importcert) for a in \"$@\"; do f=\"$a\"; done; cat \"$f\" >> {};; esac\n",
                log.display(),
                dir.path().join("imported.pem").display()
            ),
        )?;
        fs::set_permissions(
            &keytool,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )?;
        let pem = dir.path().join("ca.pem");
        fs::write(
            &pem,
            "Bag Attributes\n    friendlyName: root\n-----BEGIN CERTIFICATE-----\nAAAA\nBBBB\n-----END CERTIFICATE-----\nsubject=intermediate\n-----BEGIN CERTIFICATE-----\nCCCC\n-----END CERTIFICATE-----\n",
        )?;
        let out = std::process::Command::new("sh")
            .arg(&script)
            .arg(&pem)
            .arg(&keytool)
            .output()?;
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let calls = fs::read_to_string(&log)?;
        let imports: Vec<&str> = calls
            .lines()
            .filter(|l| l.starts_with("-importcert"))
            .collect();
        assert_eq!(imports.len(), 2, "{calls}");
        assert!(imports[0].contains("-alias claude_here-1 "), "{calls}");
        assert!(imports[1].contains("-alias claude_here-2 "), "{calls}");
        assert_eq!(
            calls.lines().filter(|l| l.starts_with("-delete")).count(),
            2,
            "{calls}"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("imported.pem"))?,
            "-----BEGIN CERTIFICATE-----\nAAAA\nBBBB\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nCCCC\n-----END CERTIFICATE-----\n"
        );
        assert!(ENTRYPOINT.contains("/usr/local/lib/claude_here/ca-jvm.sh \"${CA_CRT}\""));
        assert!(!ENTRYPOINT.contains("keytool -importcert"));
        Ok(())
    }

    /// Without a keytool the script is a no-op, so the entrypoint can call it
    /// unconditionally; without a certificate in the file it fails.
    #[test]
    fn ca_jvm_script_without_keytool_or_certificate() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = dir.path().join("ca-jvm.sh");
        fs::write(&script, CA_JVM)?;
        let pem = dir.path().join("ca.pem");
        fs::write(&pem, "not a certificate\n")?;
        let missing = dir.path().join("no-keytool");
        let out = std::process::Command::new("sh")
            .args([&script, &pem, &missing])
            .output()?;
        assert!(out.status.success());
        let keytool = dir.path().join("keytool");
        fs::write(&keytool, "#!/bin/sh\nexit 0\n")?;
        fs::set_permissions(
            &keytool,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )?;
        let out = std::process::Command::new("sh")
            .args([&script, &pem, &keytool])
            .output()?;
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("no certificate"));
        Ok(())
    }

    #[test]
    fn wraps_layer_as_root_with_entrypoint() {
        let t = wrap_user_layer("claude_here:base-u1000-rust", "ni", "RUN echo hi");
        assert!(t.starts_with("FROM claude_here:base-u1000-rust\n"));
        assert!(t.contains("\nUSER root\nRUN echo hi\nUSER root\n"));
        assert!(t.trim_end().ends_with("entrypoint.sh\"]"));
    }

    #[test]
    fn chain_tag_lists_toolchains_in_build_order() {
        let chain = toolchain::resolve(&["rust".into(), "jvm".into()]).unwrap_or_default();
        let names = toolchain::names(&chain);
        assert_eq!(
            chain_tag(1000, &names),
            "claude_here:base-u1000-jvm-rust".to_string()
        );
        assert_eq!(chain_tag(1000, &[]), base_tag(&1000));
        assert_ne!(base_tag(&1000), base_tag(&1001));
    }

    #[test]
    fn chain_tag_stays_within_the_docker_limit() {
        let all: Vec<String> = toolchain::TOOLCHAINS
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        let tag = claude_tag(&project_tag(
            &chain_tag(1000, &all),
            true,
            Path::new("/home/axel/p"),
        ));
        assert!(tag.len() <= 128, "tag too long: {} ({})", tag, tag.len());
        // Every prefix short enough to be readable stays readable.
        let few = chain_tag(1000, &["rust".to_string(), "docker".to_string()]);
        assert_eq!(few, "claude_here:base-u1000-rust-docker");
    }

    #[test]
    fn project_tag_is_short_and_distinct() {
        let stem = chain_tag(1000, &["rust".to_string()]);
        let a = project_tag(&stem, true, Path::new("/home/axel/a"));
        let b = project_tag(&stem, true, Path::new("/home/axel/b"));
        assert!(a.starts_with("claude_here:base-u1000-rust-user-"));
        assert_eq!(a.len(), "claude_here:base-u1000-rust-user-".len() + 8);
        assert_ne!(a, b);
        assert!(
            project_tag(&base_tag(&1001), false, Path::new("/x"))
                .starts_with("claude_here:base-u1001-p-")
        );
    }
}
