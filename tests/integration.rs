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

/// Run `script` through the entrypoint of `image` with the hardened base
/// flags, plus whatever `extra` adds (or removes: `no_new_privileges` off
/// for the docker toolchain, ADR 0010).
fn entrypoint_run_raw(
    image: &str,
    no_new_privileges: bool,
    extra: &[String],
    env: &[(&str, String)],
    script: &str,
) -> String {
    let uid = users_uid();
    let mut args: Vec<String> = ["run", "--rm", "--init", "--cap-drop", "ALL"]
        .iter()
        .map(ToString::to_string)
        .collect();
    for cap in claude_here::docker::CAPS {
        args.extend(["--cap-add".to_string(), (*cap).to_string()]);
    }
    if no_new_privileges {
        args.extend([
            "--security-opt".to_string(),
            "no-new-privileges".to_string(),
        ]);
    }
    args.extend(["--pids-limit".to_string(), "4096".to_string()]);
    args.extend(extra.iter().cloned());
    for (k, v) in [
        ("CH_USER", "ni".to_string()),
        ("CH_UID", uid.0.to_string()),
        ("CH_GID", uid.1.to_string()),
        ("CH_SESSION_ID", "itest".to_string()),
        ("CH_GIT_MODE", "ro".to_string()),
        ("CH_NET_CAPTURE", "0".to_string()),
    ] {
        args.extend(["-e".to_string(), format!("{k}={v}")]);
    }
    for (k, v) in env {
        args.extend(["-e".to_string(), format!("{k}={v}")]);
    }
    args.extend([image.to_string(), "bash".into(), "-c".into(), script.into()]);
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

/// The real binary, `--docker`: no `no-new-privileges`, the generated seccomp
/// profile, the per-project volume and the socket environment (ADR 0010).
#[test]
#[ignore = "needs docker"]
fn docker_toolchain_dry_run_relaxes_the_run_shape() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let xdg = config_root(tmp.path());
    let project = tmp.path().join("proj");
    fs::create_dir_all(&project).ok();
    let (code, out, err) = run_tool(&xdg, &project, &["--dry-run", "--docker", "-p", "x"]);
    assert_eq!(code, 0, "{err}");
    assert!(!out.contains("--security-opt no-new-privileges"), "{out}");
    assert!(out.contains("--security-opt apparmor=unconfined"), "{out}");
    assert!(
        out.contains("--security-opt systempaths=unconfined"),
        "{out}"
    );
    assert!(out.contains("--device /dev/net/tun"), "{out}");
    assert!(out.contains("--security-opt seccomp="), "{out}");
    assert!(out.contains("-v claude_here-containers-"), "{out}");
    assert!(out.contains("-e DOCKER_HOST=unix:///run/user/"), "{out}");
    assert!(
        out.contains("-e TESTCONTAINERS_RYUK_DISABLED=true"),
        "{out}"
    );
    assert!(out.contains("-docker-claude "), "{out}");
}

/// The docker layer builds (its setuid check passes), the entrypoint brings
/// up the podman socket, and a nested container runs — including one that
/// switches uid, which is what `no-new-privileges` had to go for.
#[test]
#[ignore = "needs docker and network"]
fn docker_toolchain_runs_a_nested_container() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let xdg = config_root(tmp.path());
    let project = tmp.path().join("proj");
    fs::create_dir_all(&project).ok();
    let (code, _, err) = run_tool(&xdg, &project, &["build", "-t", "docker"]);
    assert_eq!(code, 0, "build failed: {err}");
    let uid = users_uid();
    let image = claude_here::image::chain_tag(uid.0, &["docker".to_string()]);
    let profile = tmp.path().join("seccomp.json");
    let text = claude_here::seccomp::profile(claude_here::toolchain::PODMAN_SYSCALLS)
        .unwrap_or_else(|e| panic!("{e}"));
    fs::write(&profile, text).unwrap_or_else(|e| panic!("{e}"));
    let volume = "claude_here-containers-itest".to_string();
    let runtime = format!("/run/user/{}", uid.0);
    let extra = vec![
        "--security-opt".to_string(),
        "apparmor=unconfined".to_string(),
        "--security-opt".to_string(),
        "systempaths=unconfined".to_string(),
        "--device".to_string(),
        "/dev/net/tun".to_string(),
        "--security-opt".to_string(),
        format!("seccomp={}", profile.display()),
        "-v".to_string(),
        format!("{volume}:/home/ni/.local/share/containers/storage"),
    ];
    let env = [
        ("CH_CONTAINERS", "1".to_string()),
        ("XDG_RUNTIME_DIR", runtime.clone()),
        (
            "DOCKER_HOST",
            format!("unix://{runtime}/podman/podman.sock"),
        ),
    ];
    let script = "test -S \"${XDG_RUNTIME_DIR}/podman/podman.sock\" && echo sock=ok; \
        docker info >/dev/null 2>&1 && echo info=ok; \
        echo driver=$(podman info --format '{{.Store.GraphDriverName}}'); \
        docker run --rm docker.io/library/alpine:3.20 sh -c 'echo nested=ok'; \
        echo user=$(docker run --rm --user 1 docker.io/library/alpine:3.20 id -u); \
        getcap /usr/bin/newuidmap /usr/bin/newgidmap; \
        find / -xdev -type f -perm /6000 2>/dev/null | sed 's/^/setuid=/'";
    let out = entrypoint_run_raw(&image, false, &extra, &env, script);
    Command::new("docker")
        .args(["volume", "rm", "-f", &volume])
        .output()
        .ok();
    assert!(out.contains("sock=ok"), "{out}");
    assert!(out.contains("info=ok"), "{out}");
    assert!(
        out.contains("driver=overlay"),
        "native overlay only (ADR 0010): {out}"
    );
    assert!(out.contains("nested=ok"), "{out}");
    assert!(out.contains("user=1"), "{out}");
    assert!(
        out.contains("cap_setuid=ep") || out.contains("cap_setuid+ep"),
        "newuidmap must carry CAP_SETUID: {out}"
    );
    assert!(
        out.contains("cap_setgid=ep") || out.contains("cap_setgid+ep"),
        "newgidmap must carry CAP_SETGID: {out}"
    );
    assert!(
        !out.contains("setuid="),
        "no setuid file may be left: {out}"
    );
}

/// A throwaway self-signed CA, and the mount that puts it where the root
/// phase looks (ADR 0009).
fn itest_ca_mount(dir: &Path) -> Vec<String> {
    let pem = dir.join("ca.pem");
    let made = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=claude_here itest CA",
        ])
        .arg("-keyout")
        .arg(dir.join("ca.key"))
        .arg("-out")
        .arg(&pem)
        .output()
        .unwrap_or_else(|e| panic!("openssl: {e}"));
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    vec![
        "-v".to_string(),
        format!("{}:{}:ro", pem.display(), claude_here::run::CA_CRT),
    ]
}

/// A mounted PEM ends up in the system bundle at session start (ADR 0009).
#[test]
#[ignore = "needs docker"]
fn ca_is_added_to_the_system_bundle() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let xdg = config_root(tmp.path());
    let project = tmp.path().join("proj");
    fs::create_dir_all(&project).ok();
    ensure_base(&xdg, &project);
    let extra = itest_ca_mount(tmp.path());
    let uid = users_uid();
    let image = claude_here::image::base_tag(&uid.0);
    // A self-signed CA verifies only against a bundle that contains it.
    let script = "openssl verify -CAfile /etc/ssl/certs/ca-certificates.crt \
        /usr/local/share/ca-certificates/claude_here.crt && echo verify=ok; \
        test -L /etc/ssl/certs/claude_here.pem && echo link=ok";
    let out = entrypoint_run_raw(&image, true, &extra, &[], script);
    assert!(out.contains("verify=ok"), "{out}");
    assert!(
        out.contains("link=ok"),
        "no bundle symlink for the CA: {out}"
    );
}

/// With the jvm layer, the root phase also imports the PEM into the JDK's own
/// `cacerts`, so Maven and Gradle trust it without `JAVA_TOOL_OPTIONS`.
#[test]
#[ignore = "needs docker and the jvm layer (long build)"]
fn ca_is_imported_into_the_jvm_cacerts() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let xdg = config_root(tmp.path());
    let project = tmp.path().join("proj");
    fs::create_dir_all(&project).ok();
    let (code, _, err) = run_tool(&xdg, &project, &["build", "-t", "jvm"]);
    assert_eq!(code, 0, "build failed: {err}");
    let extra = itest_ca_mount(tmp.path());
    let uid = users_uid();
    let image = claude_here::image::chain_tag(uid.0, &["jvm".to_string()]);
    let script = "keytool -list -cacerts -storepass changeit -alias claude_here \
        && echo keytool=ok";
    let out = entrypoint_run_raw(&image, true, &extra, &[], script);
    assert!(out.contains("keytool=ok"), "{out}");
    assert!(
        out.contains("claude_here,") && out.contains("trustedCertEntry"),
        "{out}"
    );
}
