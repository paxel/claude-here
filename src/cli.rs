//! Command line surface. Tool flags are separated from Claude's own flags by
//! a small pre-parser so that anything unknown is forwarded verbatim.

use clap::{Args, Parser, Subcommand};

use crate::config::{CloudMode, ConfigFile, GitMode, MountMode, MountSpec, NetMode};

/// Subcommand names recognized when given as the first argument.
pub const SUBCOMMANDS: &[&str] = &[
    "init",
    "build",
    "update",
    "config",
    "net",
    "toolchains",
    "completions",
    "uninstall",
];

/// Toolchain switches, shared by a run and `update`.
#[derive(Debug, Args, Default, Clone, PartialEq)]
pub struct ToolchainFlags {
    /// Enable a toolchain by name. Repeatable; same as the per-toolchain flags.
    #[arg(long = "toolchain", short = 't', value_name = "NAME")]
    pub toolchain: Vec<String>,
    /// rustup stable with clippy, rustfmt and rust-analyzer.
    #[arg(long, help_heading = "Toolchains")]
    pub rust: bool,
    /// GraalVM 21, Maven, Gradle, kotlinc, jdtls.
    #[arg(long, help_heading = "Toolchains")]
    pub jvm: bool,
    /// Flutter and Dart SDK (alias: --flutter).
    #[arg(long, visible_alias = "flutter", help_heading = "Toolchains")]
    pub dart: bool,
    /// Node.js, npm, pnpm, yarn, TypeScript (aliases: --npm, --js).
    #[arg(
        long,
        visible_alias = "npm",
        visible_alias = "js",
        help_heading = "Toolchains"
    )]
    pub node: bool,
    /// poetry, ruff, mypy, pyright (implies --node and --uv).
    #[arg(long, help_heading = "Toolchains")]
    pub python: bool,
    /// Go toolchain and gopls.
    #[arg(long, help_heading = "Toolchains")]
    pub go: bool,
    /// cmake, ninja, gdb, clang, clangd, valgrind, conan (alias: --c).
    #[arg(long, visible_alias = "c", help_heading = "Toolchains")]
    pub cpp: bool,
    /// uv/uvx, also how most MCP servers are launched.
    #[arg(long, help_heading = "Toolchains")]
    pub uv: bool,
    /// Android command line tools and platform-tools (implies --jvm).
    #[arg(long, help_heading = "Toolchains")]
    pub android: bool,
    /// plantuml, d2, typst, pandoc.
    #[arg(long, help_heading = "Toolchains")]
    pub docs: bool,
    /// kubectl, helm, kustomize (alias: --kubernetes).
    #[arg(long, visible_alias = "kubernetes", help_heading = "Toolchains")]
    pub k8s: bool,
    /// terraform.
    #[arg(long, help_heading = "Toolchains")]
    pub terraform: bool,
    /// AWS CLI v2.
    #[arg(long, help_heading = "Toolchains")]
    pub aws: bool,
    /// Google Cloud CLI (large).
    #[arg(long, help_heading = "Toolchains")]
    pub gcloud: bool,
    /// Azure CLI (alias: --az).
    #[arg(long, visible_alias = "az", help_heading = "Toolchains")]
    pub azure: bool,
}

impl ToolchainFlags {
    /// Toolchain names enabled by the per-toolchain flags and `--toolchain`.
    pub fn names(&self) -> Vec<String> {
        let mut v = self.toolchain.clone();
        for (on, name) in [
            (self.node, "node"),
            (self.uv, "uv"),
            (self.python, "python"),
            (self.jvm, "jvm"),
            (self.android, "android"),
            (self.rust, "rust"),
            (self.go, "go"),
            (self.cpp, "cpp"),
            (self.dart, "dart"),
            (self.docs, "docs"),
            (self.k8s, "k8s"),
            (self.terraform, "terraform"),
            (self.aws, "aws"),
            (self.gcloud, "gcloud"),
            (self.azure, "azure"),
        ] {
            if on {
                v.push(name.to_string());
            }
        }
        v
    }
}

/// Flags accepted in front of the forwarded Claude arguments.
#[derive(Debug, Parser, Default, Clone, PartialEq)]
#[command(
    name = "claude_here",
    version,
    about = "Run Claude Code in a project-scoped Docker sandbox",
    long_about = "Run Claude Code in a project-scoped Docker sandbox.\n\n\
Every argument that is not a claude_here flag is forwarded to `claude` inside the container.\n\
Use `--` to force forwarding. Subcommands: init, build, update, config, net, toolchains, completions, uninstall.",
    disable_help_subcommand = true
)]
pub struct RunFlags {
    /// Custom local image to run instead of building the toolchain chain.
    #[arg(long, value_name = "NAME")]
    pub image: Option<String>,
    #[command(flatten)]
    pub tc: ToolchainFlags,
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
    /// Cloud mode: none (default, no credentials), ro (read verbs only), full.
    #[arg(long, value_name = "MODE")]
    pub cloud: Option<CloudMode>,
    /// Network mode: full (default) or allowlist.
    #[arg(long, value_name = "MODE")]
    pub net: Option<NetMode>,
    /// Extra host allowed in net mode allowlist. Repeatable.
    #[arg(long = "net-allow", value_name = "HOST")]
    pub net_allow: Vec<String>,
    /// MCP server from the host configuration to grant here. Repeatable.
    #[arg(long, value_name = "NAME")]
    pub mcp: Vec<String>,
    /// Forward the ssh agent (git mode full only).
    #[arg(long)]
    pub ssh: bool,
    /// Hand the host gh token to the container (git mode full only).
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
    /// Allow claude_yolo together with git mode full or cloud mode full.
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
    /// Toolchain names enabled by the per-toolchain flags and `--toolchain`.
    pub fn toolchain_names(&self) -> Vec<String> {
        self.tc.names()
    }

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
            toolchains: self.toolchain_names(),
            git_mode: self.git,
            cloud_mode: self.cloud,
            net_mode: self.net,
            net_allow: self.net_allow.clone(),
            mcp: self.mcp.clone(),
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
        /// Custom image name instead of the toolchain chain.
        #[arg(long)]
        image: Option<String>,
        /// Toolchain to include. Repeatable.
        #[arg(long = "toolchain", short = 't', value_name = "NAME")]
        toolchain: Vec<String>,
        /// Rebuild everything.
        #[arg(long)]
        rebuild: bool,
    },
    /// Fetch the newest Claude Code release, accept it and rebuild the Claude
    /// layer of this project's image. Toolchain flags select the same image a
    /// run with them would use.
    Update {
        #[command(flatten)]
        tc: ToolchainFlags,
        /// Also refresh the operating system of the base image (`--no-cache
        /// --pull`); every image rebuilds on its next start.
        #[arg(long)]
        base: bool,
    },
    /// Show or change configuration.
    Config(ConfigArgs),
    /// Inspect recorded network activity.
    Net(NetArgs),
    /// List the shipped toolchains.
    Toolchains,
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
    "--toolchain",
    "-t",
    "--mount",
    "--mount-rw",
    "--env",
    "--git",
    "--cloud",
    "--net",
    "--net-allow",
    "--mcp",
    "--docker-arg",
    "--memory",
    "--cpus",
];
/// Tool flags without a value.
const BOOL_FLAGS: &[&str] = &[
    "--rust",
    "--jvm",
    "--dart",
    "--flutter",
    "--node",
    "--npm",
    "--js",
    "--python",
    "--go",
    "--cpp",
    "--c",
    "--uv",
    "--android",
    "--docs",
    "--k8s",
    "--kubernetes",
    "--terraform",
    "--aws",
    "--gcloud",
    "--azure",
    "--az",
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

    fn parse(tool: Vec<String>) -> RunFlags {
        let mut argv = vec!["claude_here".to_string()];
        argv.extend(tool);
        match RunFlags::try_parse_from(argv) {
            Ok(f) => f,
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn splits_tool_and_claude_args() {
        let s = split_args(v(&[
            "--rust",
            "--git=commit",
            "--ssh",
            "-p",
            "hello world",
            "--model",
            "opus",
        ]));
        assert_eq!(s.tool, v(&["--rust", "--git=commit", "--ssh"]));
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
    fn every_toolchain_has_a_flag_that_is_not_forwarded() {
        for t in crate::toolchain::TOOLCHAINS {
            let flag = format!("--{}", t.name);
            let s = split_args(v(&[&flag]));
            assert_eq!(s.tool, v(&[&flag]), "{flag} is forwarded to claude");
            assert!(
                parse(s.tool)
                    .toolchain_names()
                    .contains(&t.name.to_string())
            );
            for a in t.aliases {
                let flag = format!("--{a}");
                let s = split_args(v(&[&flag]));
                assert_eq!(s.tool, v(&[&flag]), "{flag} is forwarded to claude");
            }
        }
    }

    #[test]
    fn toolchain_flags_and_names_collect() {
        let s = split_args(v(&["--npm", "--uv", "-t", "docs", "-p", "x"]));
        assert_eq!(s.tool, v(&["--npm", "--uv", "-t", "docs"]));
        let f = parse(s.tool);
        assert!(f.tc.node && f.tc.uv);
        let mut names = f.toolchain_names();
        names.sort();
        assert_eq!(names, v(&["docs", "node", "uv"]));
        assert_eq!(f.as_config_layer().toolchains.len(), 3);
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
            "--cloud",
            "ro",
            "--net",
            "allowlist",
            "--net-allow",
            "example.com",
            "--mcp",
            "github",
        ]));
        let layer = parse(s.tool).as_config_layer();
        assert_eq!(layer.mounts.len(), 2);
        assert_eq!(layer.mounts[0].path, "~/a");
        assert_eq!(layer.mounts[0].mode, MountMode::Ro);
        assert_eq!(layer.mounts[1].target.as_deref(), Some("/data"));
        assert_eq!(layer.mounts[1].mode, MountMode::Rw);
        assert_eq!(layer.env, v(&["K=V", "T"]));
        assert_eq!(layer.docker_args, v(&["--network", "host"]));
        assert_eq!(layer.net_capture, Some(false));
        assert_eq!(layer.cloud_mode, Some(CloudMode::Ro));
        assert_eq!(layer.net_mode, Some(NetMode::Allowlist));
        assert_eq!(layer.net_allow, v(&["example.com"]));
        assert_eq!(layer.mcp, v(&["github"]));
    }
}
