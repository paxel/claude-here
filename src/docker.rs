//! Pure assembly of `docker` command lines plus a thin runner.

use std::path::{Path, PathBuf};
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
    /// Raw flags appended before the image name.
    pub extra_args: Vec<String>,
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
        a.extend(["--security-opt".into(), "no-new-privileges".into()]);
        a.extend(["--pids-limit".into(), PIDS_LIMIT.to_string()]);
        a.extend(["--tmpfs".into(), "/tmp:exec".into()]);
        if let Some(m) = &self.memory {
            a.extend(["--memory".into(), m.clone()]);
        }
        if let Some(c) = &self.cpus {
            a.extend(["--cpus".into(), c.clone()]);
        }
        a.extend(["-w".into(), self.workdir.display().to_string()]);
        for m in &self.mounts {
            a.extend(["-v".into(), m.to_flag()]);
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

    /// `docker build` with inherited output.
    pub fn build(
        &self,
        context: &Path,
        tag: &str,
        build_args: &[(String, String)],
        labels: &[(String, String)],
        refresh: bool,
    ) -> Result<()> {
        let mut args: Vec<String> = vec!["build".into(), "-t".into(), tag.into()];
        if refresh {
            args.extend(["--no-cache".into(), "--pull".into()]);
        }
        for (k, v) in build_args {
            args.extend(["--build-arg".into(), format!("{k}={v}")]);
        }
        for (k, v) in labels {
            args.extend(["--label".into(), format!("{k}={v}")]);
        }
        args.push(context.display().to_string());
        let code = self.run_inherit(&args)?;
        if code != 0 {
            bail!("docker build of {tag} failed (exit {code})");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            extra_args: vec!["--network".into(), "mynet".into()],
            tty: true,
            command: vec!["claude".into(), "-p".into(), "hi".into()],
        };
        let a = spec.to_args();
        let s = a.join(" ");
        assert!(s.starts_with("run --rm --init -it --name claude_here-foo-abc --hostname claude-here --cap-drop ALL --cap-add NET_RAW"));
        assert!(s.contains("--security-opt no-new-privileges --pids-limit 4096"));
        assert!(s.contains("--memory 8g"));
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
