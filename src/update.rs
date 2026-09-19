//! Update check for the tool itself.
//!
//! The network is never touched while a session starts: the prompt is decided
//! from a cached value, and the cache is refreshed after the container exits,
//! at most once a day. A stale cache means the news arrives one session late,
//! which is the right trade for a tool that promises not to surprise anyone
//! with traffic.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::HostPaths;
use crate::session::now_secs;

/// Repository the releases are published from.
pub const REPO: &str = "paxel/claude-here";
/// How long a cached answer is considered current.
pub const MAX_AGE_SECS: i64 = 24 * 3600;
/// Seconds the check may take before it is abandoned.
const TIMEOUT_SECS: u64 = 5;

/// Version of the running binary.
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// What `~/.config/claude_here/update-check.json` holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Cache {
    /// Unix seconds of the last successful check.
    pub checked_at: i64,
    /// Latest released version, without a leading `v`.
    pub latest: String,
}

fn cache_path(paths: &HostPaths) -> PathBuf {
    paths.config_dir.join("update-check.json")
}

pub fn read_cache(paths: &HostPaths) -> Option<Cache> {
    let text = std::fs::read_to_string(cache_path(paths)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_cache(paths: &HostPaths, cache: &Cache) -> Result<()> {
    let path = cache_path(paths);
    let text = serde_json::to_string(cache).context("serializing update cache")?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// `1.2.3` / `v1.2.3` / `1.2.3-rc1` as comparable numbers. A version that
/// cannot be parsed compares as `0.0.0`, so nonsense never looks newer.
fn parts(version: &str) -> (u64, u64, u64) {
    let v = version.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next().unwrap_or("");
    let mut it = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// Is `latest` a higher version than `current`?
pub fn is_newer(latest: &str, current: &str) -> bool {
    parts(latest) > parts(current)
}

/// The newer version the cache knows about, if any.
pub fn pending(paths: &HostPaths) -> Option<String> {
    let cache = read_cache(paths)?;
    is_newer(&cache.latest, current()).then_some(cache.latest)
}

/// Whether the cached answer is old enough to be refreshed.
pub fn is_stale(cache: Option<&Cache>, now: i64) -> bool {
    cache.is_none_or(|c| now - c.checked_at >= MAX_AGE_SECS)
}

/// Ask GitHub for the latest release tag and store it. Every failure is
/// silent: this must never affect the exit code of a session.
pub fn refresh(paths: &HostPaths) -> Option<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let out = Command::new("curl")
        .args([
            "-fsS",
            "--max-time",
            &TIMEOUT_SECS.to_string(),
            "-H",
            "Accept: application/vnd.github+json",
            &url,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let tag = body.get("tag_name")?.as_str()?;
    let latest = tag.trim_start_matches('v').to_string();
    write_cache(
        paths,
        &Cache {
            checked_at: now_secs(),
            latest: latest.clone(),
        },
    )
    .ok()?;
    Some(latest)
}

/// Refresh when the cache is stale. Called after a session, so the wait is
/// never in front of the work.
pub fn refresh_if_stale(paths: &HostPaths, enabled: bool) {
    if !enabled || suppressed_by_env() {
        return;
    }
    if is_stale(read_cache(paths).as_ref(), now_secs()) {
        refresh(paths);
    }
}

/// `CLAUDE_HERE_NO_UPDATE_CHECK` turns everything here off, for CI and for
/// anyone who does not want the call at all.
pub fn suppressed_by_env() -> bool {
    std::env::var_os("CLAUDE_HERE_NO_UPDATE_CHECK").is_some_and(|v| v != "0")
}

/// How this binary got onto the machine, which decides how it is replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Install {
    Cargo,
    Brew,
    Tarball,
}

/// Guess from the path of the running binary.
pub fn install_method(exe: &Path) -> Install {
    let p = exe.display().to_string();
    if p.contains("/.cargo/bin/") {
        Install::Cargo
    } else if p.contains("/Cellar/") || p.contains("/homebrew/") || p.contains("/linuxbrew/") {
        Install::Brew
    } else {
        Install::Tarball
    }
}

impl Install {
    /// The command a user would run by hand.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Cargo => "cargo install --git https://github.com/paxel/claude-here --force",
            Self::Brew => "brew upgrade claude-here",
            Self::Tarball => {
                "curl -fsSL https://github.com/paxel/claude-here/releases/latest/download/install.sh | sh"
            }
        }
    }

    /// The same as an argv for `sh -c`.
    fn argv(self) -> [&'static str; 3] {
        ["sh", "-c", self.hint()]
    }
}

/// Prompt only when a human is there to answer and a newer version is known.
/// `-p` runs, pipes and CI therefore never stall.
pub fn should_prompt(enabled: bool, tty: bool, pending: Option<&String>) -> bool {
    enabled && tty && !suppressed_by_env() && pending.is_some()
}

/// Ask, and update when the answer is yes. Returns whether the binary was
/// replaced, in which case the caller should re-exec.
pub fn prompt_and_update(paths: &HostPaths, tty: bool, enabled: bool) -> bool {
    if !enabled || suppressed_by_env() {
        return false;
    }
    let Some(latest) = pending(paths) else {
        return false;
    };
    let exe = std::env::current_exe().unwrap_or_default();
    let method = install_method(&exe);
    if !should_prompt(enabled, tty, Some(&latest)) {
        // Not interactive: say it once, do nothing.
        eprintln!(
            "claude_here: update available: {} -> {latest}  ({})",
            current(),
            method.hint()
        );
        return false;
    }
    eprint!(
        "claude_here: update available: {} -> {latest}. Update now? [y/N] ",
        current()
    );
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    if !matches!(line.trim(), "y" | "Y" | "yes") {
        eprintln!("claude_here: skipped; update later with: {}", method.hint());
        return false;
    }
    eprintln!("claude_here: running: {}", method.hint());
    let argv = method.argv();
    let ok = Command::new(argv[0])
        .args(&argv[1..])
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!("claude_here: update failed; continuing with {}", current());
        return false;
    }
    // The cache would otherwise keep announcing the version we just installed.
    let _ = write_cache(
        paths,
        &Cache {
            checked_at: now_secs(),
            latest,
        },
    );
    true
}

/// Replace the current process with the updated binary, same arguments.
/// Returns only on failure.
pub fn reexec() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    eprintln!("claude_here: restarting with the new version");
    // A short pause lets a package manager finish replacing the file.
    std::thread::sleep(Duration::from_millis(200));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(&exe).args(&args).exec();
        eprintln!("claude_here: could not restart ({err}); run the command again");
    }
    #[cfg(not(unix))]
    {
        let _ = Command::new(&exe).args(&args).status();
    }
}

/// Terminal state, kept here so callers do not repeat it.
pub fn on_a_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        // Ten beats nine: string comparison would get this wrong.
        assert!(is_newer("0.10.0", "0.9.0"));
        // Unparseable never looks newer.
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(!is_newer("", "0.1.0"));
    }

    #[test]
    fn prerelease_compares_by_its_numbers() {
        assert!(is_newer("0.2.0-rc1", "0.1.0"));
        assert!(!is_newer("0.1.0-rc1", "0.1.0"));
    }

    #[test]
    fn staleness_follows_the_clock() {
        let fresh = Cache {
            checked_at: 1_000_000,
            latest: "0.1.0".into(),
        };
        assert!(!is_stale(Some(&fresh), 1_000_000 + MAX_AGE_SECS - 1));
        assert!(is_stale(Some(&fresh), 1_000_000 + MAX_AGE_SECS));
        assert!(is_stale(None, 0));
    }

    #[test]
    fn install_method_comes_from_the_path() {
        assert_eq!(
            install_method(Path::new("/home/axel/.cargo/bin/claude_here")),
            Install::Cargo
        );
        assert_eq!(
            install_method(Path::new("/opt/homebrew/bin/claude_here")),
            Install::Brew
        );
        assert_eq!(
            install_method(Path::new(
                "/usr/local/Cellar/claude-here/0.1.0/bin/claude_here"
            )),
            Install::Brew
        );
        assert_eq!(
            install_method(Path::new("/home/axel/.local/bin/claude_here")),
            Install::Tarball
        );
        for m in [Install::Cargo, Install::Brew, Install::Tarball] {
            assert!(!m.hint().is_empty());
        }
    }

    #[test]
    fn prompting_needs_a_terminal_and_a_newer_version() {
        let v = "0.2.0".to_string();
        assert!(should_prompt(true, true, Some(&v)));
        assert!(!should_prompt(false, true, Some(&v)));
        assert!(!should_prompt(true, false, Some(&v)));
        assert!(!should_prompt(true, true, None));
    }

    #[test]
    fn nothing_happens_when_switched_off() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = HostPaths::new(dir.path().join("home"), dir.path().join("cfg"));
        std::fs::create_dir_all(&paths.config_dir)?;
        write_cache(
            &paths,
            &Cache {
                checked_at: now_secs(),
                latest: "9.9.9".into(),
            },
        )?;
        // Disabled by config: no prompt, and nothing printed either.
        assert!(!prompt_and_update(&paths, true, false));
        refresh_if_stale(&paths, false);
        assert_eq!(
            read_cache(&paths).map(|c| c.latest).as_deref(),
            Some("9.9.9")
        );
        Ok(())
    }

    #[test]
    fn cache_round_trips() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = HostPaths::new(dir.path().join("home"), dir.path().join("cfg"));
        std::fs::create_dir_all(&paths.config_dir)?;
        assert!(read_cache(&paths).is_none());
        let c = Cache {
            checked_at: 42,
            latest: "9.9.9".into(),
        };
        write_cache(&paths, &c)?;
        assert_eq!(read_cache(&paths), Some(c));
        assert_eq!(pending(&paths).as_deref(), Some("9.9.9"));
        Ok(())
    }
}
