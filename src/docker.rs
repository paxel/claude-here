//! Pure assembly of `docker` command lines plus a thin runner.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// Capabilities the root entrypoint needs; everything else is dropped.
pub const CAPS: &[&str] = &[
    "NET_RAW",
    "SETUID",
    "SETGID",
    "SETPCAP",
    "KILL",
    "CHOWN",
    "DAC_OVERRIDE",
    "FOWNER",
];
/// Process limit inside the container.
pub const PIDS_LIMIT: u32 = 4096;

/// A bind mount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub host: PathBuf,
    pub container: PathBuf,
    pub read_only: bool,
}

impl Mount {
    pub fn ro(host: impl Into<PathBuf>, container: impl Into<PathBuf>) -> Self {
        Self {
            host: host.into(),
            container: container.into(),
            read_only: true,
        }
    }

    pub fn rw(host: impl Into<PathBuf>, container: impl Into<PathBuf>) -> Self {
        Self {
            host: host.into(),
            container: container.into(),
            read_only: false,
        }
    }

    fn to_flag(&self) -> String {
        let mut s = format!("{}:{}", self.host.display(), self.container.display());
        if self.read_only {
            s.push_str(":ro");
        }
        s
    }
}

/// Everything needed to produce `docker run ...`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunSpec {
    pub image: String,
    pub container_name: String,
    pub hostname: String,
    pub workdir: PathBuf,
    pub mounts: Vec<Mount>,
    /// `-e NAME=VALUE`
    pub env: Vec<(String, String)>,
    /// `-e NAME` (value taken from the host environment by docker)
    pub env_passthrough: Vec<String>,
    /// `--env-file PATH`
    pub env_files: Vec<PathBuf>,
    pub memory: Option<String>,
    pub cpus: Option<String>,
    /// Extra capabilities beyond `CAPS`, e.g. `NET_ADMIN` for the egress
    /// allowlist. The sandbox user holds no capabilities either way.
    pub extra_caps: Vec<String>,
    /// Raw flags appended before the image name.
    pub extra_args: Vec<String>,
    /// Keep `--security-opt no-new-privileges` off: a toolchain needs setuid
    /// helpers (ADR 0010). The default, `false`, sets it.
    pub allow_new_privileges: bool,
    /// `--security-opt apparmor=unconfined`: Docker's default profile denies
    /// `mount`, which rootless podman needs (ADR 0010). The daemon ignores
    /// the option on hosts without AppArmor.
    pub apparmor_unconfined: bool,
    /// `--security-opt systempaths=unconfined`: no masked or read-only
    /// `/proc` paths, so a nested container can mount its own procfs
    /// (ADR 0010).
    pub system_paths_unconfined: bool,
    /// Custom seccomp profile file instead of Docker's default.
    pub seccomp: Option<PathBuf>,
    /// Host device nodes passed through with `--device`.
    pub devices: Vec<String>,
    /// Named volumes `(name, container path)`.
    pub volumes: Vec<(String, PathBuf)>,
    pub tty: bool,
    /// Command executed by the entrypoint.
    pub command: Vec<String>,
}

impl RunSpec {
    /// Build the argument vector for `docker` (without the `docker` word).
    pub fn to_args(&self) -> Vec<String> {
        let mut a: Vec<String> = vec!["run".into(), "--rm".into(), "--init".into()];
        if self.tty {
            a.push("-it".into());
        } else {
            a.push("-i".into());
        }
        a.extend(["--name".into(), self.container_name.clone()]);
        a.extend(["--hostname".into(), self.hostname.clone()]);
        a.extend(["--cap-drop".into(), "ALL".into()]);
        for cap in CAPS {
            a.extend(["--cap-add".into(), (*cap).to_string()]);
        }
        for cap in &self.extra_caps {
            a.extend(["--cap-add".into(), cap.clone()]);
        }
        if !self.allow_new_privileges {
            a.extend(["--security-opt".into(), "no-new-privileges".into()]);
        }
        if self.apparmor_unconfined {
            a.extend(["--security-opt".into(), "apparmor=unconfined".into()]);
        }
        if self.system_paths_unconfined {
            a.extend(["--security-opt".into(), "systempaths=unconfined".into()]);
        }
        if let Some(p) = &self.seccomp {
            a.extend(["--security-opt".into(), format!("seccomp={}", p.display())]);
        }
        a.extend(["--pids-limit".into(), PIDS_LIMIT.to_string()]);
        a.extend(["--tmpfs".into(), "/tmp:exec".into()]);
        if let Some(m) = &self.memory {
            // Swap equal to the limit means no swap on top of it.
            a.extend(["--memory".into(), m.clone()]);
            a.extend(["--memory-swap".into(), m.clone()]);
        }
        if let Some(c) = &self.cpus {
            a.extend(["--cpus".into(), c.clone()]);
        }
        a.extend(["-w".into(), self.workdir.display().to_string()]);
        for m in &self.mounts {
            a.extend(["-v".into(), m.to_flag()]);
        }
        for d in &self.devices {
            a.extend(["--device".into(), d.clone()]);
        }
        for (name, path) in &self.volumes {
            a.extend(["-v".into(), format!("{name}:{}", path.display())]);
        }
        for (k, v) in &self.env {
            a.extend(["-e".into(), format!("{k}={v}")]);
        }
        for k in &self.env_passthrough {
            a.extend(["-e".into(), k.clone()]);
        }
        for f in &self.env_files {
            a.extend(["--env-file".into(), f.display().to_string()]);
        }
        a.extend(self.extra_args.iter().cloned());
        a.push(self.image.clone());
        a.extend(self.command.iter().cloned());
        a
    }
}

#[cfg(test)]
mod run_shape_tests {
    use super::*;

    #[test]
    fn relaxed_shape_drops_nnp_and_names_profile_and_volume() {
        let spec = RunSpec {
            image: "claude_here:base-docker".into(),
            allow_new_privileges: true,
            apparmor_unconfined: true,
            system_paths_unconfined: true,
            seccomp: Some("/home/axel/.config/claude_here/seccomp.json".into()),
            devices: vec!["/dev/net/tun".into()],
            volumes: vec![(
                "claude_here-containers-abcd1234".into(),
                "/home/ni/.local/share/containers/storage".into(),
            )],
            ..Default::default()
        };
        let s = spec.to_args().join(" ");
        assert!(!s.contains("no-new-privileges"));
        assert!(s.contains("--security-opt apparmor=unconfined"));
        assert!(s.contains("--security-opt systempaths=unconfined"));
        assert!(s.contains("--device /dev/net/tun"));
        assert!(s.contains("--security-opt seccomp=/home/axel/.config/claude_here/seccomp.json"));
        assert!(s.contains(
            "-v claude_here-containers-abcd1234:/home/ni/.local/share/containers/storage "
        ));
        assert!(s.contains("--cap-drop ALL"), "capabilities are not relaxed");
        assert!(s.contains("--pids-limit 4096"));
    }

    #[test]
    fn default_shape_keeps_nnp_and_docker_profile() {
        let s = RunSpec::default().to_args().join(" ");
        assert!(s.contains("--security-opt no-new-privileges"));
        assert!(!s.contains("--memory"), "no limit, no swap flag");
        assert!(!s.contains("seccomp="));
    }
}

/// True when raw docker args request host networking (capture is pointless then).
pub fn uses_host_network(args: &[String]) -> bool {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--network=host" || a == "--net=host" {
            return true;
        }
        if (a == "--network" || a == "--net") && it.next().is_some_and(|v| v == "host") {
            return true;
        }
    }
    false
}

/// Executes docker commands.
#[derive(Debug, Clone)]
pub struct Docker {
    binary: String,
}

impl Default for Docker {
    fn default() -> Self {
        Self {
            binary: std::env::var("CLAUDE_HERE_DOCKER").unwrap_or_else(|_| "docker".into()),
        }
    }
}

impl Docker {
    /// Fail early with a clear message when docker is unusable.
    pub fn check(&self) -> Result<()> {
        let out = Command::new(&self.binary)
            .args(["version", "--format", "{{.Server.Version}}"])
            .output()
            .with_context(|| format!("cannot execute '{}'", self.binary))?;
        if !out.status.success() {
            bail!(
                "docker daemon not reachable: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }

    /// Run interactively, inheriting stdio; returns the exit code.
    /// Run with inherited stdio and BuildKit enabled. Cache mounts in the
    /// Dockerfiles need BuildKit; docker has defaulted to it for years, but an
    /// older daemon would silently fail on `RUN --mount` without this.
    pub fn run_build(&self, args: &[String]) -> Result<i32> {
        let status = Command::new(&self.binary)
            .args(args)
            .env("DOCKER_BUILDKIT", "1")
            .status()
            .with_context(|| format!("executing {} {}", self.binary, args.join(" ")))?;
        Ok(status.code().unwrap_or(1))
    }

    pub fn run_inherit(&self, args: &[String]) -> Result<i32> {
        let status = Command::new(&self.binary)
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("executing {} {}", self.binary, args.join(" ")))?;
        Ok(status.code().unwrap_or(1))
    }

    /// Run and capture stdout; error on non-zero exit.
    pub fn output(&self, args: &[&str]) -> Result<String> {
        let out = Command::new(&self.binary)
            .args(args)
            .output()
            .with_context(|| format!("executing {} {}", self.binary, args.join(" ")))?;
        if !out.status.success() {
            bail!(
                "docker {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Read a single label from an image; `None` when the image is missing.
    pub fn image_label(&self, image: &str, label: &str) -> Option<String> {
        let fmt = format!("{{{{index .Config.Labels \"{label}\"}}}}");
        let out = Command::new(&self.binary)
            .args(["image", "inspect", "--format", &fmt, image])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    /// Content-addressed id of a local image; `None` when it is missing. A
    /// rebuild that hits the cache for every step keeps the id.
    pub fn image_id(&self, image: &str) -> Option<String> {
        let out = Command::new(&self.binary)
            .args(["image", "inspect", "--format", "{{.Id}}", image])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    /// `docker build` with inherited output.
    pub fn build(&self, build: &Build) -> Result<()> {
        let code = self.run_build(&build.to_args())?;
        if code != 0 {
            bail!("docker build of {} failed (exit {code})", build.tag);
        }
        Ok(())
    }
}

/// One `docker build` invocation.
#[derive(Debug, Clone, Default)]
pub struct Build {
    pub context: PathBuf,
    pub tag: String,
    pub build_args: Vec<(String, String)>,
    pub labels: Vec<(String, String)>,
    /// BuildKit secrets as `(id, host file)`; a `RUN --mount=type=secret`
    /// sees the file under `/run/secrets/<id>` for that step only.
    pub secrets: Vec<(String, PathBuf)>,
    /// `--no-cache --pull` (`update --base`). The BuildKit cache mounts keep
    /// the downloaded apt archives, so this re-installs, not re-downloads.
    pub refresh: bool,
}

impl Build {
    pub fn to_args(&self) -> Vec<String> {
        let mut args: Vec<String> = vec!["build".into(), "-t".into(), self.tag.clone()];
        if self.refresh {
            args.extend(["--no-cache".into(), "--pull".into()]);
        }
        for (k, v) in &self.build_args {
            args.extend(["--build-arg".into(), format!("{k}={v}")]);
        }
        for (k, v) in &self.labels {
            args.extend(["--label".into(), format!("{k}={v}")]);
        }
        for (id, src) in &self.secrets {
            args.extend(["--secret".into(), format!("id={id},src={}", src.display())]);
        }
        args.push(self.context.display().to_string());
        args
    }
}

/// A stand-in `docker` for unit tests: a shell script that logs every call
/// and keeps images (labels and ids) in a state directory, so code that
/// drives docker runs without a daemon.
#[cfg(test)]
pub mod fake {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use anyhow::Result;

    use super::Docker;

    const SCRIPT: &str = r#"#!/bin/sh
state="__STATE__"
printf '%s\n' "$*" >> "$state/calls"
img_dir() { printf '%s/images/%s' "$state" "$(printf '%s' "$1" | tr '/:' '__')"; }
case "$1" in
  version)
    [ -f "$state/down" ] && { echo "Cannot connect to the Docker daemon" >&2; exit 1; }
    echo 27.0 ;;
  image)
    [ "$2" = inspect ] || exit 1
    if [ "$3" = --format ]; then fmt="$4"; img="$5"; else fmt=""; img="$3"; fi
    d="$(img_dir "$img")"
    [ -d "$d" ] || { echo "No such image: $img" >&2; exit 1; }
    case "$fmt" in
      *Labels*) key="$(printf '%s' "$fmt" | sed 's/.*Labels "\(.*\)".*/\1/')"
                cat "$d/label.$key" 2>/dev/null || true ;;
      *.Id*) cat "$d/id" ;;
      *) echo '[{}]' ;;
    esac ;;
  build)
    [ -f "$state/fail-build" ] && exit 1
    shift; tag=""; d=""
    while [ $# -gt 1 ]; do
      case "$1" in
        -t) tag="$2"; d="$(img_dir "$tag")"; rm -rf "$d"; mkdir -p "$d"; shift 2 ;;
        --label) printf '%s' "${2#*=}" > "$d/label.${2%%=*}"; shift 2 ;;
        --build-arg|--secret) shift 2 ;;
        *) shift ;;
      esac
    done
    cp "$1/Dockerfile" "$d/Dockerfile"
    ls "$1" > "$d/context"
    printf 'sha256:%s' "$(cat "$d"/label.* 2>/dev/null | cksum | cut -d' ' -f1)" > "$d/id" ;;
  run)
    exit "$(cat "$state/run-exit" 2>/dev/null || echo 0)" ;;
  *) exit 0 ;;
esac
"#;

    pub struct FakeDocker {
        pub dir: tempfile::TempDir,
        pub docker: Docker,
    }

    impl FakeDocker {
        pub fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            fs::create_dir_all(dir.path().join("images"))?;
            let bin = dir.path().join("docker");
            let text = SCRIPT.replace("__STATE__", &dir.path().display().to_string());
            fs::write(&bin, text)?;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755))?;
            let docker = Docker {
                binary: bin.display().to_string(),
            };
            Ok(Self { dir, docker })
        }

        fn image_dir(&self, tag: &str) -> PathBuf {
            self.dir
                .path()
                .join("images")
                .join(tag.replace(['/', ':'], "_"))
        }

        /// Every call so far, one argument line each.
        pub fn calls(&self) -> Vec<String> {
            fs::read_to_string(self.dir.path().join("calls"))
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }

        /// The `docker build` calls so far, by tag.
        pub fn built(&self) -> Vec<String> {
            self.calls()
                .iter()
                .filter_map(|c| c.strip_prefix("build -t "))
                .filter_map(|c| c.split_whitespace().next())
                .map(str::to_string)
                .collect()
        }

        /// The Dockerfile and context file names an image was built from.
        pub fn built_from(&self, tag: &str) -> (String, String) {
            let d = self.image_dir(tag);
            (
                fs::read_to_string(d.join("Dockerfile")).unwrap_or_default(),
                fs::read_to_string(d.join("context")).unwrap_or_default(),
            )
        }

        pub fn label(&self, tag: &str, key: &str) -> Option<String> {
            fs::read_to_string(self.image_dir(tag).join(format!("label.{key}"))).ok()
        }

        /// An image that exists locally with these labels.
        pub fn add_image(&self, tag: &str, labels: &[(&str, &str)]) -> Result<()> {
            let d = self.image_dir(tag);
            fs::create_dir_all(&d)?;
            for (k, v) in labels {
                fs::write(d.join(format!("label.{k}")), v)?;
            }
            fs::write(d.join("id"), format!("sha256:{tag}"))?;
            Ok(())
        }

        pub fn set(&self, flag: &str, content: &str) -> Result<()> {
            fs::write(self.dir.path().join(flag), content)?;
            Ok(())
        }

        pub fn path(&self) -> &Path {
            self.dir.path()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wrapper_reports_what_docker_answers() -> Result<()> {
        let f = fake::FakeDocker::new()?;
        f.docker.check()?;
        f.set("down", "")?;
        let err = f.docker.check().err().map_or_default(|e| e.to_string());
        assert!(err.starts_with("docker daemon not reachable"), "{err}");

        assert_eq!(f.docker.image_label("missing", "k"), None);
        assert_eq!(f.docker.image_id("missing"), None);
        f.add_image("img:1", &[("k", "v"), ("empty", "")])?;
        assert_eq!(f.docker.image_label("img:1", "k").as_deref(), Some("v"));
        assert_eq!(f.docker.image_label("img:1", "empty"), None);
        assert_eq!(f.docker.image_id("img:1").as_deref(), Some("sha256:img:1"));
        assert_eq!(f.docker.output(&["image", "inspect", "img:1"])?, "[{}]");
        assert!(f.docker.output(&["image", "inspect", "missing"]).is_err());

        assert_eq!(f.docker.run_inherit(&["run".into()])?, 0);
        f.set("run-exit", "3")?;
        assert_eq!(f.docker.run_inherit(&["run".into()])?, 3);

        let ctx = tempfile::tempdir()?;
        std::fs::write(ctx.path().join("Dockerfile"), "FROM scratch\n")?;
        let build = Build {
            context: ctx.path().to_path_buf(),
            tag: "t:1".into(),
            labels: vec![("claude_here.hash".into(), "abc".into())],
            ..Default::default()
        };
        f.docker.build(&build)?;
        assert_eq!(f.label("t:1", "claude_here.hash").as_deref(), Some("abc"));
        f.set("fail-build", "")?;
        let err = f.docker.build(&build).err().map(|e| e.to_string());
        assert_eq!(err.as_deref(), Some("docker build of t:1 failed (exit 1)"));

        let missing = Docker {
            binary: f.path().join("nope").display().to_string(),
        };
        assert!(missing.check().is_err());
        assert!(missing.run_inherit(&[]).is_err());
        Ok(())
    }

    #[test]
    fn args_are_ordered_and_hardened() {
        let spec = RunSpec {
            image: "claude_here:base".into(),
            container_name: "claude_here-foo-abc".into(),
            hostname: "claude-here".into(),
            workdir: "/home/ni/foo".into(),
            mounts: vec![
                Mount::rw("/home/axel/foo", "/home/ni/foo"),
                Mount::ro("/home/axel/foo/.git", "/home/ni/foo/.git"),
            ],
            env: vec![("CH_USER".into(), "ni".into())],
            env_passthrough: vec!["TERM".into()],
            env_files: vec!["/home/axel/.config/claude_here/token".into()],
            memory: Some("8g".into()),
            cpus: None,
            extra_caps: vec!["NET_ADMIN".into()],
            extra_args: vec!["--network".into(), "mynet".into()],
            allow_new_privileges: false,
            apparmor_unconfined: false,
            system_paths_unconfined: false,
            seccomp: None,
            devices: vec![],
            volumes: vec![],
            tty: true,
            command: vec!["claude".into(), "-p".into(), "hi".into()],
        };
        let a = spec.to_args();
        let s = a.join(" ");
        assert!(s.starts_with("run --rm --init -it --name claude_here-foo-abc --hostname claude-here --cap-drop ALL --cap-add NET_RAW"));
        assert!(s.contains("--cap-add FOWNER --cap-add NET_ADMIN"));
        assert!(s.contains("--security-opt no-new-privileges --pids-limit 4096"));
        assert!(s.contains("--memory 8g --memory-swap 8g"));
        assert!(!s.contains("--cpus"));
        assert!(s.contains("-w /home/ni/foo -v /home/axel/foo:/home/ni/foo -v /home/axel/foo/.git:/home/ni/foo/.git:ro"));
        assert!(
            s.contains("-e CH_USER=ni -e TERM --env-file /home/axel/.config/claude_here/token")
        );
        assert!(s.ends_with("--network mynet claude_here:base claude -p hi"));
        // rw mount of the project comes before the ro overlay of .git
        let iv = a.iter().position(|x| x.ends_with("/foo")).unwrap_or(0);
        let ig = a.iter().position(|x| x.ends_with(".git:ro")).unwrap_or(0);
        assert!(iv < ig);
    }

    #[test]
    fn build_args_carry_secrets_and_end_with_the_context() {
        let b = Build {
            context: PathBuf::from("/tmp/ctx"),
            tag: "claude_here:base-u1000".into(),
            build_args: vec![("CH_UID".into(), "1000".into())],
            labels: vec![("claude_here.hash".into(), "abc".into())],
            secrets: vec![("claude_here_ca".into(), PathBuf::from("/home/axel/ca.pem"))],
            refresh: true,
        };
        assert_eq!(
            b.to_args(),
            [
                "build",
                "-t",
                "claude_here:base-u1000",
                "--no-cache",
                "--pull",
                "--build-arg",
                "CH_UID=1000",
                "--label",
                "claude_here.hash=abc",
                "--secret",
                "id=claude_here_ca,src=/home/axel/ca.pem",
                "/tmp/ctx",
            ]
        );
        let plain = Build {
            context: PathBuf::from("/tmp/ctx"),
            tag: "t".into(),
            ..Default::default()
        };
        assert_eq!(plain.to_args(), ["build", "-t", "t", "/tmp/ctx"]);
    }

    #[test]
    fn non_tty_uses_interactive_only() {
        let spec = RunSpec {
            tty: false,
            ..Default::default()
        };
        let a = spec.to_args();
        assert!(a.contains(&"-i".to_string()));
        assert!(!a.contains(&"-it".to_string()));
    }

    #[test]
    fn detects_host_network() {
        let v = |s: &[&str]| s.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
        assert!(uses_host_network(&v(&["--network", "host"])));
        assert!(uses_host_network(&v(&["--net=host"])));
        assert!(!uses_host_network(&v(&["--network", "bridge"])));
        assert!(!uses_host_network(&v(&["--hostname", "host"])));
    }
}
