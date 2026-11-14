//! Rendering of network summaries for the `net` subcommands.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL_CONDENSED};

use super::{HostEntry, SessionFiles, Summary, human_bytes, list_sessions};
use crate::config::Config;
use crate::docker::Docker;
use crate::paths::HostPaths;
use crate::session::{Civil, now_secs, printable_session_id, session_start_secs};

fn table() -> Table {
    let mut t = Table::new();
    t.load_style(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic);
    t
}

fn session_time(id: &str) -> String {
    session_start_secs(id).map_or_default(|s| Civil::from_unix(s).human())
}

/// Detailed view of one session.
/// What was rendered before an error is printed too, as the header of the
/// "no data" case.
pub fn print_session(files: &SessionFiles, all: bool) -> Result<()> {
    let mut out = String::new();
    let result = render_session(files, all, &mut out);
    print!("{out}");
    result
}

/// Rows of the session view before `--all`.
const SESSION_ROWS: usize = 20;

/// One host of a session: every endpoint it was reached on, merged.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HostRow {
    host: String,
    /// `ip:port` when there was one endpoint, else how many.
    endpoints: String,
    connections: u64,
    bytes_out: u64,
    bytes_in: u64,
}

/// Merge the per-endpoint entries of a summary into one row per host, the
/// busiest first; ties keep the host names in order.
fn host_rows(hosts: &[HostEntry]) -> Vec<HostRow> {
    let mut merged: BTreeMap<&str, (Vec<String>, u64, u64, u64)> = BTreeMap::new();
    for h in hosts {
        let e = merged.entry(h.host.as_str()).or_default();
        let endpoint = format!("{}:{}", h.ip, h.port);
        if !e.0.contains(&endpoint) {
            e.0.push(endpoint);
        }
        e.1 = e.1.saturating_add(h.connections);
        e.2 = e.2.saturating_add(h.bytes_out);
        e.3 = e.3.saturating_add(h.bytes_in);
    }
    let mut rows: Vec<HostRow> = merged
        .into_iter()
        .map(
            |(host, (endpoints, connections, bytes_out, bytes_in))| HostRow {
                host: host.to_string(),
                endpoints: match endpoints.as_slice() {
                    [one] => one.clone(),
                    many => format!("{} endpoints", many.len()),
                },
                connections,
                bytes_out,
                bytes_in,
            },
        )
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.bytes_in.saturating_add(r.bytes_out)));
    rows
}

fn render_session(files: &SessionFiles, all: bool, out: &mut String) -> Result<()> {
    let info = files.load_info();
    let summary = files.load_summary();
    writeln!(
        out,
        "session  {}  ({} UTC)",
        printable_session_id(&files.session_id),
        session_time(&files.session_id)
    )?;
    if let Some(i) = &info {
        writeln!(
            out,
            "project  {}\nimage    {}   git {}   {}",
            i.cwd_host.display(),
            i.image,
            i.git_mode,
            if i.yolo { "YOLO" } else { "" }
        )?;
    }
    let Some(s) = summary else {
        if files.pcap.exists() {
            writeln!(out, "no summary; capture at {}", files.pcap.display())?;
            return Ok(());
        }
        bail!(
            "no data for session {}",
            printable_session_id(&files.session_id)
        );
    };
    writeln!(
        out,
        "traffic  {} packets, {} up / {} down, {} dns quer{}",
        s.packets,
        human_bytes(s.bytes_out),
        human_bytes(s.bytes_in),
        s.dns_queries.len(),
        if s.dns_queries.len() == 1 { "y" } else { "ies" }
    )?;
    let rows = host_rows(&s.hosts);
    let shown = if all { rows.len() } else { SESSION_ROWS };
    let mut t = table();
    t.set_header(["host", "endpoint", "conns", "up", "down"]);
    for r in rows.iter().take(shown) {
        t.add_row([
            r.host.clone(),
            r.endpoints.clone(),
            r.connections.to_string(),
            human_bytes(r.bytes_out),
            human_bytes(r.bytes_in),
        ]);
    }
    writeln!(out, "{t}")?;
    let hidden = rows.len().saturating_sub(shown);
    if hidden > 0 {
        writeln!(out, "+{hidden} more host(s) (`--all`)")?;
    }
    match (s.http_requests.len(), all) {
        (0, _) => {}
        (n, false) => writeln!(out, "plain HTTP requests: {n} (`--all` lists them)")?,
        (_, true) => {
            writeln!(out, "plain HTTP requests:")?;
            for r in &s.http_requests {
                writeln!(out, "  {r}")?;
            }
        }
    }
    if files.pcap.exists() {
        writeln!(out, "pcap     {}", files.pcap.display())?;
    }
    Ok(())
}

/// Hosts aggregated over sessions of the last `days`.
pub fn print_top(dir: &Path, days: u32, project: Option<&Path>, limit: usize) -> Result<()> {
    print!("{}", render_top(dir, days, project, limit, now_secs())?);
    Ok(())
}

fn render_top(
    dir: &Path,
    days: u32,
    project: Option<&Path>,
    limit: usize,
    now: i64,
) -> Result<String> {
    let mut out = String::new();
    let cutoff = now.saturating_sub(i64::from(days).saturating_mul(86_400));
    let mut agg: BTreeMap<String, (u64, u64, u64, u64)> = BTreeMap::new();
    let mut sessions = 0usize;
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
        sessions = sessions.saturating_add(1);
        let mut seen = std::collections::BTreeSet::new();
        for h in &s.hosts {
            let e = agg.entry(h.host.clone()).or_default();
            e.0 = e.0.saturating_add(h.connections);
            e.1 = e.1.saturating_add(h.bytes_out);
            e.2 = e.2.saturating_add(h.bytes_in);
            if seen.insert(h.host.clone()) {
                e.3 = e.3.saturating_add(1);
            }
        }
    }
    let mut rows: Vec<(String, (u64, u64, u64, u64))> = agg.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.1.saturating_add(r.1.2)));
    writeln!(
        out,
        "{} session(s) in the last {days} day(s){}",
        sessions,
        project.map_or_default(|p| format!(" for {}", p.display()))
    )?;
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
    writeln!(out, "{t}")?;
    Ok(out)
}

/// Sessions whose summary mentions `needle` (host, ip or dns query).
pub fn print_grep(dir: &Path, needle: &str) -> Result<()> {
    print!("{}", render_grep(dir, needle)?);
    Ok(())
}

fn render_grep(dir: &Path, needle: &str) -> Result<String> {
    let mut out = String::new();
    let needle = needle.to_lowercase();
    let mut t = table();
    t.set_header(["session", "time (UTC)", "project", "match"]);
    let mut found = 0usize;
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
            found = found.saturating_add(1);
            let project = files
                .load_info()
                .map_or_default(|i| i.cwd_host.display().to_string());
            t.add_row([
                printable_session_id(&files.session_id).to_string(),
                session_time(&files.session_id),
                project,
                m,
            ]);
        }
    }
    if found == 0 {
        writeln!(out, "no session contacted '{needle}'")?;
    } else {
        writeln!(out, "{t}")?;
    }
    Ok(out)
}

/// Recent sessions.
pub fn print_list(dir: &Path, limit: usize) -> Result<()> {
    print!("{}", render_list(dir, limit)?);
    Ok(())
}

fn render_list(dir: &Path, limit: usize) -> Result<String> {
    let mut out = String::new();
    let mut t = table();
    t.set_header(["session", "time (UTC)", "project", "hosts", "up", "down"]);
    for files in list_sessions(dir).into_iter().take(limit) {
        let info = files.load_info();
        let s: Summary = files.load_summary().unwrap_or_default();
        t.add_row([
            printable_session_id(&files.session_id).to_string(),
            session_time(&files.session_id),
            info.map_or_default(|i| i.cwd_host.display().to_string()),
            s.hosts.len().to_string(),
            human_bytes(s.bytes_out),
            human_bytes(s.bytes_in),
        ]);
    }
    writeln!(out, "{t}")?;
    Ok(out)
}

/// Open a session's pcap in termshark inside the base image.
pub fn shark(paths: &HostPaths, cfg: &Config, session_id: &str) -> Result<()> {
    let files = SessionFiles::new(&paths.net_log_dir(), session_id);
    if !files.pcap.exists() {
        bail!("no capture at {}", files.pcap.display());
    }
    let docker = Docker::default();
    let facts = crate::run::HostFacts::gather(paths, cfg)?;
    let image = crate::image::base_tag(&facts.uid);
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::config::{CloudMode, GitMode, NetMode};
    use crate::session::SessionInfo;

    fn host(name: &str, ip: &str, connections: u64, bytes_out: u64, bytes_in: u64) -> HostEntry {
        HostEntry {
            host: name.into(),
            ip: ip.into(),
            port: 443,
            connections,
            bytes_out,
            bytes_in,
        }
    }

    fn info(session_id: &str, project: &str) -> SessionInfo {
        SessionInfo {
            tool: "claude_here".into(),
            version: "0".into(),
            session_id: session_id.into(),
            image: "claude_here:base".into(),
            toolchains: vec![],
            mcp: vec![],
            git_mode: GitMode::Ro,
            cloud_mode: CloudMode::None,
            net_mode: NetMode::Full,
            yolo: false,
            user: "ni".into(),
            cwd_host: PathBuf::from(project),
            cwd: "/home/ni/p".into(),
            mounts: vec![],
            net_capture: true,
            ssh: false,
            gh: false,
        }
    }

    fn write_summary(dir: &Path, summary: &Summary) -> Result<()> {
        let files = SessionFiles::new(dir, &summary.session_id);
        fs::write(files.summary, serde_json::to_string(summary)?)?;
        Ok(())
    }

    fn write_info(dir: &Path, info: &SessionInfo) -> Result<()> {
        let files = SessionFiles::new(dir, &info.session_id);
        fs::write(files.info, serde_json::to_string(info)?)?;
        Ok(())
    }

    fn session(files: &SessionFiles) -> Result<String> {
        session_view(files, false)
    }

    fn session_view(files: &SessionFiles, all: bool) -> Result<String> {
        let mut out = String::new();
        render_session(files, all, &mut out)?;
        Ok(out)
    }

    /// Trimmed cells of a table row, outer border included as empty cells.
    fn cells(row: &str) -> Vec<&str> {
        row.split(['│', '┆']).map(str::trim).collect()
    }

    fn summary(session_id: &str, hosts: Vec<HostEntry>, dns: &[&str]) -> Summary {
        Summary {
            session_id: session_id.into(),
            packets: 10,
            bytes_out: hosts.iter().map(|h| h.bytes_out).sum(),
            bytes_in: hosts.iter().map(|h| h.bytes_in).sum(),
            dns_queries: dns.iter().map(ToString::to_string).collect(),
            hosts,
            http_requests: vec![],
        }
    }

    #[test]
    fn session_shows_project_traffic_hosts_and_capture() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let id = "20261110-120000-aaaa";
        let mut info = info(id, "/home/axel/p");
        info.yolo = true;
        write_info(dir.path(), &info)?;
        let mut s = summary(
            id,
            vec![
                host("small.example", "10.0.0.1", 1, 10, 10),
                host("big.example", "10.0.0.2", 3, 2048, 4096),
            ],
            &["big.example"],
        );
        s.http_requests = vec!["GET http://plain.example/".into()];
        write_summary(dir.path(), &s)?;
        let files = SessionFiles::new(dir.path(), id);
        fs::write(&files.pcap, "")?;

        let out = session(&files)?;
        assert!(out.contains("session  20261110-120000-aaaa  (2026-11-10 12:00 UTC)"));
        assert!(out.contains("project  /home/axel/p"));
        assert!(out.contains("git ro"));
        assert!(out.contains("YOLO"));
        assert!(out.contains("10 packets, 2.0 KB up / 4.0 KB down, 1 dns query\n"));
        assert!(out.contains("10.0.0.2:443"));
        assert!(out.contains("plain HTTP requests: 1 (`--all` lists them)"));
        assert!(!out.contains("GET http://plain.example/"));
        let all = session_view(&files, true)?;
        assert!(all.contains("plain HTTP requests:\n  GET http://plain.example/"));
        assert!(out.contains(&format!("pcap     {}", files.pcap.display())));
        let big = out.find("big.example").unwrap_or(usize::MAX);
        let small = out.find("small.example").unwrap_or(0);
        assert!(big < small, "hosts sorted by traffic, largest first");
        Ok(())
    }

    #[test]
    fn session_without_summary_points_at_the_capture() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let files = SessionFiles::new(dir.path(), "20261110-120000-aaaa");
        fs::write(&files.pcap, "")?;
        let out = session(&files)?;
        assert!(out.contains("no summary; capture at"));
        assert!(!out.contains("project"));
        Ok(())
    }

    #[test]
    fn session_without_any_file_is_an_error() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let id = "20261110-120000-aaaa";
        write_info(dir.path(), &info(id, "/home/axel/p"))?;
        let files = SessionFiles::new(dir.path(), id);
        let mut out = String::new();
        let err = render_session(&files, false, &mut out)
            .err()
            .map(|e| e.to_string());
        assert_eq!(
            err.as_deref(),
            Some("no data for session 20261110-120000-aaaa")
        );
        assert!(out.contains("project  /home/axel/p"), "header kept: {out}");
        let printed = print_session(&files, false).err().map(|e| e.to_string());
        assert_eq!(printed, err);
        Ok(())
    }

    #[test]
    fn dns_query_count_is_plural_unless_one() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let id = "20261110-120000-aaaa";
        write_summary(dir.path(), &summary(id, vec![], &[]))?;
        let out = session(&SessionFiles::new(dir.path(), id))?;
        assert!(out.contains("0 dns queries"));
        assert!(!out.contains("pcap "));
        Ok(())
    }

    #[test]
    fn top_aggregates_recent_sessions_of_the_project() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let now = session_start_secs("20261114-000000-0000").unwrap_or(0);
        // Two recent sessions of /p, one recent of /q, one too old.
        for (id, project, hosts) in [
            (
                "20261112-000000-aaaa",
                "/p",
                vec![
                    host("api.example", "10.0.0.1", 2, 100, 200),
                    host("api.example", "10.0.0.9", 1, 1, 1),
                ],
            ),
            (
                "20261113-000000-bbbb",
                "/p",
                vec![
                    host("api.example", "10.0.0.1", 3, 100, 200),
                    host("cdn.example", "10.0.0.2", 1, 1, 1),
                ],
            ),
            (
                "20261113-000000-cccc",
                "/q",
                vec![host("other.example", "10.0.0.3", 1, 1, 1)],
            ),
            (
                "20261001-000000-dddd",
                "/p",
                vec![host("old.example", "10.0.0.4", 1, 1, 1)],
            ),
        ] {
            write_info(dir.path(), &info(id, project))?;
            write_summary(dir.path(), &summary(id, hosts, &[]))?;
        }
        // A session of /p without a summary is not counted.
        write_info(dir.path(), &info("20261113-100000-eeee", "/p"))?;

        let out = render_top(dir.path(), 7, Some(Path::new("/p")), 10, now)?;
        assert!(out.starts_with("2 session(s) in the last 7 day(s) for /p\n"));
        let api = out
            .lines()
            .find(|l| l.contains("api.example"))
            .unwrap_or_default();
        assert_eq!(cells(api).get(2), Some(&"2"), "sessions: {api}");
        assert_eq!(cells(api).get(3), Some(&"6"), "conns: {api}");
        assert!(out.contains("cdn.example"));
        assert!(!out.contains("other.example"));
        assert!(!out.contains("old.example"));

        let all = render_top(dir.path(), 7, None, 1, now)?;
        assert!(all.starts_with("3 session(s) in the last 7 day(s)\n"));
        assert!(all.contains("api.example"));
        assert!(
            !all.contains("cdn.example"),
            "limit 1 keeps the busiest host"
        );
        Ok(())
    }

    #[test]
    fn grep_matches_host_ip_and_dns_case_insensitively() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let a = "20261112-000000-aaaa";
        let b = "20261113-000000-bbbb";
        write_info(dir.path(), &info(a, "/home/axel/p"))?;
        write_summary(
            dir.path(),
            &summary(a, vec![host("API.example", "10.0.0.1", 2, 1, 1)], &[]),
        )?;
        write_summary(
            dir.path(),
            &summary(
                b,
                vec![host("cdn.example", "192.168.1.7", 1, 1, 1)],
                &["Lookup.Example"],
            ),
        )?;
        // Listed through its capture; it has no session info, so no project.
        fs::write(SessionFiles::new(dir.path(), b).pcap, "")?;

        let by_host = render_grep(dir.path(), "api.EXAMPLE")?;
        assert!(by_host.contains(a));
        assert!(by_host.contains("/home/axel/p"));
        assert!(by_host.contains("API.example (10.0.0.1:443, 2 conns)"));
        assert!(!by_host.contains(b));

        let by_ip = render_grep(dir.path(), "192.168.1")?;
        assert!(by_ip.contains(b));
        assert!(by_ip.contains("2026-11-13 00:00"));

        let by_dns = render_grep(dir.path(), "lookup")?;
        assert!(by_dns.contains("dns Lookup.Example"));

        let none = render_grep(dir.path(), "Nowhere")?;
        assert_eq!(none, "no session contacted 'nowhere'\n");
        Ok(())
    }

    #[test]
    fn list_shows_newest_sessions_first_up_to_the_limit() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let old = "20261112-000000-aaaa";
        let new = "20261113-000000-bbbb";
        write_info(dir.path(), &info(old, "/home/axel/p"))?;
        write_summary(
            dir.path(),
            &summary(
                old,
                vec![
                    host("a.example", "10.0.0.1", 1, 1024, 2048),
                    host("b.example", "10.0.0.2", 1, 0, 0),
                ],
                &[],
            ),
        )?;
        fs::write(SessionFiles::new(dir.path(), new).pcap, "")?;

        let out = render_list(dir.path(), 10)?;
        assert!(out.contains("/home/axel/p"));
        assert!(out.contains("1.0 KB"));
        assert!(out.contains("2.0 KB"));
        let old_row = out.lines().find(|l| l.contains(old)).unwrap_or_default();
        assert_eq!(cells(old_row).get(4), Some(&"2"), "host count: {old_row}");
        let (new_at, old_at) = (out.find(new), out.find(old));
        assert!(new_at.is_some() && new_at < old_at, "newest first");

        let limited = render_list(dir.path(), 1)?;
        assert!(limited.contains(new));
        assert!(!limited.contains(old));
        Ok(())
    }

    #[test]
    fn shark_needs_a_capture() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = HostPaths::new(dir.path().join("home"), dir.path().join("config"));
        let layers =
            crate::config::Layers::load(dir.path().join("g.toml"), dir.path().join("p.toml"))?;
        let cfg = layers.resolve(crate::config::ConfigFile::default());
        let err = shark(&paths, &cfg, "20261110-120000-aaaa")
            .err()
            .map_or_default(|e| e.to_string());
        assert!(err.starts_with("no capture at "), "{err}");
        Ok(())
    }

    #[test]
    fn hosts_merge_their_endpoints_into_one_row() {
        let rows = host_rows(&[
            host("cdn.example", "10.0.0.1", 2, 100, 1000),
            host("cdn.example", "10.0.0.2", 1, 50, 500),
            host("cdn.example", "10.0.0.1", 1, 1, 1),
            host("api.example", "10.0.0.9", 4, 1, 1),
        ]);
        assert_eq!(
            rows,
            vec![
                HostRow {
                    host: "cdn.example".into(),
                    endpoints: "2 endpoints".into(),
                    connections: 4,
                    bytes_out: 151,
                    bytes_in: 1501,
                },
                HostRow {
                    host: "api.example".into(),
                    endpoints: "10.0.0.9:443".into(),
                    connections: 4,
                    bytes_out: 1,
                    bytes_in: 1,
                },
            ]
        );
    }

    #[test]
    fn session_shows_the_busiest_hosts_until_all() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let id = "20261110-120000-aaaa";
        let hosts = (0..25u64)
            .map(|i| host(&format!("h{i:02}.example"), "10.0.0.1", 1, i, i))
            .collect();
        write_summary(dir.path(), &summary(id, hosts, &[]))?;
        let files = SessionFiles::new(dir.path(), id);

        let top = session_view(&files, false)?;
        assert!(top.contains("h24.example"), "busiest shown");
        assert!(top.contains("h05.example"));
        assert!(!top.contains("h04.example"), "21st and later hidden");
        assert!(top.contains("+5 more host(s) (`--all`)"));
        assert!(!top.contains("plain HTTP"), "no requests, no line");

        let all = session_view(&files, true)?;
        assert!(all.contains("h00.example"));
        assert!(!all.contains("more host"));
        Ok(())
    }

    #[test]
    fn printers_write_what_they_render() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let id = "20261112-000000-aaaa";
        write_summary(
            dir.path(),
            &summary(id, vec![host("a.example", "10.0.0.1", 1, 1, 1)], &[]),
        )?;
        let files = SessionFiles::new(dir.path(), id);
        fs::write(&files.pcap, "")?;
        print_session(&files, true)?;
        print_top(dir.path(), 7, None, 10)?;
        print_grep(dir.path(), "a.example")?;
        print_list(dir.path(), 10)?;
        Ok(())
    }
}
