//! Session identifiers and the JSON blob handed to Claude inside the container.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::GitMode;

/// Civil date/time from unix seconds (UTC), no external crate needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Civil {
    /// Howard Hinnant's `civil_from_days`.
    pub fn from_unix(secs: i64) -> Self {
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if m <= 2 { y + 1 } else { y };
        Self {
            year,
            month: u32::try_from(m).unwrap_or(1),
            day: u32::try_from(d).unwrap_or(1),
            hour: u32::try_from(rem / 3600).unwrap_or(0),
            minute: u32::try_from((rem % 3600) / 60).unwrap_or(0),
            second: u32::try_from(rem % 60).unwrap_or(0),
        }
    }

    /// `YYYYMMDD-HHMMSS`
    pub fn compact(&self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// `YYYY-MM-DD HH:MM`
    pub fn human(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

/// Current unix time in seconds.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// New session id: `YYYYMMDD-HHMMSS-xxxx` (UTC + 4 hex digits of entropy).
pub fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let salt = (nanos ^ std::process::id().rotate_left(7)) & 0xffff;
    format!("{}-{salt:04x}", Civil::from_unix(now_secs()).compact())
}

/// Parse the timestamp prefix of a session id back into unix seconds.
pub fn session_start_secs(session_id: &str) -> Option<i64> {
    let (date, rest) = session_id.split_once('-')?;
    let time = rest.get(..6)?;
    if date.len() != 8 {
        return None;
    }
    let year: i64 = date.get(..4)?.parse().ok()?;
    let month: i64 = date.get(4..6)?.parse().ok()?;
    let day: i64 = date.get(6..8)?.parse().ok()?;
    let hour: i64 = time.get(..2)?.parse().ok()?;
    let minute: i64 = time.get(2..4)?.parse().ok()?;
    let second: i64 = time.get(4..6)?.parse().ok()?;
    // days_from_civil
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Sanitize a directory name for use in a container name.
pub fn sanitize_name(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    while s.starts_with(['-', '.', '_']) {
        s.remove(0);
    }
    if s.is_empty() {
        s.push_str("work");
    }
    s.truncate(40);
    s
}

/// A mount as reported to Claude.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountInfo {
    pub host: PathBuf,
    pub container: PathBuf,
    pub mode: String,
}

/// JSON exported as `CLAUDE_HERE_SESSION_INFO` and stored next to the capture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionInfo {
    pub tool: String,
    pub version: String,
    pub session_id: String,
    pub image: String,
    /// Enabled toolchains in build order.
    pub toolchains: Vec<String>,
    pub git_mode: GitMode,
    pub yolo: bool,
    pub user: String,
    pub cwd_host: PathBuf,
    pub cwd: PathBuf,
    pub mounts: Vec<MountInfo>,
    pub net_capture: bool,
    pub ssh: bool,
    pub gh: bool,
}

impl SessionInfo {
    /// Text appended to Claude's system prompt so it knows its constraints.
    pub fn system_prompt(&self) -> String {
        let mut s = format!(
            "You are running inside a claude_here Docker sandbox (session {}). Container user '{}' has no sudo and no root. \
             The host working directory {} is mounted at {}; host home paths appear under /home/{}. \
             Only the mounted paths listed in CLAUDE_HERE_SESSION_INFO exist here. ",
            self.session_id,
            self.user,
            self.cwd_host.display(),
            self.cwd.display(),
            self.user
        );
        if self.toolchains.is_empty() {
            s.push_str(
                "No language toolchain is enabled in this image beyond what the base ships (git, python3, ripgrep, jq, graphviz, build-essential). ",
            );
        } else {
            s.push_str("Enabled toolchains: ");
            s.push_str(&self.toolchains.join(", "));
            s.push_str(". Their compilers, package managers and language servers are on PATH. ");
        }
        match self.git_mode {
            GitMode::Ro => s.push_str(
                "Git mode is 'ro': every .git directory is bind-mounted read-only and enforced by the kernel. Committing, staging, checking out, switching branches, stashing, resetting and pushing are impossible; do not attempt them, do not try to work around this. Report changed files and let the user commit. ",
            ),
            GitMode::Commit => s.push_str(
                "Git mode is 'commit': you may inspect, stage and commit locally. Checkout, switch, branch creation, reset, rebase, merge, stash, tag and any remote operation are denied by the git wrapper; do not attempt them or bypass the wrapper. ",
            ),
            GitMode::Full => s.push_str("Git mode is 'full': git is unrestricted. "),
        }
        if self.net_capture {
            s.push_str("All network traffic of this session is recorded for later review. ");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_conversion_matches_known_dates() {
        let c = Civil::from_unix(0);
        assert_eq!((c.year, c.month, c.day, c.hour), (1970, 1, 1, 0));
        // 2026-09-19 12:34:56 UTC
        let c = Civil::from_unix(1_789_821_296);
        assert_eq!(c.compact(), "20260919-123456");
        assert_eq!(
            session_start_secs("20260919-123456-ab12"),
            Some(1_789_821_296)
        );
    }

    #[test]
    fn round_trip_session_start() {
        for secs in [0_i64, 951_782_400, 1_700_000_000, 4_102_444_799] {
            let id = format!("{}-0000", Civil::from_unix(secs).compact());
            assert_eq!(session_start_secs(&id), Some(secs), "{id}");
        }
        assert_eq!(session_start_secs("garbage"), None);
    }

    #[test]
    fn session_id_shape() {
        let id = new_session_id();
        assert_eq!(id.len(), "20260919-123456-ab12".len(), "{id}");
        assert!(session_start_secs(&id).is_some());
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name("my project (x)"), "my-project--x-");
        assert_eq!(sanitize_name("..hidden"), "hidden");
        assert_eq!(sanitize_name(""), "work");
    }

    #[test]
    fn prompt_mentions_mode() {
        let info = SessionInfo {
            tool: "claude_here".into(),
            version: "0".into(),
            session_id: "s".into(),
            image: "claude_here:base".into(),
            toolchains: vec!["rust".into()],
            git_mode: GitMode::Ro,
            yolo: false,
            user: "ni".into(),
            cwd_host: "/home/axel/p".into(),
            cwd: "/home/ni/p".into(),
            mounts: vec![],
            net_capture: true,
            ssh: false,
            gh: false,
        };
        let p = info.system_prompt();
        assert!(p.contains("'ro'"));
        assert!(p.contains("Enabled toolchains: rust"));
        assert!(p.contains("/home/ni/p"));
        assert!(p.contains("recorded"));
    }
}
