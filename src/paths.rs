//! Host-side directory layout and host→container path rewriting.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Directory name used below `~/.config` and inside projects.
pub const TOOL_DIR_NAME: &str = "claude_here";
/// Per-project configuration directory name.
pub const PROJECT_DIR_NAME: &str = ".claude_here";

/// Resolved host-side locations used by the tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPaths {
    /// The user's home directory on the host.
    pub home: PathBuf,
    /// `~/.config/claude_here`.
    pub config_dir: PathBuf,
}

impl HostPaths {
    /// Resolve from environment (`HOME`, `XDG_CONFIG_HOME`).
    pub fn from_env() -> Result<Self> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .context("HOME is not set or not absolute")?;
        let config_base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        Ok(Self::new(home, config_base.join(TOOL_DIR_NAME)))
    }

    /// Build from explicit values (tests).
    pub fn new(home: PathBuf, config_dir: PathBuf) -> Self {
        Self { home, config_dir }
    }

    /// Global `config.toml`.
    pub fn global_config(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// Persistent container-side `~/.claude` directory.
    pub fn container_home(&self) -> PathBuf {
        self.config_dir.join("home")
    }

    /// File holding `CLAUDE_CODE_OAUTH_TOKEN=...` in docker `--env-file` format.
    pub fn token_file(&self) -> PathBuf {
        self.config_dir.join("token")
    }

    /// Global user Dockerfile layer.
    pub fn user_dockerfile(&self) -> PathBuf {
        self.config_dir.join("Dockerfile")
    }

    /// Where network captures and summaries land.
    pub fn net_log_dir(&self) -> PathBuf {
        self.config_dir.join("logs").join("net")
    }

    /// Isolated cache root used when `caches.isolated = true`.
    /// Generated plugin directory (skills + language server declarations).
    pub fn plugin_dir(&self) -> PathBuf {
        self.config_dir.join("plugin")
    }

    /// Per-session directory the container writes its capture into. Only this
    /// directory is mounted, so a session can never read another session's
    /// capture (ADR 0008).
    pub fn net_out_dir(&self, session_id: &str) -> PathBuf {
        self.config_dir.join("logs").join("out").join(session_id)
    }

    /// Per-run directory holding the network summaries this project may read.
    pub fn net_view_dir(&self) -> PathBuf {
        self.config_dir.join("net-view")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.config_dir.join("cache")
    }

    /// Expand a leading `~` or `~/` to the host home directory.
    pub fn expand_tilde(&self, raw: &str) -> PathBuf {
        if raw == "~" {
            return self.home.clone();
        }
        if let Some(rest) = raw.strip_prefix("~/") {
            return self.home.join(rest);
        }
        PathBuf::from(raw)
    }
}

/// Rewrites host paths under the host home to the container home.
///
/// `/home/axel/data/x` becomes `/home/ni/data/x`; paths outside the host home
/// are returned unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRewriter {
    host_home: PathBuf,
    container_home: PathBuf,
}

impl PathRewriter {
    pub fn new(host_home: impl Into<PathBuf>, container_user: &str) -> Self {
        Self {
            host_home: host_home.into(),
            container_home: PathBuf::from("/home").join(container_user),
        }
    }

    /// Container home directory (`/home/<user>`).
    pub fn container_home(&self) -> &Path {
        &self.container_home
    }

    /// Rewrite a single absolute host path.
    pub fn rewrite(&self, host_path: &Path) -> PathBuf {
        match host_path.strip_prefix(&self.host_home) {
            Ok(rel) => self.container_home.join(rel),
            Err(_) => host_path.to_path_buf(),
        }
    }

    /// Rewrite every occurrence of the host home prefix inside text content
    /// (used for seeded config files that embed absolute paths).
    pub fn rewrite_text(&self, text: &str) -> String {
        let host = self.host_home.to_string_lossy();
        let container = self.container_home.to_string_lossy();
        if host == container {
            return text.to_string();
        }
        text.replace(host.as_ref(), container.as_ref())
    }
}

/// Locate the project directory (`.claude_here`) for a working directory.
pub fn project_dir(cwd: &Path) -> PathBuf {
    cwd.join(PROJECT_DIR_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewriter() -> PathRewriter {
        PathRewriter::new("/home/axel", "ni")
    }

    #[test]
    fn rewrites_paths_under_home() {
        let out = rewriter().rewrite(Path::new("/home/axel/data/development/foo"));
        assert_eq!(out, PathBuf::from("/home/ni/data/development/foo"));
    }

    #[test]
    fn keeps_paths_outside_home() {
        let out = rewriter().rewrite(Path::new("/srv/repo"));
        assert_eq!(out, PathBuf::from("/srv/repo"));
    }

    #[test]
    fn does_not_rewrite_sibling_prefix() {
        // `/home/axel2` must not be treated as inside `/home/axel`.
        let out = rewriter().rewrite(Path::new("/home/axel2/x"));
        assert_eq!(out, PathBuf::from("/home/axel2/x"));
    }

    #[test]
    fn home_itself_maps_to_container_home() {
        assert_eq!(
            rewriter().rewrite(Path::new("/home/axel")),
            PathBuf::from("/home/ni")
        );
    }

    #[test]
    fn rewrites_text_occurrences() {
        let text = r#"{"cmd":"/home/axel/.claude/plugins/x","other":"/opt/y"}"#;
        assert_eq!(
            rewriter().rewrite_text(text),
            r#"{"cmd":"/home/ni/.claude/plugins/x","other":"/opt/y"}"#
        );
    }

    #[test]
    fn same_user_text_is_untouched() {
        let r = PathRewriter::new("/home/axel", "axel");
        assert_eq!(r.rewrite_text("/home/axel/x"), "/home/axel/x");
    }

    #[test]
    fn expands_tilde() {
        let hp = HostPaths::new("/home/axel".into(), "/home/axel/.config/claude_here".into());
        assert_eq!(hp.expand_tilde("~/.m2"), PathBuf::from("/home/axel/.m2"));
        assert_eq!(hp.expand_tilde("~"), PathBuf::from("/home/axel"));
        assert_eq!(hp.expand_tilde("/abs"), PathBuf::from("/abs"));
        assert_eq!(hp.expand_tilde("~x"), PathBuf::from("~x"));
    }
}
