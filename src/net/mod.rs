//! Host-side handling of per-session network captures.

pub mod report;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::HostPaths;
use crate::session::{SessionInfo, now_secs, session_start_secs};

/// One observed remote endpoint (written by `net-summary.sh` inside the container).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostEntry {
    pub host: String,
    pub ip: String,
    pub port: u16,
    pub connections: u64,
    pub bytes_out: u64,
    pub bytes_in: u64,
}

/// Summary produced at container exit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Summary {
    pub session_id: String,
    pub packets: u64,
    pub bytes_out: u64,
    pub bytes_in: u64,
    pub dns_queries: Vec<String>,
    pub hosts: Vec<HostEntry>,
    #[serde(default)]
    pub http_requests: Vec<String>,
}

/// Files that belong to one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFiles {
    pub session_id: String,
    pub pcap: PathBuf,
    pub summary: PathBuf,
    pub info: PathBuf,
}

impl SessionFiles {
    pub fn new(dir: &Path, session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            pcap: dir.join(format!("{session_id}.pcap")),
            summary: dir.join(format!("{session_id}.summary.json")),
            info: dir.join(format!("{session_id}.session.json")),
        }
    }

    pub fn load_summary(&self) -> Option<Summary> {
        let text = fs::read_to_string(&self.summary).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn load_info(&self) -> Option<SessionInfo> {
        let text = fs::read_to_string(&self.info).ok()?;
        serde_json::from_str(&text).ok()
    }
}

/// All sessions in the log directory, newest first.
pub fn list_sessions(dir: &Path) -> Vec<SessionFiles> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_suffix(".session.json")
                .or_else(|| name.strip_suffix(".pcap"))
                .map(str::to_string)
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids.reverse();
    ids.iter().map(|id| SessionFiles::new(dir, id)).collect()
}

/// Persist the session info next to the capture (host side, before docker starts).
pub fn write_session_info(paths: &HostPaths, info: &SessionInfo) -> Result<()> {
    let files = SessionFiles::new(&paths.net_log_dir(), &info.session_id);
    fs::create_dir_all(paths.net_log_dir())?;
    let text = serde_json::to_string_pretty(info).context("serializing session info")?;
    fs::write(&files.info, text).with_context(|| format!("writing {}", files.info.display()))
}

/// Human readable byte count.
pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    #[allow(clippy::cast_precision_loss)]
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// One-line summary printed after the container exits.
pub fn print_exit_summary(paths: &HostPaths, session_id: &str, info: &SessionInfo) {
    let files = SessionFiles::new(&paths.net_log_dir(), session_id);
    write_session_info(paths, info).ok();
    match files.load_summary() {
        Some(s) => eprintln!(
            "claude_here: net: {} host(s), {} connection(s), {} up / {} down — `claude_here net last` for details",
            s.hosts.len(),
            s.hosts.iter().map(|h| h.connections).sum::<u64>(),
            human_bytes(s.bytes_out),
            human_bytes(s.bytes_in)
        ),
        None if files.pcap.exists() => eprintln!(
            "claude_here: net: capture saved to {} (no summary)",
            files.pcap.display()
        ),
        None => eprintln!("claude_here: net: no capture produced"),
    }
}

/// Delete session files older than `retention_days`. Returns number of sessions removed.
pub fn prune(paths: &HostPaths, retention_days: u32) -> Result<usize> {
    prune_dir(&paths.net_log_dir(), retention_days, now_secs())
}

fn prune_dir(dir: &Path, retention_days: u32, now: i64) -> Result<usize> {
    let cutoff = now - i64::from(retention_days) * 86_400;
    let mut removed = 0;
    for files in list_sessions(dir) {
        let Some(start) = session_start_secs(&files.session_id) else {
            continue;
        };
        if start < cutoff {
            for p in [&files.pcap, &files.summary, &files.info] {
                if p.exists() {
                    fs::remove_file(p).with_context(|| format!("removing {}", p.display()))?;
                }
            }
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_humanized() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
        assert_eq!(human_bytes(3 * 1024 * 1024 + 1024 * 512), "3.5 MB");
    }

    #[test]
    fn lists_and_prunes_by_session_age() -> Result<()> {
        let dir = tempfile::tempdir()?;
        for id in ["20260101-000000-aaaa", "20260918-000000-bbbb"] {
            fs::write(dir.path().join(format!("{id}.pcap")), "")?;
            fs::write(dir.path().join(format!("{id}.session.json")), "{}")?;
        }
        fs::write(dir.path().join("20260917-000000-cccc.summary.json"), "{}")?;
        let sessions = list_sessions(dir.path());
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, "20260918-000000-bbbb");
        // now = 2026-09-19, retention 90 days -> January is gone
        let now = session_start_secs("20260919-000000-0000").unwrap_or(0);
        let removed = prune_dir(dir.path(), 90, now)?;
        assert_eq!(removed, 1);
        assert_eq!(list_sessions(dir.path()).len(), 1);
        Ok(())
    }
}
