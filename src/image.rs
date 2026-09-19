//! Image chain: `base` → variant → global user layer → project layer.
//! Every image carries a `claude_here.hash` label; a layer is rebuilt when
//! its inputs (Dockerfile text, build args, parent hash) change.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::docker::Docker;
use crate::paths::HostPaths;

/// Image repository name.
pub const REPO: &str = "claude_here";
/// Label holding the input hash.
pub const HASH_LABEL: &str = "claude_here.hash";
/// Label holding the tool version that built the image.
pub const VERSION_LABEL: &str = "claude_here.version";
/// Known variants shipped with the tool.
pub const VARIANTS: &[&str] = &["base", "rust", "jvm"];

/// Tag of a variant image for a host uid. Images embed uid/gid/user name, so
/// every host user gets an own chain on a shared docker daemon.
pub fn variant_tag(variant: &str, uid: u32) -> String {
    format!("{REPO}:{variant}-u{uid}")
}

/// Tag of the base image for a host uid.
pub fn base_tag(uid: &u32) -> String {
    variant_tag("base", *uid)
}

const DOCKERFILE_BASE: &str = include_str!("../images/Dockerfile.base");
const DOCKERFILE_RUST: &str = include_str!("../images/Dockerfile.rust");
const DOCKERFILE_JVM: &str = include_str!("../images/Dockerfile.jvm");
const DOCKERFILE_ADDONS: &str = include_str!("../images/Dockerfile.addons");
const ENTRYPOINT: &str = include_str!("../images/entrypoint.sh");
const NET_SUMMARY: &str = include_str!("../images/net-summary.sh");
const GIT_SHIM: &str = include_str!("../images/git-shim.sh");

/// Template written to `~/.config/claude_here/Dockerfile` on init.
pub const USER_DOCKERFILE_TEMPLATE: &str = "\
# claude_here global user layer.
#
# Add instructions that should be part of every claude_here image on this
# machine. Do NOT write a FROM line: the tool builds this on top of the
# selected variant (base, rust, jvm, ...). The build runs as root; you do not
# need USER lines. Changes trigger an automatic rebuild on next start.
#
# Node.js/npm and uv have their own switches (node = true / uv = true, or
# --node / --uv); use this file for everything else. Examples:
# RUN apt-get update && apt-get install -y --no-install-recommends shellcheck && rm -rf /var/lib/apt/lists/*
# RUN curl -fsSL https://example.com/tool.tar.gz | tar -xz -C /usr/local/bin
";

/// Template for `.claude_here/Dockerfile` (documented, not auto-created).
pub const PROJECT_DOCKERFILE_TEMPLATE: &str = "\
# claude_here project layer. Same rules as the global layer: no FROM, root
# build context. Built on top of the global user layer.
";

fn variant_dockerfile(variant: &str) -> Option<&'static str> {
    match variant {
        "base" => Some(DOCKERFILE_BASE),
        "rust" => Some(DOCKERFILE_RUST),
        "jvm" => Some(DOCKERFILE_JVM),
        _ => None,
    }
}

/// Whether the variant name is one of the shipped ones (vs. a custom image).
pub fn is_known_variant(name: &str) -> bool {
    VARIANTS.contains(&name)
}

/// Identity parameters that go into the base image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub user: String,
    pub uid: u32,
    pub gid: u32,
    pub claude_version: String,
}

/// Optional tools layered on top of a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Addons {
    pub node: bool,
    pub uv: bool,
}

impl Addons {
    pub fn any(self) -> bool {
        self.node || self.uv
    }

    /// Tag suffix such as `-node-uv`.
    pub fn suffix(self) -> String {
        let mut s = String::new();
        if self.node {
            s.push_str("-node");
        }
        if self.uv {
            s.push_str("-uv");
        }
        s
    }
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

/// Compute the tag for a project layer on top of `stem` (variant tag + addon suffix).
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
    pub addons: Addons,
    pub policy: BuildPolicy,
}

impl Builder<'_> {
    /// Make sure the full chain for `variant` (or a custom image name) exists
    /// and is current; returns the tag to run.
    pub fn ensure(&self, variant: &str, project_dir: &Path) -> Result<String> {
        if !is_known_variant(variant) {
            // Custom image: user is responsible; just make sure it exists.
            if self.docker.image_label(variant, "").is_none()
                && self.docker.output(&["image", "inspect", variant]).is_err()
            {
                bail!("custom image '{variant}' not found locally");
            }
            return Ok(variant.to_string());
        }
        let (mut tag, mut hash) = self.ensure_base()?;
        if variant != "base" {
            (tag, hash) = self.ensure_variant(variant, &tag, &hash)?;
        }
        if self.addons.any() {
            (tag, hash) = self.ensure_addons(variant, &tag, &hash)?;
        }
        let stem = format!(
            "{}{}",
            variant_tag(variant, self.identity.uid),
            self.addons.suffix()
        );
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

    fn ensure_variant(
        &self,
        variant: &str,
        parent: &str,
        parent_hash: &str,
    ) -> Result<(String, String)> {
        let tag = variant_tag(variant, self.identity.uid);
        let text = variant_dockerfile(variant).context("unknown variant")?;
        let hash = hash_inputs(&[parent_hash, text]);
        if self.is_current(&tag, &hash) {
            return Ok((tag, hash));
        }
        self.refuse_if_no_build(&tag)?;
        eprintln!("claude_here: building {tag}");
        let ctx = BuildContext::new(variant)?;
        ctx.write("Dockerfile", text)?;
        let build_args = vec![
            ("BASE".to_string(), parent.to_string()),
            ("CH_USER".to_string(), self.identity.user.clone()),
        ];
        self.docker
            .build(&ctx.dir, &tag, &build_args, &Self::labels(&hash), false)?;
        Ok((tag, hash))
    }

    fn ensure_addons(
        &self,
        variant: &str,
        parent: &str,
        parent_hash: &str,
    ) -> Result<(String, String)> {
        let tag = format!(
            "{}{}",
            variant_tag(variant, self.identity.uid),
            self.addons.suffix()
        );
        let hash = hash_inputs(&[parent_hash, DOCKERFILE_ADDONS, &self.addons.suffix()]);
        if self.is_current(&tag, &hash) {
            return Ok((tag, hash));
        }
        self.refuse_if_no_build(&tag)?;
        eprintln!("claude_here: building {tag}");
        let ctx = BuildContext::new("addons")?;
        ctx.write("Dockerfile", DOCKERFILE_ADDONS)?;
        let flag = |b: bool| if b { "1" } else { "0" }.to_string();
        let build_args = vec![
            ("BASE".to_string(), parent.to_string()),
            ("CH_USER".to_string(), self.identity.user.clone()),
            ("ADD_NODE".to_string(), flag(self.addons.node)),
            ("ADD_UV".to_string(), flag(self.addons.uv)),
        ];
        self.docker
            .build(&ctx.dir, &tag, &build_args, &Self::labels(&hash), false)?;
        Ok((tag, hash))
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
        let t = wrap_user_layer("claude_here:rust", "ni", "RUN echo hi");
        assert!(t.starts_with("FROM claude_here:rust\n"));
        assert!(t.contains("\nUSER root\nRUN echo hi\nUSER root\n"));
        assert!(t.trim_end().ends_with("entrypoint.sh\"]"));
    }

    #[test]
    fn project_tag_is_short_and_distinct() {
        let stem = variant_tag("rust", 1000);
        let a = project_tag(&stem, true, Path::new("/home/axel/a"));
        let b = project_tag(&stem, true, Path::new("/home/axel/b"));
        assert!(a.starts_with("claude_here:rust-u1000-user-"));
        assert_eq!(a.len(), "claude_here:rust-u1000-user-".len() + 8);
        assert_ne!(a, b);
        assert!(
            project_tag(&variant_tag("base", 1001), false, Path::new("/x"))
                .starts_with("claude_here:base-u1001-p-")
        );
        assert_ne!(variant_tag("base", 1000), variant_tag("base", 1001));
        let ad = Addons {
            node: true,
            uv: true,
        };
        assert_eq!(ad.suffix(), "-node-uv");
        assert_eq!(Addons::default().suffix(), "");
        assert!(!Addons::default().any());
    }

    #[test]
    fn variants_have_dockerfiles() {
        for v in VARIANTS {
            assert!(variant_dockerfile(v).is_some(), "{v}");
        }
        assert!(variant_dockerfile("golang").is_none());
    }
}
