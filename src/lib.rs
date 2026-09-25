//! claude_here: run Claude Code inside a project-scoped Docker sandbox.

pub mod claude_update;
pub mod cli;
pub mod config;
pub mod docker;
pub mod git;
pub mod image;
pub mod init;
pub mod net;
pub mod paths;
pub mod plugin;
pub mod run;
pub mod session;
pub mod toolchain;
pub mod update;

use std::io::Write;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser};

use cli::{Cli, Command, ConfigAction, NetAction, RunFlags, SUBCOMMANDS, split_args};
use config::{ConfigFile, Layers};
use image::BuildPolicy;
use paths::{HostPaths, project_dir};

/// Entry point shared by both binaries.
pub fn main_entry(yolo_binary: bool) -> ExitCode {
    match dispatch(yolo_binary) {
        Ok(code) => ExitCode::from(u8::try_from(code.clamp(0, 255)).unwrap_or(1)),
        Err(e) => {
            eprintln!("claude_here: error: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn dispatch(yolo_binary: bool) -> Result<i32> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let paths = HostPaths::from_env()?;
    if argv
        .first()
        .is_some_and(|a| SUBCOMMANDS.contains(&a.as_str()))
    {
        let mut full = vec!["claude_here".to_string()];
        full.extend(argv);
        let cli = Cli::parse_from(full);
        return run_subcommand(&paths, cli.command).map(|()| 0);
    }
    let split = split_args(argv);
    if split.wants_help {
        RunFlags::command().print_long_help()?;
        println!();
        return Ok(0);
    }
    if split.wants_version {
        println!("claude_here {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    let mut tool = vec!["claude_here".to_string()];
    tool.extend(split.tool);
    let flags = RunFlags::parse_from(tool);
    let yolo = yolo_binary || flags.yolo;

    let cwd = std::env::current_dir().context("reading current directory")?;
    let layers = Layers::load(paths.global_config(), project_dir(&cwd).join("config.toml"))?;
    let cli_layer = flags.as_config_layer();
    if flags.save || flags.save_global {
        save_layer(&layers, &cli_layer, flags.save_global)?;
    }
    let cfg = layers.resolve(cli_layer);
    // Decided from the cache, so nothing is fetched before a session starts.
    // A successful update replaces this binary, so re-exec with the same
    // arguments instead of running the session with the old one.
    if !flags.dry_run
        && update::prompt_and_update(&paths, update::on_a_terminal(), cfg.update_check)
    {
        update::reexec();
    }
    // Asked after the tool's own update, which re-execs. A custom image has no
    // Claude layer, and --no-build could not act on a "y".
    if !flags.dry_run && !flags.no_build && cfg.image.is_none() {
        claude_update::prompt(&paths, &cfg, update::on_a_terminal());
    }
    let policy = BuildPolicy {
        force: flags.rebuild,
        no_build: flags.no_build,
        refresh_base: false,
    };
    if flags.dry_run {
        return dry_run(&cfg, &paths, yolo, flags.i_know, split.claude);
    }
    run::execute(&cfg, &paths, policy, yolo, flags.i_know, split.claude)
}

fn save_layer(layers: &Layers, cli_layer: &ConfigFile, global: bool) -> Result<()> {
    let (path, existing) = if global {
        (&layers.global_path, layers.global.clone())
    } else {
        (&layers.project_path, layers.project.clone())
    };
    let mut merged = existing.merged_with(cli_layer.clone());
    // Saving `--rust` twice must not list it twice.
    let mut seen = std::collections::HashSet::new();
    merged.toolchains.retain(|t| seen.insert(t.clone()));
    merged.save(path)?;
    eprintln!("claude_here: saved to {}", path.display());
    Ok(())
}

fn dry_run(
    cfg: &config::Config,
    paths: &HostPaths,
    yolo: bool,
    i_know: bool,
    claude_args: Vec<String>,
) -> Result<i32> {
    let facts = run::HostFacts::gather(paths, cfg)?;
    let git = git::scan(&facts.cwd)?;
    let chain = toolchain::resolve(&cfg.toolchains)?;
    let req = run::RunRequest {
        yolo,
        i_know,
        claude_args,
        image_tag: run::planned_tag(cfg, paths, &facts)?,
        session_id: session::new_session_id(),
        toolchains: chain,
        mcp: cfg.mcp.clone(),
    };
    let a = run::assemble(cfg, paths, &facts, &git, &req)?;
    for w in &a.warnings {
        eprintln!("note: {w}");
    }
    let mut out = std::io::stdout().lock();
    write!(out, "docker")?;
    for arg in a.spec.to_args() {
        if arg.contains(' ') || arg.contains('"') {
            write!(out, " '{}'", arg.replace('\'', "'\\''"))?;
        } else {
            write!(out, " {arg}")?;
        }
    }
    writeln!(out)?;
    Ok(0)
}

fn run_subcommand(paths: &HostPaths, command: Command) -> Result<()> {
    match command {
        Command::Init(args) => init::run(paths, &args),
        Command::Build {
            image,
            toolchain,
            rebuild,
        } => {
            let cfg = load_config(
                paths,
                ConfigFile {
                    image,
                    toolchains: toolchain,
                    ..Default::default()
                },
            )?;
            let tag = run::build_only(
                &cfg,
                paths,
                BuildPolicy {
                    force: rebuild,
                    ..Default::default()
                },
            )?;
            println!("{tag}");
            Ok(())
        }
        Command::Update { tc, base } => {
            let cfg = load_config(
                paths,
                ConfigFile {
                    toolchains: tc.names(),
                    ..Default::default()
                },
            )?;
            let version = claude_update::update_now(paths, &cfg)?;
            if claude_update::channel(&cfg.claude_version).is_some() {
                println!("claude: {version} accepted");
            } else {
                println!("claude: pinned to {version} (claude_version)");
            }
            let built = run::build_reporting(
                &cfg,
                paths,
                BuildPolicy {
                    force: false,
                    no_build: false,
                    refresh_base: base,
                },
            )?;
            println!(
                "claude: {} -> {}  ({})",
                built.claude_before.as_deref().unwrap_or("(none)"),
                built.claude_after.as_deref().unwrap_or("(unknown)"),
                built.tag
            );
            // `update` is when someone is thinking about versions anyway.
            update::refresh_if_stale(paths, cfg.update_check);
            if let Some(latest) = update::pending(paths) {
                let exe = std::env::current_exe().unwrap_or_default();
                println!(
                    "claude_here: {} -> {latest} available  ({})",
                    update::current(),
                    update::install_method(&exe).hint()
                );
            }
            Ok(())
        }
        Command::Config(args) => config_command(paths, args.action),
        Command::Toolchains => {
            print_toolchains();
            Ok(())
        }
        Command::Net(args) => net_command(paths, args.action),
        Command::Completions { shell, bin } => {
            print!("{}", completions_text(shell, &bin));
            Ok(())
        }
        Command::Uninstall { purge, yes } => uninstall(paths, purge, yes),
    }
}

fn load_config(paths: &HostPaths, cli_layer: ConfigFile) -> Result<config::Config> {
    let cwd = std::env::current_dir().context("reading current directory")?;
    let layers = Layers::load(paths.global_config(), project_dir(&cwd).join("config.toml"))?;
    Ok(layers.resolve(cli_layer))
}

fn config_command(paths: &HostPaths, action: ConfigAction) -> Result<()> {
    let cwd = std::env::current_dir().context("reading current directory")?;
    let project_path = project_dir(&cwd).join("config.toml");
    match action {
        ConfigAction::Show => {
            let layers = Layers::load(paths.global_config(), project_path)?;
            println!("# global:  {}", layers.global_path.display());
            println!("# project: {}", layers.project_path.display());
            let merged = layers.global.clone().merged_with(layers.project.clone());
            print!("{}", toml::to_string_pretty(&merged)?);
            let effective = layers.resolve(ConfigFile::default());
            for note in &effective.legacy_notes {
                println!("# obsolete: {note}");
            }
            let chain = toolchain::resolve(&effective.toolchains)?;
            println!(
                "# effective: image={} toolchains={} user={} git_mode={} cloud_mode={} net_mode={} net_capture={}",
                effective.image.as_deref().unwrap_or("(chain)"),
                if chain.is_empty() {
                    "(none)".to_string()
                } else {
                    toolchain::names(&chain).join(",")
                },
                effective.user,
                effective.git_mode,
                effective.cloud_mode,
                effective.net_mode,
                effective.net_capture
            );
            Ok(())
        }
        ConfigAction::Set { key, value, global } => {
            let path = if global {
                paths.global_config()
            } else {
                project_path
            };
            config::set_key(&path, &key, &value)?;
            println!("{}: {key} = {value}", path.display());
            Ok(())
        }
        ConfigAction::Path => {
            println!("{}", paths.global_config().display());
            println!("{}", project_path.display());
            Ok(())
        }
    }
}

fn net_command(paths: &HostPaths, action: NetAction) -> Result<()> {
    let cfg = load_config(paths, ConfigFile::default())?;
    let dir = paths.net_log_dir();
    match action {
        NetAction::Last => {
            let Some(files) = net::list_sessions(&dir).into_iter().next() else {
                bail!("no recorded sessions");
            };
            net::report::print_session(&files)
        }
        NetAction::Show { session_id } => {
            net::report::print_session(&net::SessionFiles::new(&dir, &session_id))
        }
        NetAction::Top {
            days,
            project,
            limit,
        } => {
            let cwd = std::env::current_dir()?;
            net::report::print_top(&dir, days, project.then_some(cwd.as_path()), limit)
        }
        NetAction::Grep { host } => net::report::print_grep(&dir, &host),
        NetAction::Shark { session_id } => {
            let id = match session_id {
                Some(id) => id,
                None => net::list_sessions(&dir)
                    .into_iter()
                    .next()
                    .map(|f| f.session_id)
                    .context("no recorded sessions")?,
            };
            net::report::shark(paths, &cfg, &id)
        }
        NetAction::Prune { days } => {
            let n = net::prune(paths, days.unwrap_or(cfg.net_retention_days))?;
            println!("removed {n} session(s)");
            Ok(())
        }
        NetAction::List { limit } => net::report::print_list(&dir, limit),
    }
}

/// One line per shipped toolchain, for `claude_here toolchains`.
fn print_toolchains() {
    // Chain order, so the listing matches how the image is layered: expensive
    // toolchains first, at the bottom, where nothing is inserted below them.
    let mut all: Vec<&toolchain::Toolchain> = toolchain::TOOLCHAINS.iter().collect();
    all.sort_by_key(|t| t.order);
    for t in all {
        let mut flags = format!("--{}", t.name);
        for a in t.aliases {
            flags.push_str(", --");
            flags.push_str(a);
        }
        let implies = if t.implies.is_empty() {
            String::new()
        } else {
            format!("  (implies {})", t.implies.join(", "))
        };
        println!("{flags:<28} {}{implies}", t.description);
    }
}

/// Shell completion script for one of the binaries.
pub fn completions_text(shell: clap_complete::Shell, bin: &str) -> String {
    let mut cmd = RunFlags::command()
        .name(bin.to_string())
        .subcommands(Cli::command().get_subcommands().cloned());
    let mut buf = Vec::new();
    clap_complete::generate(shell, &mut cmd, bin, &mut buf);
    String::from_utf8_lossy(&buf).to_string()
}

fn uninstall(paths: &HostPaths, purge: bool, yes: bool) -> Result<()> {
    let docker = docker::Docker::default();
    let images = docker
        .output(&[
            "images",
            "--format",
            "{{.Repository}}:{{.Tag}}",
            image::REPO,
        ])
        .unwrap_or_default();
    // Only this host user's chain; other users on the same daemon keep theirs.
    let (uid, _) = run::process_ids()?;
    let marker = format!("-u{uid}");
    let tags: Vec<&str> = images
        .lines()
        .filter(|l| !l.is_empty() && (l.contains(&format!("{marker}-")) || l.ends_with(&marker)))
        .collect();
    if !yes {
        println!(
            "will remove {} image(s){}",
            tags.len(),
            if purge {
                format!(" and {}", paths.config_dir.display())
            } else {
                String::new()
            }
        );
        print!("continue? [y/N] ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if !matches!(line.trim(), "y" | "Y" | "yes") {
            bail!("aborted");
        }
    }
    for t in &tags {
        if let Err(e) = docker.output(&["rmi", "-f", t]) {
            eprintln!("claude_here: {e:#}");
        }
    }
    if purge && paths.config_dir.exists() {
        std::fs::remove_dir_all(&paths.config_dir)
            .with_context(|| format!("removing {}", paths.config_dir.display()))?;
    }
    println!(
        "removed {} image(s){}",
        tags.len(),
        if purge { ", config purged" } else { "" }
    );
    println!("binary: remove with `cargo uninstall claude-here` or delete it from your PATH");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_does_not_repeat_toolchains() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let project = dir.path().join("config.toml");
        std::fs::write(&project, "toolchains = [\"dart\", \"rust\", \"rust\"]\n")?;
        let layers = Layers::load(dir.path().join("global.toml"), project.clone())?;
        let cli = ConfigFile {
            toolchains: vec!["dart".into(), "node".into()],
            ..Default::default()
        };
        save_layer(&layers, &cli, false)?;
        assert_eq!(
            ConfigFile::load(&project)?.toolchains,
            vec!["dart".to_string(), "rust".into(), "node".into()]
        );
        Ok(())
    }
}
