//! Integration tests that need a working docker daemon. Run with
//! `cargo test -- --ignored`. The first run builds `claude_here:base`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_claude_here"))
}

/// Isolated config dir with a dummy token so `assemble` is satisfied.
fn config_root(dir: &Path) -> PathBuf {
    let cfg = dir.join("claude_here");
    fs::create_dir_all(&cfg).ok();
    fs::write(cfg.join("token"), "CLAUDE_CODE_OAUTH_TOKEN=dummy\n").ok();
    dir.to_path_buf()
}

fn run_tool(xdg: &Path, cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .env("XDG_CONFIG_HOME", xdg)
        .output()
        .unwrap_or_else(|e| panic!("spawn: {e}"));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn entrypoint_run(mode: &str, project: &Path, git_ro: bool, script: &str) -> String {
    let uid = users_uid();
    let mut args: Vec<String> = ["run", "--rm", "--init", "--cap-drop", "ALL"]
        .iter()
        .map(ToString::to_string)
        .collect();
    for cap in claude_here::docker::CAPS {
        args.extend(["--cap-add".to_string(), (*cap).to_string()]);
    }
    args.extend(
        [
            "--security-opt",
            "no-new-privileges",
            "-w",
            "/home/ni/w",
            "-v",
            &format!("{}:/home/ni/w", project.display()),
        ]
        .iter()
        .map(ToString::to_string),
    );
    if git_ro {
        args.extend([
            "-v".to_string(),
            format!("{}/.git:/home/ni/w/.git:ro", project.display()),
        ]);
    }
    args.extend(
        [
            "-e",
            "CH_USER=ni",
            "-e",
            &format!("CH_UID={}", uid.0),
            "-e",
            &format!("CH_GID={}", uid.1),
            "-e",
            "CH_SESSION_ID=itest",
            "-e",
            &format!("CH_GIT_MODE={mode}"),
            "-e",
            "CH_NET_CAPTURE=0",
            &claude_here::image::base_tag(&uid.0),
            "bash",
            "-c",
            script,
        ]
        .iter()
        .map(ToString::to_string),
    );
    let out = Command::new("docker")
        .args(&args)
        .output()
        .unwrap_or_else(|e| panic!("docker: {e}"));
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn users_uid() -> (u32, u32) {
    use std::os::unix::fs::MetadataExt;
    let home = std::env::var("HOME").unwrap_or_default();
    let m = fs::metadata(home).unwrap_or_else(|e| panic!("stat home: {e}"));
    (m.uid(), m.gid())
}

fn ensure_base(xdg: &Path, cwd: &Path) {
    let (code, out, err) = run_tool(xdg, cwd, &["build"]);
    assert_eq!(code, 0, "build failed: {err}");
    assert!(out.contains("claude_here:base"), "{out}");
}

#[test]
#[ignore = "needs docker"]
fn claude_runs_inside_the_sandbox() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let xdg = config_root(tmp.path());
    let project = tmp.path().join("proj");
    fs::create_dir_all(&project).ok();
    ensure_base(&xdg, &project);
    let (code, out, err) = run_tool(&xdg, &project, &["--no-net-log", "--", "--version"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Claude Code"), "stdout: {out}\nstderr: {err}");
    assert!(err.contains("git ro"), "{err}");
}

#[test]
#[ignore = "needs docker"]
fn dry_run_prints_docker_command() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let xdg = config_root(tmp.path());
    let project = tmp.path().join("proj");
    fs::create_dir_all(project.join(".git")).ok();
    let (code, out, _) = run_tool(&xdg, &project, &["--dry-run", "--rust", "-p", "x"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("docker run --rm --init"));
    assert!(out.contains(".git:ro"));
    assert!(out.contains("claude_here:base-u"));
    // The image a run uses ends with the Claude layer.
    assert!(out.contains("-rust-claude "));
    assert!(out.contains(" claude --append-system-prompt"));
    assert!(out.trim_end().ends_with("-p x"));
}

#[test]
#[ignore = "needs docker"]
fn ro_mode_blocks_every_git_write() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let xdg = config_root(tmp.path());
    let project = tmp.path().join("repo");
    fs::create_dir_all(&project).ok();
    ensure_base(&xdg, &project);
    // Create the repository inside the container (git is available there) so
    // the host needs no git configuration.
    let prep = entrypoint_run(
        "full",
        &project,
        false,
        "git init -q && git -c user.name=t -c user.email=t@t commit -q --allow-empty -m init && echo x > f && echo prepared",
    );
    assert!(prep.contains("prepared"), "{prep}");
    let out = entrypoint_run(
        "ro",
        &project,
        true,
        r#"git status --short; git add f; echo "add=$?"; git -c user.name=t -c user.email=t@t commit -qm x; echo "commit=$?"; git checkout -b evil; echo "checkout=$?"; echo ref: refs/heads/evil > .git/HEAD; echo "head=$?"; echo y >> f; echo "file=$?""#,
    );
    assert!(out.contains("add=128"), "{out}");
    assert!(out.contains("commit=128"), "{out}");
    assert!(out.contains("checkout=128"), "{out}");
    assert!(out.contains("head=1"), "{out}");
    assert!(out.contains("file=0"), "{out}");
    assert!(out.contains("Read-only file system"), "{out}");
}

#[test]
#[ignore = "needs docker"]
fn commit_mode_shim_allows_commit_and_denies_push_and_checkout() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let xdg = config_root(tmp.path());
    let project = tmp.path().join("repo");
    fs::create_dir_all(&project).ok();
    ensure_base(&xdg, &project);
    let out = entrypoint_run(
        "commit",
        &project,
        false,
        r#"git init -q 2>&1; echo "init=$?"; /usr/bin/git init -q && echo x > f && git add f && git -c user.name=t -c user.email=t@t commit -qm x; echo "commit=$?"; git checkout -b evil; echo "checkout=$?"; git push; echo "push=$?"; git branch; echo "branch=$?"; git stash; echo "stash=$?"; cat /etc/claude_here/git_mode"#,
    );
    assert!(out.contains("init=125"), "{out}");
    assert!(out.contains("commit=0"), "{out}");
    assert!(out.contains("checkout=125"), "{out}");
    assert!(out.contains("push=125"), "{out}");
    assert!(out.contains("branch=0"), "{out}");
    assert!(out.contains("stash=125"), "{out}");
    assert!(out.contains("\ncommit\n"), "{out}");
}

#[test]
#[ignore = "needs docker"]
fn sandbox_user_cannot_stop_capture_and_summary_is_written() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let xdg = config_root(tmp.path());
    let project = tmp.path().join("proj");
    fs::create_dir_all(&project).ok();
    ensure_base(&xdg, &project);
    let out_dir = tmp.path().join("out");
    fs::create_dir_all(&out_dir).ok();
    let uid = users_uid();
    let mut args: Vec<String> = ["run", "--rm", "--init", "--cap-drop", "ALL"]
        .iter()
        .map(ToString::to_string)
        .collect();
    for cap in claude_here::docker::CAPS {
        args.extend(["--cap-add".to_string(), (*cap).to_string()]);
    }
    args.extend(
        [
            "--security-opt", "no-new-privileges",
            "-v", &format!("{}:/var/log/claude_here_out", out_dir.display()),
            "-e", "CH_USER=ni",
            "-e", &format!("CH_UID={}", uid.0),
            "-e", &format!("CH_GID={}", uid.1),
            "-e", "CH_SESSION_ID=cap",
            "-e", "CH_GIT_MODE=ro",
            "-e", "CH_NET_CAPTURE=1",
            &claude_here::image::base_tag(&uid.0), "bash", "-c",
            "sleep 1; kill $(pgrep tcpdump) 2>&1; echo kill=$?; curl -sS -o /dev/null http://example.com/; echo curl=$?",
        ]
        .iter()
        .map(ToString::to_string),
    );
    let out = Command::new("docker")
        .args(&args)
        .output()
        .unwrap_or_else(|e| panic!("docker: {e}"));
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("kill=1"), "{text}");
    assert!(text.contains("curl=0"), "{text}");
    let summary = fs::read_to_string(out_dir.join("cap.summary.json")).unwrap_or_default();
    assert!(summary.contains("\"example.com\""), "{summary}");
    assert!(summary.contains("GET http://example.com/"), "{summary}");
    assert!(out_dir.join("cap.pcap").exists());
}
