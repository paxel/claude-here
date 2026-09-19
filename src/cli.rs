//! Command line surface. Tool flags are separated from Claude's own flags by
//! a small pre-parser so that anything unknown is forwarded verbatim.

use clap::{Args, Parser, Subcommand};

use crate::config::{ConfigFile, GitMode, MountMode, MountSpec};

/// Subcommand names recognized when given as the first argument.
pub const SUBCOMMANDS: &[&str] = &[
    "init",
    "build",
    "update",
    "config",
    "net",
    "completions",
    "uninstall",
];

/// Flags accepted in front of the forwarded Claude arguments.
#[derive(Debug, Parser, Default, Clone, PartialEq)]
#[command(
    name = "claude_here",
    version,
    about = "Run Claude Code in a project-scoped Docker sandbox",
    long_about = "Run Claude Code in a project-scoped Docker sandbox.\n\n\
Every argument that is not a claude_here flag is forwarded to `claude` inside the container.\n\
Use `--` to force forwarding. Subcommands: init, build, update, config, net, completions, uninstall.",
    disable_help_subcommand = true
)]
pub struct RunFlags {
    /// Image variant (base, rust, jvm) or a custom local image name.
    #[arg(long, value_name = "NAME")]
    pub image: Option<String>,
    /// Add Node.js with the latest npm/npx to the image (alias: --npm).
    #[arg(long, visible_alias = "npm")]
    pub node: bool,
    /// Add uv/uvx (fast Python package manager) to the image.
    #[arg(long)]
    pub uv: bool,
    /// Mount a host path read-only (`path` or `host:container`). Repeatable.
    #[arg(long, value_name = "PATH")]
    pub mount: Vec<String>,
    /// Mount a host path read-write (`path` or `host:container`). Repeatable.
    #[arg(long = "mount-rw", value_name = "PATH")]
    pub mount_rw: Vec<String>,
    /// Environment variable: `KEY=VALUE` literal or `KEY` to pass through from the host. Repeatable.
    #[arg(long, value_name = "KEY[=VALUE]")]
    pub env: Vec<String>,
    /// Git mode: ro (default, kernel enforced), commit, full.
    #[arg(long, value_name = "MODE")]
    pub git: Option<GitMode>,
    /// Forward the ssh agent (git mode full only).
    #[arg(long)]
    pub ssh: bool,
    /// Mount gh CLI config read-only (git mode full only).
    #[arg(long)]
    pub gh: bool,
    /// Raw docker run argument, appended before the image. Repeatable.
    #[arg(long = "docker-arg", value_name = "ARG", allow_hyphen_values = true)]
    pub docker_arg: Vec<String>,
    /// Container memory limit, e.g. 8g.
    #[arg(long, value_name = "SIZE")]
    pub memory: Option<String>,
    /// Container CPU limit, e.g. 4.
    #[arg(long, value_name = "N")]
    pub cpus: Option<f64>,
    /// Disable network capture for this run.
    #[arg(long = "no-net-log")]
    pub no_net_log: bool,
    /// Rebuild the whole image chain before running.
    #[arg(long)]
    pub rebuild: bool,
    /// Fail instead of building missing or stale images.
    #[arg(long = "no-build")]
    pub no_build: bool,
    /// Persist the given flags into the project config (.claude_here/config.toml).
    #[arg(long)]
    pub save: bool,
    /// Persist the given flags into the global config.
    #[arg(long = "save-global")]
    pub save_global: bool,
    /// Allow claude_yolo together with git mode full.
    #[arg(long = "i-know")]
    pub i_know: bool,
    /// Print the docker command instead of running it.
    #[arg(long = "dry-run")]
    pub dry_run: bool,
    /// Skip permission prompts (what claude_yolo does).
    #[arg(long, hide = true)]
    pub yolo: bool,
}

impl RunFlags {
    /// The CLI layer as a config overlay.
    pub fn as_config_layer(&self) -> ConfigFile {
        let mut env = self.env.clone();
        env.retain(|e| !e.is_empty());
        let mut mounts: Vec<MountSpec> = self
            .mount
            .iter()
            .map(|m| parse_mount(m, MountMode::Ro))
            .collect();
        mounts.extend(self.mount_rw.iter().map(|m| parse_mount(m, MountMode::Rw)));
        ConfigFile {
            image: self.image.clone(),
            node: self.node.then_some(true),
            uv: self.uv.then_some(true),
            git_mode: self.git,
            ssh: self.ssh.then_some(true),
            gh: self.gh.then_some(true),
            net_capture: self.no_net_log.then_some(false),
            memory: self.memory.clone(),
            cpus: self.cpus,
            env,
            docker_args: self.docker_arg.clone(),
            mounts,
            ..Default::default()
        }
    }
}

fn parse_mount(raw: &str, mode: MountMode) -> MountSpec {
    // `host:container` — but a lone path may not contain ':' in practice.
    match raw.split_once(':') {
        Some((h, c)) if !h.is_empty() && c.starts_with('/') => MountSpec {
            path: h.to_string(),
            mode,
            target: Some(c.to_string()),
        },
        _ => MountSpec {
            path: raw.to_string(),
            mode,
            target: None,
        },
    }
}

/// Top-level parser used when the first argument is a subcommand.
#[derive(Debug, Parser)]
#[command(
    name = "claude_here",
    version,
    about = "Run Claude Code in a project-scoped Docker sandbox"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create config, obtain a Claude token, seed the container home, install shell integration.
    Init(InitArgs),
    /// Build (or refresh) the image chain for the current project without running.
    Build {
        /// Image variant or custom image name.
        #[arg(long)]
        image: Option<String>,
        /// Rebuild everything.
        #[arg(long)]
        rebuild: bool,
    },
    /// Rebuild the base image without cache to pick up a new Claude Code release.
    Update,
    /// Show or change configuration.
    Config(ConfigArgs),
    /// Inspect recorded network activity.
    Net(NetArgs),
    /// Print shell completions.
    Completions {
        /// Shell: fish, bash, zsh.
        shell: clap_complete::Shell,
        /// Binary name to generate for (claude_here or claude_yolo).
        #[arg(long, default_value = "claude_here")]
        bin: String,
    },
    /// Remove images and optionally all configuration.
    Uninstall {
        /// Also delete ~/.config/claude_here (token, container home, logs).
        #[arg(long)]
        purge: bool,
        /// Do not ask for confirmation.
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Use this token instead of running `claude setup-token`.
    #[arg(long, value_name = "TOKEN")]
    pub token: Option<String>,
    /// Read the token from stdin.
    #[arg(long = "token-stdin")]
    pub token_stdin: bool,
    /// Skip token setup entirely.
    #[arg(long = "no-token")]
    pub no_token: bool,
    /// Comma separated list of items to seed from the host ~/.claude.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "settings.json,CLAUDE.md,skills,commands,plugins"
    )]
    pub seed: Vec<String>,
    /// Re-copy seed items even if the container home was seeded before.
    #[arg(long)]
    pub reseed: bool,
    /// Do not install fish completions.
    #[arg(long = "no-shell")]
    pub no_shell: bool,
    /// Answer yes to all questions.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the effective configuration and where each layer lives.
    Show,
    /// Set a key (dotted for tables, e.g. caches.isolated) in the project config.
    Set {
        key: String,
        value: String,
        /// Write to the global config instead.
        #[arg(long)]
        global: bool,
    },
    /// Print config file paths.
    Path,
}

#[derive(Debug, Args)]
pub struct NetArgs {
    #[command(subcommand)]
    pub action: NetAction,
}

#[derive(Debug, Subcommand)]
pub enum NetAction {
    /// Summary of the most recent session.
    Last,
    /// Summary of one session.
    Show { session_id: String },
    /// Hosts aggregated across sessions.
    Top {
        /// Look back this many days.
        #[arg(long, default_value_t = 30)]
        days: u32,
        /// Only sessions of the current project directory.
        #[arg(long)]
        project: bool,
        /// Number of rows.
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    /// Sessions that contacted a host (substring match).
    Grep { host: String },
    /// Open the capture of a session in termshark (runs inside the image).
    Shark { session_id: Option<String> },
    /// Delete sessions older than the retention period.
    Prune {
        /// Override retention in days.
        #[arg(long)]
        days: Option<u32>,
    },
    /// List recorded sessions.
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

/// Tool flags that take a value.
const VALUE_FLAGS: &[&str] = &[
    "--image",
    "--mount",
    "--mount-rw",
    "--env",
    "--git",
    "--docker-arg",
    "--memory",
    "--cpus",
];
/// Tool flags without a value.
const BOOL_FLAGS: &[&str] = &[
    "--node",
    "--npm",
    "--uv",
    "--ssh",
    "--gh",
    "--no-net-log",
    "--rebuild",
    "--no-build",
    "--save",
    "--save-global",
    "--i-know",
    "--dry-run",
    "--yolo",
];

/// Result of splitting argv.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Split {
    pub tool: Vec<String>,
    pub claude: Vec<String>,
    /// `-h`/`--help`/`-V`/`--version` seen outside `--`: show tool help/version.
    pub wants_help: bool,
    pub wants_version: bool,
}

/// Separate claude_here flags from arguments meant for `claude`.
pub fn split_args<I: IntoIterator<Item = String>>(args: I) -> Split {
    let mut out = Split::default();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--" {
            out.claude.extend(it);
            break;
        }
        if a == "-h" || a == "--help" {
            out.wants_help = true;
            continue;
        }
        if a == "-V" || a == "--version" {
            out.wants_version = true;
            continue;
        }
        if BOOL_FLAGS.contains(&a.as_str()) {
            out.tool.push(a);
            continue;
        }
        if VALUE_FLAGS.contains(&a.as_str()) {
            out.tool.push(a);
            if let Some(v) = it.next() {
                out.tool.push(v);
            }
            continue;
        }
        if let Some((k, _)) = a.split_once('=')
            && VALUE_FLAGS.contains(&k)
        {
            out.tool.push(a);
            continue;
        }
        out.claude.push(a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn splits_tool_and_claude_args() {
        let s = split_args(v(&[
            "--image",
            "rust",
            "--git=commit",
            "--ssh",
            "-p",
            "hello world",
            "--model",
            "opus",
        ]));
        assert_eq!(s.tool, v(&["--image", "rust", "--git=commit", "--ssh"]));
        assert_eq!(s.claude, v(&["-p", "hello world", "--model", "opus"]));
        assert!(!s.wants_help);
    }

    #[test]
    fn double_dash_forces_passthrough() {
        let s = split_args(v(&["--rebuild", "--", "--image", "x", "--help"]));
        assert_eq!(s.tool, v(&["--rebuild"]));
        assert_eq!(s.claude, v(&["--image", "x", "--help"]));
        assert!(!s.wants_help);
    }

    #[test]
    fn npm_alias_is_a_tool_flag() {
        let s = split_args(v(&["--npm", "--uv", "-p", "x"]));
        assert_eq!(s.tool, v(&["--npm", "--uv"]));
        let mut argv = vec!["claude_here".to_string()];
        argv.extend(s.tool);
        let f = RunFlags::try_parse_from(argv).map_err(|e| e.to_string());
        let Ok(f) = f else { panic!("parse") };
        assert!(f.node && f.uv);
        assert_eq!(f.as_config_layer().node, Some(true));
    }

    #[test]
    fn help_detected() {
        assert!(split_args(v(&["--help"])).wants_help);
        assert!(split_args(v(&["-p", "x", "-h"])).wants_help);
        assert!(split_args(v(&["--version"])).wants_version);
    }

    #[test]
    fn flags_parse_into_layer() {
        let s = split_args(v(&[
            "--mount",
            "~/a",
            "--mount-rw",
            "/srv/x:/data",
            "--env",
            "K=V",
            "--env",
            "T",
            "--docker-arg",
            "--network",
            "--docker-arg",
            "host",
            "--no-net-log",
        ]));
        let mut argv = vec!["claude_here".to_string()];
        argv.extend(s.tool);
        let f = RunFlags::try_parse_from(argv).map_err(|e| e.to_string());
        let f = match f {
            Ok(f) => f,
            Err(e) => panic!("{e}"),
        };
        let layer = f.as_config_layer();
        assert_eq!(layer.mounts.len(), 2);
        assert_eq!(layer.mounts[0].path, "~/a");
        assert_eq!(layer.mounts[0].mode, MountMode::Ro);
        assert_eq!(layer.mounts[1].target.as_deref(), Some("/data"));
        assert_eq!(layer.mounts[1].mode, MountMode::Rw);
        assert_eq!(layer.env, v(&["K=V", "T"]));
        assert_eq!(layer.docker_args, v(&["--network", "host"]));
        assert_eq!(layer.net_capture, Some(false));
    }
}
