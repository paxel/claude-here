//! Rendering of network summaries for the `net` subcommands.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL_CONDENSED};

use super::{HostEntry, SessionFiles, Summary, human_bytes, list_sessions};
use crate::config::Config;
use crate::docker::Docker;
use crate::paths::HostPaths;
use crate::session::{Civil, now_secs, session_start_secs};

fn table() -> Table {
    let mut t = Table::new();
    t.load_style(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic);
    t
}

fn session_time(id: &str) -> String {
    session_start_secs(id)
        .map(|s| Civil::from_unix(s).human())
        .unwrap_or_default()
}

/// Detailed view of one session.
pub fn print_session(files: &SessionFiles) -> Result<()> {
    let info = files.load_info();
    let summary = files.load_summary();
    println!(
        "session  {}  ({} UTC)",
        files.session_id,
        session_time(&files.session_id)
    );
    if let Some(i) = &info {
        println!(
            "project  {}\nimage    {}   git {}   {}",
            i.cwd_host.display(),
            i.image,
            i.git_mode,
            if i.yolo { "YOLO" } else { "" }
        );
    }
    let Some(s) = summary else {
        if files.pcap.exists() {
            println!("no summary; capture at {}", files.pcap.display());
            return Ok(());
        }
        bail!("no data for session {}", files.session_id);
    };
    println!(
        "traffic  {} packets, {} up / {} down, {} dns quer{}",
        s.packets,
        human_bytes(s.bytes_out),
        human_bytes(s.bytes_in),
        s.dns_queries.len(),
        if s.dns_queries.len() == 1 { "y" } else { "ies" }
    );
    let mut t = table();
    t.set_header(["host", "ip:port", "conns", "up", "down"]);
    let mut hosts = s.hosts.clone();
    hosts.sort_by_key(|h| std::cmp::Reverse(h.bytes_in + h.bytes_out));
    for h in hosts {
        t.add_row([
            h.host,
            format!("{}:{}", h.ip, h.port),
            h.connections.to_string(),
            human_bytes(h.bytes_out),
            human_bytes(h.bytes_in),
        ]);
    }
    println!("{t}");
    if !s.http_requests.is_empty() {
        println!("plain HTTP requests:");
        for r in &s.http_requests {
            println!("  {r}");
        }
    }
    if files.pcap.exists() {
        println!("pcap     {}", files.pcap.display());
    }
    Ok(())
}

/// Hosts aggregated over sessions of the last `days`.
pub fn print_top(dir: &Path, days: u32, project: Option<&Path>, limit: usize) -> Result<()> {
    let cutoff = now_secs() - i64::from(days) * 86_400;
    let mut agg: BTreeMap<String, (u64, u64, u64, u64)> = BTreeMap::new();
    let mut sessions = 0;
    for files in list_sessions(dir) {
        if session_start_secs(&files.session_id).is_none_or(|s| s < cutoff) {
            continue;
        }
        if let Some(p) = project
            && files.load_info().is_none_or(|i| i.cwd_host != p)
        {
            continue;
        }
        let Some(s) = files.load_summary() else {
            continue;
        };
        sessions += 1;
        let mut seen = std::collections::BTreeSet::new();
        for h in &s.hosts {
            let e = agg.entry(h.host.clone()).or_default();
            e.0 += h.connections;
            e.1 += h.bytes_out;
            e.2 += h.bytes_in;
            if seen.insert(h.host.clone()) {
                e.3 += 1;
            }
        }
    }
    let mut rows: Vec<(String, (u64, u64, u64, u64))> = agg.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.1 + r.1.2));
    println!(
        "{} session(s) in the last {days} day(s){}",
        sessions,
        project
            .map(|p| format!(" for {}", p.display()))
            .unwrap_or_default()
    );
    let mut t = table();
    t.set_header(["host", "sessions", "conns", "up", "down"]);
    for (host, (conns, up, down, sess)) in rows.into_iter().take(limit) {
        t.add_row([
            host,
            sess.to_string(),
            conns.to_string(),
            human_bytes(up),
            human_bytes(down),
        ]);
    }
    println!("{t}");
    Ok(())
}

/// Sessions whose summary mentions `needle` (host, ip or dns query).
pub fn print_grep(dir: &Path, needle: &str) -> Result<()> {
    let needle = needle.to_lowercase();
    let mut t = table();
    t.set_header(["session", "time (UTC)", "project", "match"]);
    let mut found = 0;
    for files in list_sessions(dir) {
        let Some(s) = files.load_summary() else {
            continue;
        };
        let hit = s
            .hosts
            .iter()
            .find(|h| h.host.to_lowercase().contains(&needle) || h.ip.contains(&needle))
            .map(|h: &HostEntry| {
                format!("{} ({}:{}, {} conns)", h.host, h.ip, h.port, h.connections)
            })
            .or_else(|| {
                s.dns_queries
                    .iter()
                    .find(|q| q.to_lowercase().contains(&needle))
                    .map(|q| format!("dns {q}"))
            });
        if let Some(m) = hit {
            found += 1;
            let project = files
                .load_info()
                .map(|i| i.cwd_host.display().to_string())
                .unwrap_or_default();
            t.add_row([
                files.session_id.clone(),
                session_time(&files.session_id),
                project,
                m,
            ]);
        }
    }
    if found == 0 {
        println!("no session contacted '{needle}'");
    } else {
        println!("{t}");
    }
    Ok(())
}

/// Recent sessions.
pub fn print_list(dir: &Path, limit: usize) -> Result<()> {
    let mut t = table();
    t.set_header(["session", "time (UTC)", "project", "hosts", "up", "down"]);
    for files in list_sessions(dir).into_iter().take(limit) {
        let info = files.load_info();
        let s: Summary = files.load_summary().unwrap_or_default();
        t.add_row([
            files.session_id.clone(),
            session_time(&files.session_id),
            info.map(|i| i.cwd_host.display().to_string())
                .unwrap_or_default(),
            s.hosts.len().to_string(),
            human_bytes(s.bytes_out),
            human_bytes(s.bytes_in),
        ]);
    }
    println!("{t}");
    Ok(())
}

/// Open a session's pcap in termshark inside the base image.
pub fn shark(paths: &HostPaths, cfg: &Config, session_id: &str) -> Result<()> {
    let files = SessionFiles::new(&paths.net_log_dir(), session_id);
    if !files.pcap.exists() {
        bail!("no capture at {}", files.pcap.display());
    }
    let docker = Docker::default();
    let facts = crate::run::HostFacts::gather(paths)?;
    let image = format!("{}:base", crate::image::REPO);
    docker
        .image_label(&image, crate::image::HASH_LABEL)
        .context("base image not built yet; run `claude_here build` first")?;
    let _ = cfg;
    let args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "-it".into(),
        "--network".into(),
        "none".into(),
        "--user".into(),
        format!("{}:{}", facts.uid, facts.gid),
        "-e".into(),
        "TERM".into(),
        "-e".into(),
        "HOME=/tmp".into(),
        "-v".into(),
        format!("{}:/capture.pcap:ro", files.pcap.display()),
        "--entrypoint".into(),
        "termshark".into(),
        image,
        "-r".into(),
        "/capture.pcap".into(),
    ];
    let code = docker.run_inherit(&args)?;
    if code != 0 {
        bail!("termshark exited with {code}");
    }
    Ok(())
}
