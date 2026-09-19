//! Turn resolved config + host facts into a `RunSpec` (pure) and execute it.

use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{Config, GitMode, MountMode};
use crate::docker::{Docker, Mount, RunSpec, uses_host_network};
use crate::git::GitLayout;
use crate::image::{self, BuildPolicy, Builder, Identity};
use crate::paths::{HostPaths, PathRewriter, project_dir};
use crate::session::{MountInfo, SessionInfo, new_session_id, sanitize_name};
use crate::toolchain::{self, Toolchain};

/// Container-side path of the host net log directory.
pub const NET_OUT_DIR: &str = "/var/log/claude_here_out";
/// Container-side path of a forwarded ssh agent socket.
pub const SSH_AGENT_PATH: &str = "/run/claude_here/ssh-agent";

/// Facts about the host gathered before assembly (kept separate for tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFacts {
    pub cwd: PathBuf,
    pub uid: u32,
    pub gid: u32,
    pub tty: bool,
    pub gitconfig: Option<PathBuf>,
    pub ssh_auth_sock: Option<PathBuf>,
    pub known_hosts: Option<PathBuf>,
    /// Token from `gh auth token` on the host (keyring-backed logins keep it
    /// out of `hosts.yml`); only gathered when `gh` is requested in `full` mode.
    pub gh_token: Option<String>,
    pub token_file: Option<PathBuf>,
    /// Absolute host path of an empty file used to mask excluded cache files.
    pub empty_file: PathBuf,
}

impl HostFacts {
    pub fn gather(paths: &HostPaths, cfg: &Config) -> Result<Self> {
        let cwd = std::env::current_dir().context("reading current directory")?;
        let (uid, gid) = process_ids()?;
        let exists = |p: PathBuf| p.exists().then_some(p);
        let ssh_auth_sock = std::env::var_os("SSH_AUTH_SOCK")
            .map(PathBuf::from)
            .filter(|p| p.exists());
        let empty_file = paths.config_dir.join("empty");
        Ok(Self {
            cwd,
            uid,
            gid,
            tty: std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
            gitconfig: exists(paths.home.join(".gitconfig")),
            ssh_auth_sock,
            known_hosts: exists(paths.home.join(".ssh").join("known_hosts")),
            gh_token: (cfg.gh && cfg.git_mode == GitMode::Full)
                .then(host_gh_token)
                .flatten(),
            token_file: exists(paths.token_file()),
            empty_file,
        })
    }
}

/// `gh auth token` on the host, if gh is installed and logged in.
fn host_gh_token() -> Option<String> {
    let out = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!t.is_empty()).then_some(t)
}

/// Per-run env-file carrying `GH_TOKEN`; written 0600 before and removed after the run.
pub fn gh_env_file(paths: &HostPaths, session_id: &str) -> PathBuf {
    paths.config_dir.join(format!("gh-{session_id}.env"))
}

/// Effective uid/gid of the calling process (`id -u` / `id -g`), so container
/// files are owned by whoever runs the tool — never derived from directory
/// ownership.
pub fn process_ids() -> Result<(u32, u32)> {
    let read = |flag: &str| -> Result<u32> {
        let out = std::process::Command::new("id")
            .arg(flag)
            .output()
            .with_context(|| format!("running id {flag}"))?;
        if !out.status.success() {
            bail!("id {flag} failed");
        }
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .with_context(|| format!("unexpected output from id {flag}"))
    };
    Ok((read("-u")?, read("-g")?))
}

/// What the caller wants beyond config.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunRequest {
    pub yolo: bool,
    pub i_know: bool,
    pub claude_args: Vec<String>,
    pub image_tag: String,
    pub session_id: String,
    /// Enabled toolchains in build order, as resolved by `toolchain::resolve`.
    pub toolchains: Vec<&'static Toolchain>,
}

/// Assembly output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assembled {
    pub spec: RunSpec,
    pub info: SessionInfo,
    pub warnings: Vec<String>,
}

/// Pure assembly of the docker invocation.
pub fn assemble(
    cfg: &Config,
    paths: &HostPaths,
    facts: &HostFacts,
    git: &GitLayout,
    req: &RunRequest,
) -> Result<Assembled> {
    if req.yolo && cfg.git_mode == GitMode::Full && !req.i_know {
        bail!("claude_yolo with git mode 'full' refused; pass --i-know to override");
    }
    let Some(token_file) = facts.token_file.clone() else {
        bail!(
            "no token at {}; run `claude_here init` first",
            paths.token_file().display()
        );
    };
    let rw = PathRewriter::new(&paths.home, &cfg.user);
    let chome = rw.container_home().to_path_buf();
    let mut warnings = Vec::new();
    let mut mounts = Vec::new();
    let mut env: Vec<(String, String)> = Vec::new();

    let cwd_c = rw.rewrite(&facts.cwd);
    mounts.push(Mount::rw(&facts.cwd, &cwd_c));

    if cfg.git_mode == GitMode::Ro {
        for dir in &git.protected_dirs {
            mounts.push(Mount::ro(dir, rw.rewrite(dir)));
        }
    }
    warnings.extend(git.notes.iter().cloned());

    mounts.push(Mount::rw(paths.container_home(), chome.join(".claude")));
    if let Some(gc) = &facts.gitconfig {
        mounts.push(Mount::ro(gc, chome.join(".gitconfig")));
    }

    let net_capture = cfg.net_capture && !uses_host_network(&cfg.docker_args);
    if cfg.net_capture && !net_capture {
        warnings.push("host networking requested via docker_args; network capture disabled".into());
    }
    if net_capture {
        mounts.push(Mount::rw(paths.net_log_dir(), NET_OUT_DIR));
    }

    add_extra_mounts(cfg, paths, &rw, &mut mounts);
    add_cache_mounts(
        cfg,
        paths,
        facts,
        &rw,
        &req.toolchains,
        &mut mounts,
        &mut warnings,
    );
    add_truststore(cfg, paths, &chome, &mut mounts, &mut env);

    let (ssh, gh) = add_ssh_gh(cfg, facts, &chome, &mut mounts, &mut env, &mut warnings);

    let env_passthrough = add_env(cfg, facts, req, &chome, net_capture, &mut env);

    let info = session_info(cfg, facts, req, &cwd_c, &mounts, net_capture, ssh, gh);
    env.push((
        "CLAUDE_HERE_SESSION_INFO".into(),
        serde_json::to_string(&info).context("serializing session info")?,
    ));

    let mut command = vec!["claude".to_string()];
    if req.yolo {
        command.push("--dangerously-skip-permissions".into());
    }
    command.extend(["--append-system-prompt".to_string(), info.system_prompt()]);
    command.extend(req.claude_args.iter().cloned());

    let base = facts
        .cwd
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let short = req.session_id.rsplit('-').next().unwrap_or("x");
    let spec = RunSpec {
        image: req.image_tag.clone(),
        container_name: format!("claude_here-{}-{short}", sanitize_name(&base)),
        hostname: "claude-here".into(),
        workdir: cwd_c,
        mounts,
        env,
        env_passthrough,
        env_files: env_files(token_file, gh.then(|| gh_env_file(paths, &req.session_id))),
        memory: cfg.memory.clone(),
        cpus: cfg.cpus.map(|c| c.to_string()),
        extra_args: cfg.docker_args.clone(),
        tty: facts.tty,
        command,
    };
    Ok(Assembled {
        spec,
        info,
        warnings,
    })
}

/// The blob handed to Claude: what was granted and where things are.
#[allow(clippy::too_many_arguments)]
fn session_info(
    cfg: &Config,
    facts: &HostFacts,
    req: &RunRequest,
    cwd_c: &Path,
    mounts: &[Mount],
    net_capture: bool,
    ssh: bool,
    gh: bool,
) -> SessionInfo {
    SessionInfo {
        tool: "claude_here".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        session_id: req.session_id.clone(),
        image: req.image_tag.clone(),
        toolchains: toolchain::names(&req.toolchains),
        git_mode: cfg.git_mode,
        yolo: req.yolo,
        user: cfg.user.clone(),
        cwd_host: facts.cwd.clone(),
        cwd: cwd_c.to_path_buf(),
        mounts: mounts
            .iter()
            .map(|m| MountInfo {
                host: m.host.clone(),
                container: m.container.clone(),
                mode: if m.read_only { "ro" } else { "rw" }.to_string(),
            })
            .collect(),
        net_capture,
        ssh,
        gh,
    }
}

fn env_files(token_file: PathBuf, gh_file: Option<PathBuf>) -> Vec<PathBuf> {
    let mut v = vec![token_file];
    v.extend(gh_file);
    v
}

/// User-configured mounts (`[[mounts]]` / `--mount*`).
fn add_extra_mounts(cfg: &Config, paths: &HostPaths, rw: &PathRewriter, mounts: &mut Vec<Mount>) {
    for m in &cfg.mounts {
        let host = paths.expand_tilde(&m.path);
        let target = m
            .target
            .as_ref()
            .map_or_else(|| rw.rewrite(&host), PathBuf::from);
        mounts.push(match m.mode {
            MountMode::Ro => Mount::ro(host, target),
            MountMode::Rw => Mount::rw(host, target),
        });
    }
}

/// `[tls] truststore` → read-only mount + `JAVA_TOOL_OPTIONS`.
fn add_truststore(
    cfg: &Config,
    paths: &HostPaths,
    chome: &Path,
    mounts: &mut Vec<Mount>,
    env: &mut Vec<(String, String)>,
) {
    let Some(ts) = &cfg.truststore else { return };
    let host = paths.expand_tilde(ts);
    let target = chome.join(".claude_here").join("cacerts");
    mounts.push(Mount::ro(host, &target));
    env.push((
        "JAVA_TOOL_OPTIONS".into(),
        format!(
            "-Djavax.net.ssl.trustStore={} -Djavax.net.ssl.trustStorePassword={}",
            target.display(),
            cfg.truststore_password
        ),
    ));
}

/// Fixed session variables plus the configured `env` entries; returns the
/// names to pass through from the host.
fn add_env(
    cfg: &Config,
    facts: &HostFacts,
    req: &RunRequest,
    chome: &Path,
    net_capture: bool,
    env: &mut Vec<(String, String)>,
) -> Vec<String> {
    env.extend([
        ("CH_USER".to_string(), cfg.user.clone()),
        ("CH_UID".to_string(), facts.uid.to_string()),
        ("CH_GID".to_string(), facts.gid.to_string()),
        ("CH_SESSION_ID".to_string(), req.session_id.clone()),
        ("CH_GIT_MODE".to_string(), cfg.git_mode.to_string()),
        (
            "CH_NET_CAPTURE".to_string(),
            if net_capture { "1" } else { "0" }.to_string(),
        ),
        (
            "CLAUDE_CONFIG_DIR".to_string(),
            chome.join(".claude").display().to_string(),
        ),
    ]);

    let mut env_passthrough: Vec<String> = vec!["TERM".into(), "COLORTERM".into()];
    for e in &cfg.env {
        match e.split_once('=') {
            Some((k, v)) => env.push((k.to_string(), v.to_string())),
            None => env_passthrough.push(e.clone()),
        }
    }

    env_passthrough
}

/// ssh agent / gh config are mounted only in git mode `full`.
fn add_ssh_gh(
    cfg: &Config,
    facts: &HostFacts,
    chome: &Path,
    mounts: &mut Vec<Mount>,
    env: &mut Vec<(String, String)>,
    warnings: &mut Vec<String>,
) -> (bool, bool) {
    let mut ssh = false;
    let mut gh = false;
    if cfg.git_mode != GitMode::Full {
        if cfg.ssh || cfg.gh {
            warnings.push(format!(
                "ssh/gh are only mounted in git mode 'full' (current: {}); skipped",
                cfg.git_mode
            ));
        }
        return (ssh, gh);
    }
    if cfg.ssh {
        if let Some(sock) = &facts.ssh_auth_sock {
            mounts.push(Mount::rw(sock, SSH_AGENT_PATH));
            env.push(("SSH_AUTH_SOCK".into(), SSH_AGENT_PATH.into()));
            if let Some(kh) = &facts.known_hosts {
                mounts.push(Mount::ro(kh, chome.join(".ssh").join("known_hosts")));
            }
            ssh = true;
        } else {
            warnings.push("--ssh requested but SSH_AUTH_SOCK is not set; skipped".into());
        }
    }
    if cfg.gh {
        // Only the token travels (per-run env-file). Mounting ~/.config/gh is
        // pointless: keyring-backed logins keep no token in hosts.yml and the
        // stale entry would make `gh auth status` report an invalid account.
        if facts.gh_token.is_some() {
            gh = true;
        } else {
            warnings.push(
                "--gh requested but `gh auth token` returned nothing on the host; gh inside will be unauthenticated"
                    .into(),
            );
        }
    }
    (ssh, gh)
}

fn add_cache_mounts(
    cfg: &Config,
    paths: &HostPaths,
    facts: &HostFacts,
    rw: &PathRewriter,
    chain: &[&Toolchain],
    mounts: &mut Vec<Mount>,
    warnings: &mut Vec<String>,
) {
    let chome = rw.container_home();
    let credential_warning = |dir: &Path, file: &str, key: &str, warnings: &mut Vec<String>| {
        if dir.join(file).exists() {
            warnings.push(format!(
                "{} is mounted and may contain credentials; exclude it via caches.{key}_exclude",
                dir.join(file).display()
            ));
        }
    };
    for t in chain {
        for c in t.caches {
            let host = if cfg.caches_isolated {
                paths.cache_dir().join(c.key)
            } else {
                paths.expand_tilde(cfg.caches.override_for(c.key).unwrap_or(c.host))
            };
            let target = if c.container.starts_with('/') {
                PathBuf::from(c.container)
            } else {
                chome.join(c.container)
            };
            if c.subdirs.is_empty() {
                mounts.push(Mount::rw(&host, &target));
            } else {
                for sub in c.subdirs {
                    mounts.push(Mount::rw(host.join(sub), target.join(sub)));
                }
            }
            // Maven and Gradle keep credentials next to the cache; mask the
            // named files with an empty read-only file and warn otherwise.
            let excludes = match c.key {
                "m2" => Some((&cfg.m2_exclude, "settings.xml", "m2")),
                "gradle" => Some((&cfg.gradle_exclude, "gradle.properties", "gradle")),
                _ => None,
            };
            if let Some((list, secret, key)) = excludes {
                if !list.iter().any(|f| f == secret) {
                    credential_warning(&host, secret, key, warnings);
                }
                for f in list {
                    mounts.push(Mount::ro(&facts.empty_file, target.join(f)));
                }
            }
        }
    }
}

/// Ensure host-side directories exist before docker creates them as root.
pub fn prepare_host_dirs(paths: &HostPaths, cfg: &Config, spec: &RunSpec) -> Result<()> {
    fs::create_dir_all(paths.container_home())?;
    ensure_onboarded(&paths.container_home().join(".claude.json"))?;
    fs::create_dir_all(paths.net_log_dir())?;
    if !paths.config_dir.join("empty").exists() {
        fs::write(paths.config_dir.join("empty"), "")?;
    }
    if cfg.caches_isolated {
        fs::create_dir_all(paths.cache_dir())?;
    }
    for m in &spec.mounts {
        if !m.read_only && !m.host.exists() {
            fs::create_dir_all(&m.host)
                .with_context(|| format!("creating mount source {}", m.host.display()))?;
        }
    }
    Ok(())
}

/// Make sure the container `.claude.json` says onboarding is done, otherwise
/// interactive sessions start with the theme/login wizard despite the token.
fn ensure_onboarded(path: &Path) -> Result<()> {
    let mut obj = crate::init::read_json_object(path);
    if obj.get("hasCompletedOnboarding") == Some(&serde_json::Value::Bool(true)) {
        return Ok(());
    }
    obj.insert(
        "hasCompletedOnboarding".into(),
        serde_json::Value::Bool(true),
    );
    crate::init::write_json_object(path, &obj)
}

/// Full run: build images, assemble, execute. Returns the container exit code.
pub fn execute(
    cfg: &Config,
    paths: &HostPaths,
    policy: BuildPolicy,
    yolo: bool,
    i_know: bool,
    claude_args: Vec<String>,
) -> Result<i32> {
    let docker = Docker::default();
    docker.check()?;
    let facts = HostFacts::gather(paths, cfg)?;
    let git = crate::git::scan(&facts.cwd)?;
    let chain = toolchain::resolve(&cfg.toolchains)?;
    let builder = Builder {
        docker: &docker,
        paths,
        identity: Identity {
            user: cfg.user.clone(),
            uid: facts.uid,
            gid: facts.gid,
            claude_version: cfg.claude_version.clone(),
        },
        toolchains: chain.clone(),
        policy,
    };
    let image_tag = builder.ensure(cfg.image.as_deref(), &project_dir(&facts.cwd))?;
    let req = RunRequest {
        yolo,
        i_know,
        claude_args,
        image_tag,
        session_id: new_session_id(),
        toolchains: chain,
    };
    let assembled = assemble(cfg, paths, &facts, &git, &req)?;
    prepare_host_dirs(paths, cfg, &assembled.spec)?;
    for w in &assembled.warnings {
        eprintln!("claude_here: note: {w}");
    }
    let protected = if cfg.git_mode == GitMode::Ro {
        format!(" ({} .git dir(s) read-only)", git.protected_dirs.len())
    } else {
        String::new()
    };
    eprintln!(
        "claude_here: session {} | image {} | git {}{} | net {}{}",
        req.session_id,
        assembled.info.image,
        cfg.git_mode,
        protected,
        if assembled.info.net_capture {
            "recorded"
        } else {
            "not recorded"
        },
        if yolo { " | YOLO" } else { "" }
    );
    let gh_file = gh_env_file(paths, &req.session_id);
    if assembled.info.gh
        && let Some(token) = &facts.gh_token
    {
        crate::init::write_env_file(&gh_file, "GH_TOKEN", token)?;
    }
    let code = docker.run_inherit(&assembled.spec.to_args());
    let _ = fs::remove_file(&gh_file);
    let code = code?;
    if assembled.info.net_capture {
        crate::net::print_exit_summary(paths, &req.session_id, &assembled.info);
        crate::net::prune(paths, cfg.net_retention_days).ok();
    }
    Ok(code)
}

/// Build (or refresh) the image chain without running.
pub fn build_only(cfg: &Config, paths: &HostPaths, policy: BuildPolicy) -> Result<String> {
    let docker = Docker::default();
    docker.check()?;
    let facts = HostFacts::gather(paths, cfg)?;
    let builder = Builder {
        docker: &docker,
        paths,
        identity: Identity {
            user: cfg.user.clone(),
            uid: facts.uid,
            gid: facts.gid,
            claude_version: cfg.claude_version.clone(),
        },
        toolchains: toolchain::resolve(&cfg.toolchains)?,
        policy,
    };
    builder.ensure(cfg.image.as_deref(), &project_dir(&facts.cwd))
}

/// Claude Code version baked into the base image, if built.
pub fn installed_claude_version() -> Option<String> {
    let docker = Docker::default();
    docker
        .output(&[
            "run",
            "--rm",
            "--entrypoint",
            "claude",
            &image::base_tag(&process_ids().unwrap_or((0, 0)).0),
            "--version",
        ])
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigFile, MountSpec};
    use std::collections::BTreeSet;

    fn paths() -> HostPaths {
        HostPaths::new("/home/axel".into(), "/home/axel/.config/claude_here".into())
    }

    fn facts() -> HostFacts {
        HostFacts {
            cwd: "/home/axel/data/development/foo".into(),
            uid: 1000,
            gid: 1000,
            tty: true,
            gitconfig: Some("/home/axel/.gitconfig".into()),
            ssh_auth_sock: Some("/run/user/1000/ssh".into()),
            known_hosts: Some("/home/axel/.ssh/known_hosts".into()),
            gh_token: Some("gho_test".into()),
            token_file: Some("/home/axel/.config/claude_here/token".into()),
            empty_file: "/home/axel/.config/claude_here/empty".into(),
        }
    }

    fn git() -> GitLayout {
        GitLayout {
            protected_dirs: BTreeSet::from(["/home/axel/data/development/foo/.git".into()]),
            notes: vec![],
        }
    }

    fn req() -> RunRequest {
        RunRequest {
            yolo: false,
            i_know: false,
            claude_args: vec!["-p".into(), "hi".into()],
            image_tag: "claude_here:base".into(),
            session_id: "20260919-120000-abcd".into(),
            toolchains: vec![],
        }
    }

    fn cfg(f: ConfigFile) -> Config {
        f.into()
    }

    /// `req()` with the given toolchains resolved, as `execute` would.
    fn req_with(names: &[&str]) -> RunRequest {
        let owned: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
        RunRequest {
            toolchains: toolchain::resolve(&owned).unwrap_or_default(),
            ..req()
        }
    }

    #[test]
    fn ro_mode_mounts_git_read_only_and_no_ssh() -> Result<()> {
        let c = cfg(ConfigFile {
            ssh: Some(true),
            gh: Some(true),
            ..Default::default()
        });
        let a = assemble(&c, &paths(), &facts(), &git(), &req())?;
        let s = a.spec.to_args().join(" ");
        assert!(s.contains("-v /home/axel/data/development/foo:/home/ni/data/development/foo "));
        assert!(s.contains(
            "-v /home/axel/data/development/foo/.git:/home/ni/data/development/foo/.git:ro"
        ));
        assert!(s.contains("-v /home/axel/.config/claude_here/home:/home/ni/.claude "));
        assert!(s.contains("-e CLAUDE_CONFIG_DIR=/home/ni/.claude"));
        assert!(s.contains("-e CH_GIT_MODE=ro"));
        assert!(s.contains("--env-file /home/axel/.config/claude_here/token"));
        assert!(!s.contains("SSH_AUTH_SOCK"));
        assert!(!s.contains("/.config/gh"));
        assert!(
            a.warnings
                .iter()
                .any(|w| w.contains("only mounted in git mode 'full'"))
        );
        assert!(s.contains("-w /home/ni/data/development/foo"));
        assert!(s.ends_with("-p hi"));
        assert!(s.contains("claude_here:base claude --append-system-prompt"));
        assert!(a.spec.container_name.starts_with("claude_here-foo-abcd"));
        Ok(())
    }

    #[test]
    fn full_mode_with_ssh_and_gh() -> Result<()> {
        let c = cfg(ConfigFile {
            git_mode: Some(GitMode::Full),
            ssh: Some(true),
            gh: Some(true),
            ..Default::default()
        });
        let a = assemble(&c, &paths(), &facts(), &git(), &req())?;
        let s = a.spec.to_args().join(" ");
        assert!(!s.contains(".git:ro"));
        assert!(s.contains("-v /run/user/1000/ssh:/run/claude_here/ssh-agent "));
        assert!(s.contains("-e SSH_AUTH_SOCK=/run/claude_here/ssh-agent"));
        assert!(!s.contains("/.config/gh"));
        assert!(
            s.contains("--env-file /home/axel/.config/claude_here/gh-20260919-120000-abcd.env")
        );
        assert!(
            !s.contains("gho_test"),
            "token must never be on the command line"
        );
        assert!(a.info.ssh && a.info.gh);
        Ok(())
    }

    #[test]
    fn yolo_full_refused_without_i_know() {
        let c = cfg(ConfigFile {
            git_mode: Some(GitMode::Full),
            ..Default::default()
        });
        let mut r = req();
        r.yolo = true;
        assert!(assemble(&c, &paths(), &facts(), &git(), &r).is_err());
        r.i_know = true;
        let a = assemble(&c, &paths(), &facts(), &git(), &r);
        assert!(a.is_ok_and(|a| {
            a.spec
                .command
                .contains(&"--dangerously-skip-permissions".to_string())
        }));
    }

    #[test]
    fn missing_token_is_an_error() {
        let mut f = facts();
        f.token_file = None;
        let err = assemble(&cfg(ConfigFile::default()), &paths(), &f, &git(), &req());
        assert!(err.is_err());
    }

    #[test]
    fn env_and_mounts_and_caches() -> Result<()> {
        let c = cfg(ConfigFile {
            toolchains: vec!["jvm".into()],
            env: vec!["FOO=bar".into(), "HOSTVAR".into()],
            docker_args: vec!["--network".into(), "host".into()],
            mounts: vec![
                MountSpec {
                    path: "~/shared".into(),
                    mode: MountMode::Ro,
                    target: None,
                },
                MountSpec {
                    path: "/srv/data".into(),
                    mode: MountMode::Rw,
                    target: Some("/data".into()),
                },
            ],
            tls: crate::config::TlsFile {
                truststore: Some("~/certs/cacerts".into()),
                truststore_password: None,
            },
            caches: crate::config::CachesFile {
                m2_exclude: Some(vec!["settings.xml".into()]),
                ..Default::default()
            },
            ..Default::default()
        });
        let a = assemble(&c, &paths(), &facts(), &git(), &req_with(&["jvm"]))?;
        let s = a.spec.to_args().join(" ");
        assert!(s.contains("-e FOO=bar"));
        assert!(s.contains("-e HOSTVAR "));
        assert!(s.contains("-v /home/axel/shared:/home/ni/shared:ro"));
        assert!(s.contains("-v /srv/data:/data "));
        assert!(s.contains("-v /home/axel/.m2:/home/ni/.m2 "));
        assert!(s.contains("-v /home/axel/.config/claude_here/empty:/home/ni/.m2/settings.xml:ro"));
        assert!(s.contains("-v /home/axel/.gradle:/home/ni/.gradle "));
        assert!(s.contains("-v /home/axel/certs/cacerts:/home/ni/.claude_here/cacerts:ro"));
        assert!(s.contains("-e JAVA_TOOL_OPTIONS=-Djavax.net.ssl.trustStore=/home/ni/.claude_here/cacerts -Djavax.net.ssl.trustStorePassword=changeit"));
        assert!(!a.info.net_capture);
        assert!(s.contains("-e CH_NET_CAPTURE=0"));
        assert!(!s.contains(NET_OUT_DIR));
        assert!(s.contains("--network host claude_here:base"));
        Ok(())
    }

    #[test]
    fn rust_caches_and_isolated() -> Result<()> {
        let c = cfg(ConfigFile {
            toolchains: vec!["rust".into()],
            caches: crate::config::CachesFile {
                isolated: Some(true),
                ..Default::default()
            },
            ..Default::default()
        });
        let a = assemble(&c, &paths(), &facts(), &git(), &req_with(&["rust"]))?;
        let s = a.spec.to_args().join(" ");
        assert!(s.contains(
            "-v /home/axel/.config/claude_here/cache/cargo/registry:/home/ni/.cargo/registry "
        ));
        assert!(
            s.contains("-v /home/axel/.config/claude_here/cache/cargo/git:/home/ni/.cargo/git ")
        );
        Ok(())
    }

    #[test]
    fn node_and_uv_caches() -> Result<()> {
        let c = cfg(ConfigFile {
            toolchains: vec!["node".into(), "uv".into()],
            ..Default::default()
        });
        let a = assemble(&c, &paths(), &facts(), &git(), &req_with(&["node", "uv"]))?;
        let s = a.spec.to_args().join(" ");
        assert!(s.contains("-v /home/axel/.npm:/home/ni/.npm "));
        assert!(s.contains("-v /home/axel/.cache/uv:/home/ni/.cache/uv "));
        Ok(())
    }

    #[test]
    fn same_user_name_keeps_paths() -> Result<()> {
        let c = cfg(ConfigFile {
            user: Some("axel".into()),
            ..Default::default()
        });
        let a = assemble(&c, &paths(), &facts(), &git(), &req())?;
        assert_eq!(
            a.spec.workdir,
            PathBuf::from("/home/axel/data/development/foo")
        );
        Ok(())
    }
}
