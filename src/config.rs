//! TOML configuration: global (`~/.config/claude_here/config.toml`) is
//! overridden by project (`.claude_here/config.toml`), which is overridden
//! by CLI flags. Lists (mounts, env, docker_args) are concatenated in that
//! order.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Default container user name.
pub const DEFAULT_USER: &str = "ni";
/// Default image variant.
pub const DEFAULT_IMAGE: &str = "base";
/// Default network capture retention.
pub const DEFAULT_NET_RETENTION_DAYS: u32 = 90;

/// How much git is allowed to do inside the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GitMode {
    /// `.git` is bind-mounted read-only. Kernel enforced. No ssh/gh.
    #[default]
    Ro,
    /// `.git` writable, but the `git` shim allows only local, non-history-rewriting commands.
    Commit,
    /// No restriction.
    Full,
}

impl GitMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ro => "ro",
            Self::Commit => "commit",
            Self::Full => "full",
        }
    }
}

impl fmt::Display for GitMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GitMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "ro" => Ok(Self::Ro),
            "commit" => Ok(Self::Commit),
            "full" => Ok(Self::Full),
            other => bail!("unknown git mode '{other}' (expected ro, commit or full)"),
        }
    }
}

/// Mount access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MountMode {
    #[default]
    Ro,
    Rw,
}

/// An extra bind mount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountSpec {
    /// Host path (may start with `~`).
    pub path: String,
    #[serde(default)]
    pub mode: MountMode,
    /// Container path; defaults to the home-rewritten host path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Host cache directories mounted into the container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CachesFile {
    /// Use `~/.config/claude_here/cache/*` instead of host caches.
    pub isolated: Option<bool>,
    pub m2: Option<String>,
    pub gradle: Option<String>,
    pub cargo: Option<String>,
    /// File names inside `~/.m2` masked with an empty read-only file.
    pub m2_exclude: Option<Vec<String>>,
    /// File names inside `~/.gradle` masked with an empty read-only file.
    pub gradle_exclude: Option<Vec<String>>,
}

/// TLS related settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TlsFile {
    /// JKS/PKCS12 trust store mounted read-only and wired via `JAVA_TOOL_OPTIONS`.
    pub truststore: Option<String>,
    pub truststore_password: Option<String>,
}

/// On-disk representation. Every field optional so layers can be merged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigFile {
    pub image: Option<String>,
    pub user: Option<String>,
    pub git_mode: Option<GitMode>,
    pub ssh: Option<bool>,
    pub gh: Option<bool>,
    pub net_capture: Option<bool>,
    pub net_retention_days: Option<u32>,
    pub claude_version: Option<String>,
    pub memory: Option<String>,
    pub cpus: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub docker_args: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<MountSpec>,
    pub caches: CachesFile,
    pub tls: TlsFile,
}

impl ConfigFile {
    /// Load a file; a missing file is an empty layer.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Serialize and write, creating parent directories.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serializing config")?;
        fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// Overlay `other` on top of `self`: scalars from `other` win when set,
    /// lists are appended.
    #[must_use]
    pub fn merged_with(mut self, other: Self) -> Self {
        macro_rules! take {
            ($($field:ident),*) => { $( if other.$field.is_some() { self.$field = other.$field; } )* };
        }
        take!(
            image,
            user,
            git_mode,
            ssh,
            gh,
            net_capture,
            net_retention_days,
            claude_version,
            memory,
            cpus
        );
        self.env.extend(other.env);
        self.docker_args.extend(other.docker_args);
        self.mounts.extend(other.mounts);
        macro_rules! take_sub {
            ($sub:ident: $($field:ident),*) => { $( if other.$sub.$field.is_some() { self.$sub.$field = other.$sub.$field; } )* };
        }
        take_sub!(caches: isolated, m2, gradle, cargo, m2_exclude, gradle_exclude);
        take_sub!(tls: truststore, truststore_password);
        self
    }
}

/// Fully resolved configuration with defaults applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub image: String,
    pub user: String,
    pub git_mode: GitMode,
    pub ssh: bool,
    pub gh: bool,
    pub net_capture: bool,
    pub net_retention_days: u32,
    pub claude_version: String,
    pub memory: Option<String>,
    pub cpus: Option<f64>,
    pub env: Vec<String>,
    pub docker_args: Vec<String>,
    pub mounts: Vec<MountSpec>,
    pub caches_isolated: bool,
    pub cache_m2: String,
    pub cache_gradle: String,
    pub cache_cargo: String,
    pub m2_exclude: Vec<String>,
    pub gradle_exclude: Vec<String>,
    pub truststore: Option<String>,
    pub truststore_password: String,
}

impl From<ConfigFile> for Config {
    fn from(f: ConfigFile) -> Self {
        Self {
            image: f.image.unwrap_or_else(|| DEFAULT_IMAGE.to_string()),
            user: f.user.unwrap_or_else(|| DEFAULT_USER.to_string()),
            git_mode: f.git_mode.unwrap_or_default(),
            ssh: f.ssh.unwrap_or(false),
            gh: f.gh.unwrap_or(false),
            net_capture: f.net_capture.unwrap_or(true),
            net_retention_days: f.net_retention_days.unwrap_or(DEFAULT_NET_RETENTION_DAYS),
            claude_version: f.claude_version.unwrap_or_else(|| "latest".to_string()),
            memory: f.memory,
            cpus: f.cpus,
            env: f.env,
            docker_args: f.docker_args,
            mounts: f.mounts,
            caches_isolated: f.caches.isolated.unwrap_or(false),
            cache_m2: f.caches.m2.unwrap_or_else(|| "~/.m2".to_string()),
            cache_gradle: f.caches.gradle.unwrap_or_else(|| "~/.gradle".to_string()),
            cache_cargo: f.caches.cargo.unwrap_or_else(|| "~/.cargo".to_string()),
            m2_exclude: f.caches.m2_exclude.unwrap_or_default(),
            gradle_exclude: f.caches.gradle_exclude.unwrap_or_default(),
            truststore: f.tls.truststore,
            truststore_password: f
                .tls
                .truststore_password
                .unwrap_or_else(|| "changeit".to_string()),
        }
    }
}

/// The three configuration layers, kept separate so `--save` can target one.
#[derive(Debug, Clone)]
pub struct Layers {
    pub global_path: PathBuf,
    pub project_path: PathBuf,
    pub global: ConfigFile,
    pub project: ConfigFile,
}

impl Layers {
    pub fn load(global_path: PathBuf, project_path: PathBuf) -> Result<Self> {
        Ok(Self {
            global: ConfigFile::load(&global_path)?,
            project: ConfigFile::load(&project_path)?,
            global_path,
            project_path,
        })
    }

    /// Resolve global < project < `cli`.
    pub fn resolve(&self, cli: ConfigFile) -> Config {
        self.global
            .clone()
            .merged_with(self.project.clone())
            .merged_with(cli)
            .into()
    }
}

/// Set a dotted key (`caches.isolated`, `git_mode`) in a TOML file, creating
/// the file when missing. Values are parsed as TOML when possible, else
/// stored as strings.
pub fn set_key(path: &Path, key: &str, raw_value: &str) -> Result<()> {
    let mut table: toml::Table = match fs::read_to_string(path) {
        Ok(text) => text
            .parse()
            .with_context(|| format!("invalid config {}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let value = parse_value(raw_value);
    let mut parts = key.split('.').peekable();
    let mut current = &mut table;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.insert(part.to_string(), value);
            break;
        }
        let entry = current
            .entry(part.to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        current = entry
            .as_table_mut()
            .with_context(|| format!("config key '{part}' is not a table"))?;
    }
    // Validate against the schema before writing.
    let text = toml::to_string_pretty(&table).context("serializing config")?;
    let _: ConfigFile = toml::from_str(&text).context("value rejected by config schema")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn parse_value(raw: &str) -> toml::Value {
    if let Ok(b) = raw.parse::<bool>() {
        return toml::Value::Boolean(b);
    }
    if let Ok(i) = raw.parse::<i64>() {
        return toml::Value::Integer(i);
    }
    if let Ok(f) = raw.parse::<f64>() {
        return toml::Value::Float(f);
    }
    if raw.starts_with('[')
        && let Ok(v) = format!("v = {raw}").parse::<toml::Table>()
        && let Some(v) = v.get("v")
    {
        return v.clone();
    }
    toml::Value::String(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_apply() {
        let c: Config = ConfigFile::default().into();
        assert_eq!(c.image, "base");
        assert_eq!(c.user, "ni");
        assert_eq!(c.git_mode, GitMode::Ro);
        assert!(c.net_capture);
        assert_eq!(c.net_retention_days, 90);
        assert!(!c.ssh && !c.gh);
        assert_eq!(c.truststore_password, "changeit");
    }

    #[test]
    fn project_overrides_global_and_lists_append() {
        let global: ConfigFile = toml::from_str(
            r#"
image = "rust"
git_mode = "commit"
env = ["A=1"]
[[mounts]]
path = "~/g"
"#,
        )
        .unwrap_or_default();
        let project: ConfigFile = toml::from_str(
            r#"
image = "jvm"
env = ["B=2"]
[[mounts]]
path = "~/p"
mode = "rw"
[caches]
isolated = true
"#,
        )
        .unwrap_or_default();
        let layers = Layers {
            global_path: "/g".into(),
            project_path: "/p".into(),
            global,
            project,
        };
        let cli = ConfigFile {
            git_mode: Some(GitMode::Full),
            env: vec!["C=3".into()],
            ..Default::default()
        };
        let c = layers.resolve(cli);
        assert_eq!(c.image, "jvm");
        assert_eq!(c.git_mode, GitMode::Full);
        assert_eq!(c.env, vec!["A=1", "B=2", "C=3"]);
        assert_eq!(c.mounts.len(), 2);
        assert_eq!(c.mounts[1].mode, MountMode::Rw);
        assert!(c.caches_isolated);
    }

    #[test]
    fn unknown_keys_rejected() {
        let r: Result<ConfigFile, _> = toml::from_str("imgae = \"x\"");
        assert!(r.is_err());
    }

    #[test]
    fn git_mode_parses() {
        assert_eq!("ro".parse::<GitMode>().ok(), Some(GitMode::Ro));
        assert_eq!("commit".parse::<GitMode>().ok(), Some(GitMode::Commit));
        assert_eq!("full".parse::<GitMode>().ok(), Some(GitMode::Full));
        assert!("yolo".parse::<GitMode>().is_err());
    }

    #[test]
    fn set_key_creates_nested_and_validates() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        set_key(&path, "git_mode", "commit")?;
        set_key(&path, "caches.isolated", "true")?;
        set_key(&path, "env", r#"["A=1", "B"]"#)?;
        let f = ConfigFile::load(&path)?;
        assert_eq!(f.git_mode, Some(GitMode::Commit));
        assert_eq!(f.caches.isolated, Some(true));
        assert_eq!(f.env, vec!["A=1", "B"]);
        assert!(set_key(&path, "git_mode", "bogus").is_err());
        assert!(set_key(&path, "nope", "1").is_err());
        Ok(())
    }

    #[test]
    fn save_round_trips() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("sub").join("config.toml");
        let f = ConfigFile {
            image: Some("rust".into()),
            mounts: vec![MountSpec {
                path: "~/x".into(),
                mode: MountMode::Rw,
                target: Some("/opt/x".into()),
            }],
            ..Default::default()
        };
        f.save(&path)?;
        assert_eq!(ConfigFile::load(&path)?, f);
        Ok(())
    }
}
