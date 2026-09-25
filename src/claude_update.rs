//! Claude Code release detection.
//!
//! Same rules as the tool's own update check (`update`): a session never waits
//! for the network. The start decides from a cached release version, and the
//! cache is refreshed after the container exits, at most once a day. A new
//! release is offered with a prompt; once accepted, the version is recorded and
//! every image chain rebuilds its Claude layer on its next start, without
//! asking again.
//!
//! `claude_version = "latest"|"stable"` follows that channel. Any other value
//! is a pin: no check, no prompt, that exact version.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::paths::HostPaths;
use crate::session::now_secs;
use crate::update::{MAX_AGE_SECS, is_newer, suppressed_by_env};

/// Where `install.sh` itself reads the version of a channel from.
pub const RELEASES: &str = "https://downloads.claude.ai/claude-code-releases";
/// Seconds the check may take before it is abandoned.
const TIMEOUT_SECS: u64 = 5;

/// What `~/.config/claude_here/claude-version.json` holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Cache {
    /// Unix seconds of the last successful check.
    pub checked_at: i64,
    /// Channel `latest` was read from.
    pub channel: String,
    /// Newest release of `channel` at `checked_at`.
    pub latest: String,
    /// Version the user agreed to; what every Claude layer is built with.
    #[serde(default)]
    pub accepted: Option<String>,
}

fn cache_path(paths: &HostPaths) -> PathBuf {
    paths.config_dir.join("claude-version.json")
}

pub fn read_cache(paths: &HostPaths) -> Option<Cache> {
    let text = std::fs::read_to_string(cache_path(paths)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_cache(paths: &HostPaths, cache: &Cache) -> Result<()> {
    let path = cache_path(paths);
    let text = serde_json::to_string(cache).context("serializing Claude version cache")?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// The channel `claude_version` follows, or `None` when it is a pin.
pub fn channel(claude_version: &str) -> Option<&str> {
    matches!(claude_version, "latest" | "stable").then_some(claude_version)
}

/// `1.2.3` or `1.2.3-suffix`, the shape `install.sh` accepts. Anything else
/// (a captive portal's HTML, an error page) is not a version.
pub fn is_version(s: &str) -> bool {
    let (core, suffix) = match s.split_once('-') {
        Some((c, rest)) => (c, Some(rest)),
        None => (s, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && suffix.is_none_or(|x| !x.is_empty() && !x.contains(char::is_whitespace))
}

/// Ask the release server for the current version of `channel`.
fn fetch(channel: &str) -> Option<String> {
    let out = Command::new("curl")
        .args([
            "-fsS",
            "--max-time",
            &TIMEOUT_SECS.to_string(),
            &format!("{RELEASES}/{channel}"),
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    is_version(&v).then_some(v)
}

/// Fetch and store the newest release of `channel`, keeping the accepted
/// version. Every failure is silent and leaves the cache untouched.
pub fn refresh(paths: &HostPaths, channel: &str) -> Option<String> {
    let latest = fetch(channel)?;
    let accepted = read_cache(paths).and_then(|c| c.accepted);
    write_cache(
        paths,
        &Cache {
            checked_at: now_secs(),
            channel: channel.to_string(),
            latest: latest.clone(),
            accepted,
        },
    )
    .ok()?;
    Some(latest)
}

/// Whether the cached answer is missing, old, or about another channel.
pub fn is_stale(cache: Option<&Cache>, channel: &str, now: i64) -> bool {
    cache.is_none_or(|c| c.channel != channel || now - c.checked_at >= MAX_AGE_SECS)
}

/// Refresh when the cache is stale. Called after a session.
pub fn refresh_if_stale(paths: &HostPaths, cfg: &Config) {
    let Some(ch) = channel(&cfg.claude_version) else {
        return;
    };
    if !cfg.update_check || suppressed_by_env() {
        return;
    }
    if is_stale(read_cache(paths).as_ref(), ch, now_secs()) {
        refresh(paths, ch);
    }
}

/// Record `version` as accepted.
fn accept(paths: &HostPaths, channel: &str, version: &str) -> Result<()> {
    let mut cache = read_cache(paths).unwrap_or_else(|| Cache {
        checked_at: now_secs(),
        channel: channel.to_string(),
        latest: version.to_string(),
        accepted: None,
    });
    cache.accepted = Some(version.to_string());
    write_cache(paths, &cache)
}

/// What a session start does about Claude Code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Nothing,
    /// No terminal: one line, no question.
    Hint {
        from: String,
        to: String,
    },
    Ask {
        from: String,
        to: String,
    },
}

/// Pure decision behind the start prompt. Nothing is offered while nothing is
/// accepted yet: the first build takes the newest version by itself.
pub fn decide(channel: Option<&str>, enabled: bool, tty: bool, cache: Option<&Cache>) -> Decision {
    let (Some(ch), true, Some(cache)) = (channel, enabled, cache) else {
        return Decision::Nothing;
    };
    let Some(accepted) = &cache.accepted else {
        return Decision::Nothing;
    };
    if cache.channel != ch || !is_newer(&cache.latest, accepted) {
        return Decision::Nothing;
    }
    let (from, to) = (accepted.clone(), cache.latest.clone());
    if tty {
        Decision::Ask { from, to }
    } else {
        Decision::Hint { from, to }
    }
}

/// Offer a newer Claude Code at the start of a session. A "y" accepts the
/// version; images pick it up when they are built next, this one right away.
pub fn prompt(paths: &HostPaths, cfg: &Config, tty: bool) {
    let ch = channel(&cfg.claude_version);
    let enabled = cfg.update_check && !suppressed_by_env();
    match decide(ch, enabled, tty, read_cache(paths).as_ref()) {
        Decision::Nothing => {}
        Decision::Hint { from, to } => {
            eprintln!("claude_here: Claude Code {from} -> {to} available  (claude_here update)");
        }
        Decision::Ask { from, to } => {
            eprint!("claude_here: Claude Code {from} -> {to} available. Update now? [y/N] ");
            let _ = std::io::stderr().flush();
            let mut line = String::new();
            if std::io::stdin().read_line(&mut line).is_err() {
                return;
            }
            if !matches!(line.trim(), "y" | "Y" | "yes") {
                eprintln!("claude_here: skipped; update later with: claude_here update");
                return;
            }
            let Some(ch) = ch else { return };
            if let Err(e) = accept(paths, ch, &to) {
                eprintln!("claude_here: could not record Claude Code {to}: {e:#}");
            }
        }
    }
}

/// The version the Claude layer is built with: the pin, else the accepted
/// version. With nothing accepted yet (first start) the newest release is
/// taken and recorded; it is fetched right away when the cache has none,
/// unless update checks are off. Without any answer the channel name goes to
/// `install.sh`, which resolves it during the build.
pub fn version_for_build(paths: &HostPaths, cfg: &Config) -> String {
    let Some(ch) = channel(&cfg.claude_version) else {
        return cfg.claude_version.clone();
    };
    let cache = read_cache(paths);
    if let Some(accepted) = cache.as_ref().and_then(|c| c.accepted.clone()) {
        return accepted;
    }
    let known = cache.filter(|c| c.channel == ch).map(|c| c.latest);
    let latest = known.or_else(|| {
        (cfg.update_check && !suppressed_by_env())
            .then(|| refresh(paths, ch))
            .flatten()
    });
    match latest {
        Some(v) => {
            let _ = accept(paths, ch, &v);
            v
        }
        None => ch.to_string(),
    }
}

/// `claude_here update`: fetch now and accept the result, no question asked.
/// An explicit request, so it runs even with update checks switched off.
pub fn update_now(paths: &HostPaths, cfg: &Config) -> Result<String> {
    let Some(ch) = channel(&cfg.claude_version) else {
        return Ok(cfg.claude_version.clone());
    };
    let Some(latest) = refresh(paths, ch) else {
        bail!("could not read the current Claude Code release from {RELEASES}/{ch}");
    };
    accept(paths, ch, &latest)?;
    Ok(latest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(channel: &str, latest: &str, accepted: Option<&str>) -> Cache {
        Cache {
            checked_at: 1_000_000,
            channel: channel.into(),
            latest: latest.into(),
            accepted: accepted.map(Into::into),
        }
    }

    #[test]
    fn only_channels_are_followed() {
        assert_eq!(channel("latest"), Some("latest"));
        assert_eq!(channel("stable"), Some("stable"));
        assert_eq!(channel("2.1.274"), None);
    }

    #[test]
    fn versions_have_the_install_sh_shape() {
        assert!(is_version("2.1.282"));
        assert!(is_version("2.1.282-beta.1"));
        assert!(!is_version("2.1"));
        assert!(!is_version("2.1.x"));
        assert!(!is_version("<!doctype html>"));
        assert!(!is_version(""));
        assert!(!is_version("2.1.282-"));
        assert!(!is_version("2.1.282-a b"));
    }

    #[test]
    fn a_newer_release_is_offered_with_a_terminal_and_hinted_without() {
        let c = cache("latest", "2.1.282", Some("2.1.274"));
        let (from, to) = ("2.1.274".to_string(), "2.1.282".to_string());
        assert_eq!(
            decide(Some("latest"), true, true, Some(&c)),
            Decision::Ask {
                from: from.clone(),
                to: to.clone()
            }
        );
        assert_eq!(
            decide(Some("latest"), true, false, Some(&c)),
            Decision::Hint { from, to }
        );
    }

    #[test]
    fn nothing_is_offered_when_there_is_nothing_to_decide() {
        let newer = cache("latest", "2.1.282", Some("2.1.274"));
        // Pinned, or checks switched off.
        assert_eq!(decide(None, true, true, Some(&newer)), Decision::Nothing);
        assert_eq!(
            decide(Some("latest"), false, true, Some(&newer)),
            Decision::Nothing
        );
        // No cache, nothing accepted yet, already current, or another channel.
        assert_eq!(decide(Some("latest"), true, true, None), Decision::Nothing);
        let fresh = cache("latest", "2.1.282", None);
        assert_eq!(
            decide(Some("latest"), true, true, Some(&fresh)),
            Decision::Nothing
        );
        let current = cache("latest", "2.1.282", Some("2.1.282"));
        assert_eq!(
            decide(Some("latest"), true, true, Some(&current)),
            Decision::Nothing
        );
        assert_eq!(
            decide(Some("stable"), true, true, Some(&newer)),
            Decision::Nothing
        );
    }

    #[test]
    fn staleness_follows_clock_and_channel() {
        let c = cache("latest", "2.1.282", None);
        assert!(!is_stale(Some(&c), "latest", 1_000_000 + MAX_AGE_SECS - 1));
        assert!(is_stale(Some(&c), "latest", 1_000_000 + MAX_AGE_SECS));
        assert!(is_stale(Some(&c), "stable", 1_000_000));
        assert!(is_stale(None, "latest", 0));
    }

    fn paths() -> Result<(tempfile::TempDir, HostPaths)> {
        let dir = tempfile::tempdir()?;
        let paths = HostPaths::new(dir.path().join("home"), dir.path().join("cfg"));
        std::fs::create_dir_all(&paths.config_dir)?;
        Ok((dir, paths))
    }

    fn config(claude_version: &str, update_check: bool) -> Config {
        let mut cfg = Config::from(crate::config::ConfigFile::default());
        cfg.claude_version = claude_version.into();
        cfg.update_check = update_check;
        cfg
    }

    #[test]
    fn builds_use_the_pin_or_the_accepted_version() -> Result<()> {
        let (_dir, paths) = paths()?;
        assert_eq!(version_for_build(&paths, &config("2.1.1", true)), "2.1.1");
        write_cache(&paths, &cache("latest", "2.1.282", Some("2.1.274")))?;
        // Newer is known but not accepted: builds stay on the accepted one.
        assert_eq!(
            version_for_build(&paths, &config("latest", true)),
            "2.1.274"
        );
        Ok(())
    }

    #[test]
    fn the_first_build_takes_and_records_the_newest_known() -> Result<()> {
        let (_dir, paths) = paths()?;
        write_cache(&paths, &cache("latest", "2.1.282", None))?;
        assert_eq!(
            version_for_build(&paths, &config("latest", false)),
            "2.1.282"
        );
        assert_eq!(
            read_cache(&paths).and_then(|c| c.accepted).as_deref(),
            Some("2.1.282")
        );
        Ok(())
    }

    #[test]
    fn without_an_answer_the_channel_goes_to_install_sh() -> Result<()> {
        let (_dir, paths) = paths()?;
        // Checks off: no fetch, nothing recorded.
        assert_eq!(
            version_for_build(&paths, &config("latest", false)),
            "latest"
        );
        assert!(read_cache(&paths).is_none());
        Ok(())
    }

    #[test]
    fn accepting_keeps_what_the_check_found() -> Result<()> {
        let (_dir, paths) = paths()?;
        write_cache(&paths, &cache("latest", "2.1.282", Some("2.1.274")))?;
        accept(&paths, "latest", "2.1.282")?;
        assert_eq!(
            read_cache(&paths),
            Some(cache("latest", "2.1.282", Some("2.1.282")))
        );
        Ok(())
    }
}
