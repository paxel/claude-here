//! TOML configuration: global (`~/.config/claude_here/config.toml`) is
//! overridden by project (`.claude_here/config.toml`), which is overridden
//! by CLI flags. Lists (toolchains, mounts, env, docker_args) are concatenated
//! in that order.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Default container user name.
pub const DEFAULT_USER: &str = "ni";
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

/// How much of a live cloud account the container may touch. Mirrors
/// `GitMode`: `none` mounts no credentials at all, `ro` mounts them behind
/// verb-allowlisting shims, `full` is unrestricted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CloudMode {
    /// No cloud credentials are mounted. Authoring and validating only.
    #[default]
    None,
    /// Credentials mounted; `kubectl`, `helm`, `terraform` and the vendor CLIs
    /// are shims that allow read verbs only.
    Ro,
    /// No restriction.
    Full,
}

impl CloudMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Ro => "ro",
            Self::Full => "full",
        }
    }
}

impl fmt::Display for CloudMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CloudMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "none" => Ok(Self::None),
            "ro" => Ok(Self::Ro),
            "full" => Ok(Self::Full),
            other => bail!("unknown cloud mode '{other}' (expected none, ro or full)"),
        }
    }
}

/// Whether egress is unrestricted or filtered by the in-container proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetMode {
    /// Unrestricted egress (still recorded).
    #[default]
    Full,
    /// Everything except DNS and the allowlisting proxy is dropped.
    Allowlist,
}

impl NetMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Allowlist => "allowlist",
        }
    }
}

impl fmt::Display for NetMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for NetMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "full" => Ok(Self::Full),
            "allowlist" => Ok(Self::Allowlist),
            other => bail!("unknown net mode '{other}' (expected full or allowlist)"),
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

/// Host cache directories mounted into the container. Every key corresponds to
/// a `toolchain::Cache` key; unset keys use the toolchain's default path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct CachesFile {
    /// Use `~/.config/claude_here/cache/*` instead of host caches.
    pub isolated: Option<bool>,
    pub m2: Option<String>,
    pub gradle: Option<String>,
    pub cargo: Option<String>,
    pub npm: Option<String>,
    pub uv: Option<String>,
    pub pip: Option<String>,
    pub go: Option<String>,
    #[serde(rename = "pub")]
    pub pub_cache: Option<String>,
    pub conan: Option<String>,
    pub android: Option<String>,
    /// File names inside `~/.m2` masked with an empty read-only file.
    pub m2_exclude: Option<Vec<String>>,
    /// File names inside `~/.gradle` masked with an empty read-only file.
    pub gradle_exclude: Option<Vec<String>>,
}

impl CachesFile {
    /// Configured host path for a `toolchain::Cache` key, if any.
    pub fn override_for(&self, key: &str) -> Option<&str> {
        match key {
            "m2" => self.m2.as_deref(),
            "gradle" => self.gradle.as_deref(),
            "cargo" => self.cargo.as_deref(),
            "npm" => self.npm.as_deref(),
            "uv" => self.uv.as_deref(),
            "pip" => self.pip.as_deref(),
            "go" => self.go.as_deref(),
            "pub" => self.pub_cache.as_deref(),
            "conan" => self.conan.as_deref(),
            "android" => self.android.as_deref(),
            _ => None,
        }
    }
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
    /// Custom local image, bypassing the toolchain chain entirely.
    pub image: Option<String>,
    pub user: Option<String>,
    /// Shipped toolchains to layer on the base image, in any order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub toolchains: Vec<String>,
    pub git_mode: Option<GitMode>,
    pub cloud_mode: Option<CloudMode>,
    pub net_mode: Option<NetMode>,
    pub ssh: Option<bool>,
    pub gh: Option<bool>,
    pub net_capture: Option<bool>,
    pub net_retention_days: Option<u32>,
    pub claude_version: Option<String>,
    pub memory: Option<String>,
    pub cpus: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    /// Extra hosts allowed when `net_mode` is `allowlist`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub net_allow: Vec<String>,
    /// MCP servers from the host configuration that may be used here. Nothing
    /// is inherited: an MCP server is a granted capability (ADR 0007).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub docker_args: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<MountSpec>,
    pub caches: CachesFile,
    pub tls: TlsFile,
    /// Superseded by `toolchains = ["node"]`. Kept so existing configs load.
    pub node: Option<bool>,
    /// Superseded by `toolchains = ["uv"]`. Kept so existing configs load.
    pub uv: Option<bool>,
}

/// Names of the toolchains that used to be image variants.
const LEGACY_VARIANTS: &[&str] = &["rust", "jvm"];

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
            cloud_mode,
            net_mode,
            ssh,
            gh,
            net_capture,
            net_retention_days,
            claude_version,
            memory,
            cpus,
            node,
            uv
        );
        self.toolchains.extend(other.toolchains);
        self.env.extend(other.env);
        self.net_allow.extend(other.net_allow);
        self.mcp.extend(other.mcp);
        self.docker_args.extend(other.docker_args);
        self.mounts.extend(other.mounts);
        macro_rules! take_sub {
            ($sub:ident: $($field:ident),*) => { $( if other.$sub.$field.is_some() { self.$sub.$field = other.$sub.$field; } )* };
        }
        take_sub!(
            caches: isolated,
            m2,
            gradle,
            cargo,
            npm,
            uv,
            pip,
            go,
            pub_cache,
            conan,
            android,
            m2_exclude,
            gradle_exclude
        );
        take_sub!(tls: truststore, truststore_password);
        self
    }
}

/// Fully resolved configuration with defaults applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Custom image; `None` means build the toolchain chain.
    pub image: Option<String>,
    pub user: String,
    pub toolchains: Vec<String>,
    pub git_mode: GitMode,
    pub cloud_mode: CloudMode,
    pub net_mode: NetMode,
    pub ssh: bool,
    pub gh: bool,
    pub net_capture: bool,
    pub net_retention_days: u32,
    pub claude_version: String,
    pub memory: Option<String>,
    pub cpus: Option<f64>,
    pub env: Vec<String>,
    pub net_allow: Vec<String>,
    pub mcp: Vec<String>,
    pub docker_args: Vec<String>,
    pub mounts: Vec<MountSpec>,
    pub caches: CachesFile,
    pub caches_isolated: bool,
    pub m2_exclude: Vec<String>,
    pub gradle_exclude: Vec<String>,
    pub truststore: Option<String>,
    pub truststore_password: String,
    /// Notes about superseded configuration keys that were translated.
    pub legacy_notes: Vec<String>,
}

/// Translate the pre-0.1.0 keys: `image = "base"|"rust"|"jvm"` selected a
/// variant, `node`/`uv` were switches. All three are toolchains now.
fn migrate_legacy(f: &ConfigFile) -> (Option<String>, Vec<String>, Vec<String>) {
    let mut toolchains = f.toolchains.clone();
    let mut notes = Vec::new();
    let mut image = f.image.clone();
    if let Some(name) = f.image.as_deref() {
        if name == "base" {
            notes.push(
                "image = \"base\" is obsolete; the base image is the default. Remove the key"
                    .into(),
            );
            image = None;
        } else if LEGACY_VARIANTS.contains(&name) {
            notes.push(format!(
                "image = \"{name}\" is obsolete; using toolchains = [\"{name}\"] instead. Update the config or run `claude_here --{name} --save-global`"
            ));
            toolchains.push(name.to_string());
            image = None;
        }
    }
    for (set, name) in [(f.node, "node"), (f.uv, "uv")] {
        if set == Some(true) {
            notes.push(format!(
                "{name} = true is obsolete; using toolchains = [\"{name}\"] instead"
            ));
            toolchains.push(name.to_string());
        }
    }
    (image, toolchains, notes)
}

impl From<ConfigFile> for Config {
    fn from(f: ConfigFile) -> Self {
        let (image, toolchains, legacy_notes) = migrate_legacy(&f);
        Self {
            image,
            user: f.user.unwrap_or_else(|| DEFAULT_USER.to_string()),
            toolchains,
            git_mode: f.git_mode.unwrap_or_default(),
            cloud_mode: f.cloud_mode.unwrap_or_default(),
            net_mode: f.net_mode.unwrap_or_default(),
            ssh: f.ssh.unwrap_or(false),
            gh: f.gh.unwrap_or(false),
            net_capture: f.net_capture.unwrap_or(true),
            net_retention_days: f.net_retention_days.unwrap_or(DEFAULT_NET_RETENTION_DAYS),
            claude_version: f.claude_version.unwrap_or_else(|| "latest".to_string()),
            memory: f.memory,
            cpus: f.cpus,
            env: f.env,
            net_allow: f.net_allow,
            mcp: f.mcp,
            docker_args: f.docker_args,
            mounts: f.mounts,
            caches_isolated: f.caches.isolated.unwrap_or(false),
            m2_exclude: f.caches.m2_exclude.clone().unwrap_or_default(),
            gradle_exclude: f.caches.gradle_exclude.clone().unwrap_or_default(),
            caches: f.caches,
            truststore: f.tls.truststore,
            truststore_password: f
                .tls
                .truststore_password
                .unwrap_or_else(|| "changeit".to_string()),
            legacy_notes,
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
        assert_eq!(c.image, None);
        assert!(c.toolchains.is_empty());
        assert_eq!(c.user, "ni");
        assert_eq!(c.git_mode, GitMode::Ro);
        assert_eq!(c.cloud_mode, CloudMode::None);
        assert_eq!(c.net_mode, NetMode::Full);
        assert!(c.net_capture);
        assert_eq!(c.net_retention_days, 90);
        assert!(!c.ssh && !c.gh);
        assert!(c.mcp.is_empty());
        assert_eq!(c.truststore_password, "changeit");
    }

    #[test]
    fn project_overrides_global_and_lists_append() {
        let global: ConfigFile = toml::from_str(
            r#"
toolchains = ["rust"]
git_mode = "commit"
env = ["A=1"]
[[mounts]]
path = "~/g"
"#,
        )
        .unwrap_or_default();
        let project: ConfigFile = toml::from_str(
            r#"
toolchains = ["jvm"]
cloud_mode = "ro"
env = ["B=2"]
mcp = ["github"]
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
            toolchains: vec!["docs".into()],
            ..Default::default()
        };
        let c = layers.resolve(cli);
        assert_eq!(c.toolchains, vec!["rust", "jvm", "docs"]);
        assert_eq!(c.git_mode, GitMode::Full);
        assert_eq!(c.cloud_mode, CloudMode::Ro);
        assert_eq!(c.env, vec!["A=1", "B=2", "C=3"]);
        assert_eq!(c.mcp, vec!["github"]);
        assert_eq!(c.mounts.len(), 2);
        assert_eq!(c.mounts[1].mode, MountMode::Rw);
        assert!(c.caches_isolated);
    }

    #[test]
    fn legacy_keys_become_toolchains() {
        let f: ConfigFile =
            toml::from_str("node = true\nuv = true\nimage = \"rust\"\n").unwrap_or_default();
        let c: Config = f.into();
        assert_eq!(c.image, None);
        assert_eq!(c.toolchains, vec!["rust", "node", "uv"]);
        assert_eq!(c.legacy_notes.len(), 3);

        let c: Config = toml::from_str::<ConfigFile>("image = \"base\"\n")
            .unwrap_or_default()
            .into();
        assert_eq!(c.image, None);
        assert!(c.toolchains.is_empty());
        assert_eq!(c.legacy_notes.len(), 1);

        // A custom image keeps its meaning and produces no note.
        let c: Config = toml::from_str::<ConfigFile>("image = \"my/dev:latest\"\n")
            .unwrap_or_default()
            .into();
        assert_eq!(c.image.as_deref(), Some("my/dev:latest"));
        assert!(c.legacy_notes.is_empty());
    }

    #[test]
    fn unknown_keys_rejected() {
        let r: Result<ConfigFile, _> = toml::from_str("imgae = \"x\"");
        assert!(r.is_err());
        let r: Result<ConfigFile, _> = toml::from_str("[caches]\nnope = \"x\"");
        assert!(r.is_err());
    }

    #[test]
    fn modes_parse() {
        assert_eq!("ro".parse::<GitMode>().ok(), Some(GitMode::Ro));
        assert_eq!("commit".parse::<GitMode>().ok(), Some(GitMode::Commit));
        assert_eq!("full".parse::<GitMode>().ok(), Some(GitMode::Full));
        assert!("yolo".parse::<GitMode>().is_err());
        assert_eq!("none".parse::<CloudMode>().ok(), Some(CloudMode::None));
        assert_eq!("ro".parse::<CloudMode>().ok(), Some(CloudMode::Ro));
        assert!("commit".parse::<CloudMode>().is_err());
        assert_eq!("full".parse::<NetMode>().ok(), Some(NetMode::Full));
        assert_eq!(
            "allowlist".parse::<NetMode>().ok(),
            Some(NetMode::Allowlist)
        );
        assert!("none".parse::<NetMode>().is_err());
    }

    #[test]
    fn cache_overrides_are_looked_up_by_key() {
        let c = CachesFile {
            m2: Some("/opt/m2".into()),
            pub_cache: Some("/opt/pub".into()),
            ..Default::default()
        };
        assert_eq!(c.override_for("m2"), Some("/opt/m2"));
        assert_eq!(c.override_for("pub"), Some("/opt/pub"));
        assert_eq!(c.override_for("cargo"), None);
        assert_eq!(c.override_for("nope"), None);
    }

    #[test]
    fn set_key_creates_nested_and_validates() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        set_key(&path, "git_mode", "commit")?;
        set_key(&path, "caches.isolated", "true")?;
        set_key(&path, "env", r#"["A=1", "B"]"#)?;
        set_key(&path, "toolchains", r#"["rust", "docs"]"#)?;
        let f = ConfigFile::load(&path)?;
        assert_eq!(f.git_mode, Some(GitMode::Commit));
        assert_eq!(f.caches.isolated, Some(true));
        assert_eq!(f.env, vec!["A=1", "B"]);
        assert_eq!(f.toolchains, vec!["rust", "docs"]);
        assert!(set_key(&path, "git_mode", "bogus").is_err());
        assert!(set_key(&path, "nope", "1").is_err());
        Ok(())
    }

    #[test]
    fn save_round_trips() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("sub").join("config.toml");
        let f = ConfigFile {
            toolchains: vec!["rust".into()],
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
