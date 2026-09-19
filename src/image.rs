//! Image chain: `base` → one layer per enabled toolchain → global user layer →
//! project layer. Every image carries a `claude_here.hash` label; a layer is
//! rebuilt when its inputs (Dockerfile text, build args, parent hash) change.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::docker::Docker;
use crate::paths::HostPaths;
use crate::toolchain::Toolchain;

/// Image repository name.
pub const REPO: &str = "claude_here";
/// Label holding the input hash.
pub const HASH_LABEL: &str = "claude_here.hash";
/// Label holding the tool version that built the image.
pub const VERSION_LABEL: &str = "claude_here.version";

/// Tag of the base image for a host uid. Images embed uid/gid/user name, so
/// every host user gets an own chain on a shared docker daemon.
pub fn base_tag(uid: &u32) -> String {
    format!("{REPO}:base-u{uid}")
}

/// Tag of the image with `names` layered on the base, in build order. Each
/// prefix of a chain is itself a valid tag, so `--go` and `--go --jvm` share
/// the `base-go` layer.
pub fn chain_tag(uid: u32, names: &[String]) -> String {
    let mut tag = base_tag(&uid);
    for n in names {
        tag.push('-');
        tag.push_str(n);
    }
    tag
}

const DOCKERFILE_BASE: &str = include_str!("../images/Dockerfile.base");
const ENTRYPOINT: &str = include_str!("../images/entrypoint.sh");
const NET_SUMMARY: &str = include_str!("../images/net-summary.sh");
const GIT_SHIM: &str = include_str!("../images/git-shim.sh");

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
    pub claude_version: String,
}

/// Build policy flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildPolicy {
    /// Rebuild every layer regardless of hash.
    pub force: bool,
    /// Never build; fail when an image is missing or stale.
    pub no_build: bool,
    /// Pass `--no-cache` and `--pull` to the base build (used by `update`).
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
    hash_inputs(&[input])[..8].to_string()
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

/// Orchestrates building the image chain.
pub struct Builder<'a> {
    pub docker: &'a Docker,
    pub paths: &'a HostPaths,
    pub identity: Identity,
    /// Toolchain layers in build order, as returned by `toolchain::resolve`.
    pub toolchains: Vec<&'static Toolchain>,
    pub policy: BuildPolicy,
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
        let (mut tag, mut hash) = self.ensure_base()?;
        let mut names: Vec<String> = Vec::new();
        for t in &self.toolchains {
            names.push(t.name.to_string());
            let layer_tag = chain_tag(self.identity.uid, &names);
            (tag, hash) = self.ensure_toolchain(&layer_tag, t, &tag, &hash)?;
        }
        let stem = chain_tag(self.identity.uid, &names);
        let user_file = self.paths.user_dockerfile();
        let user_snippet = read_snippet(&user_file)?;
        let has_user = user_snippet.as_deref().is_some_and(has_instructions);
        if let Some(snippet) = user_snippet.filter(|s| has_instructions(s)) {
            let next_tag = format!("{stem}-user");
            (tag, hash) = self.ensure_layer(&next_tag, &tag, &hash, &snippet)?;
        }
        let project_file = project_dir.join("Dockerfile");
        if let Some(snippet) = read_snippet(&project_file)?.filter(|s| has_instructions(s)) {
            let next_tag = project_tag(&stem, has_user, project_dir);
            (tag, _) = self.ensure_layer(&next_tag, &tag, &hash, &snippet)?;
        }
        Ok(tag)
    }

    fn ensure_base(&self) -> Result<(String, String)> {
        let id = &self.identity;
        let tag = base_tag(&id.uid);
        let hash = hash_inputs(&[
            DOCKERFILE_BASE,
            ENTRYPOINT,
            NET_SUMMARY,
            GIT_SHIM,
            &id.user,
            &id.uid.to_string(),
            &id.gid.to_string(),
            &id.claude_version,
        ]);
        if self.is_current(&tag, &hash) && !self.policy.refresh_base {
            return Ok((tag, hash));
        }
        self.refuse_if_no_build(&tag)?;
        eprintln!("claude_here: building {tag}");
        let ctx = BuildContext::new("base")?;
        ctx.write("Dockerfile", DOCKERFILE_BASE)?;
        ctx.write("entrypoint.sh", ENTRYPOINT)?;
        ctx.write("net-summary.sh", NET_SUMMARY)?;
        ctx.write("git-shim.sh", GIT_SHIM)?;
        let build_args = vec![
            ("CH_USER".to_string(), id.user.clone()),
            ("CH_UID".to_string(), id.uid.to_string()),
            ("CH_GID".to_string(), id.gid.to_string()),
            ("CLAUDE_VERSION".to_string(), id.claude_version.clone()),
        ];
        self.docker.build(
            &ctx.dir,
            &tag,
            &build_args,
            &Self::labels(&hash),
            self.policy.refresh_base,
        )?;
        Ok((tag, hash))
    }

    fn ensure_toolchain(
        &self,
        tag: &str,
        toolchain: &Toolchain,
        parent: &str,
        parent_hash: &str,
    ) -> Result<(String, String)> {
        let hash = hash_inputs(&[parent_hash, toolchain.dockerfile]);
        if self.is_current(tag, &hash) {
            return Ok((tag.to_string(), hash));
        }
        self.refuse_if_no_build(tag)?;
        eprintln!("claude_here: building {tag}");
        let ctx = BuildContext::new(toolchain.name)?;
        ctx.write("Dockerfile", toolchain.dockerfile)?;
        let build_args = vec![
            ("BASE".to_string(), parent.to_string()),
            ("CH_USER".to_string(), self.identity.user.clone()),
        ];
        self.docker
            .build(&ctx.dir, tag, &build_args, &Self::labels(&hash), false)?;
        Ok((tag.to_string(), hash))
    }

    fn ensure_layer(
        &self,
        tag: &str,
        parent: &str,
        parent_hash: &str,
        snippet: &str,
    ) -> Result<(String, String)> {
        let text = wrap_user_layer(parent, &self.identity.user, snippet);
        let hash = hash_inputs(&[parent_hash, &text]);
        if self.is_current(tag, &hash) {
            return Ok((tag.to_string(), hash));
        }
        self.refuse_if_no_build(tag)?;
        eprintln!("claude_here: building {tag}");
        let ctx = BuildContext::new("layer")?;
        ctx.write("Dockerfile", &text)?;
        self.docker
            .build(&ctx.dir, tag, &[], &Self::labels(&hash), false)?;
        Ok((tag.to_string(), hash))
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

    #[test]
    fn hash_is_stable_and_sensitive() {
        let a = hash_inputs(&["x", "y"]);
        let b = hash_inputs(&["x", "y"]);
        let c = hash_inputs(&["xy", ""]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn detects_instructions() {
        assert!(!has_instructions("# only comment\n\n   \n"));
        assert!(has_instructions("# c\nRUN apt-get install -y x\n"));
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
        let tag = project_tag(&chain_tag(1000, &all), true, Path::new("/home/axel/p"));
        assert!(tag.len() <= 128, "tag too long: {} ({})", tag, tag.len());
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
