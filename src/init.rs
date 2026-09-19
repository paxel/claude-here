//! `claude_here init`: config skeleton, token, seeded container home, shell integration.

use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::cli::InitArgs;
use crate::config::{ConfigFile, DEFAULT_USER};
use crate::image::USER_DOCKERFILE_TEMPLATE;
use crate::paths::{HostPaths, PathRewriter};

/// Commented starter written when no global config exists.
pub const CONFIG_TEMPLATE: &str = "\
# claude_here global configuration. Project files (.claude_here/config.toml)
# override these values; lists (env, mounts, docker_args) are appended.
#
# image = \"base\"              # base | rust | jvm | <custom local image>
# user = \"ni\"                 # container user name; set to your host user for identical paths
# git_mode = \"ro\"             # ro | commit | full
# ssh = false                   # forward ssh agent (git mode full only)
# gh = false                    # mount gh config read-only (git mode full only)
# net_capture = true
# net_retention_days = 90
# claude_version = \"latest\"   # pinned Claude Code version for the base image
# memory = \"8g\"
# cpus = 4
# env = [\"EDITOR=nano\", \"MY_HOST_VAR\"]
# docker_args = []
#
# [[mounts]]
# path = \"~/shared\"
# mode = \"ro\"                 # ro | rw
# target = \"/shared\"          # optional, defaults to the home-rewritten host path
#
# [caches]
# isolated = false              # true: use ~/.config/claude_here/cache instead of host caches
# m2 = \"~/.m2\"
# gradle = \"~/.gradle\"
# cargo = \"~/.cargo\"
# m2_exclude = [\"settings.xml\"]
# gradle_exclude = [\"gradle.properties\"]
#
# [tls]
# truststore = \"~/certs/cacerts\"
# truststore_password = \"changeit\"
";

/// Marker written after the first successful seed.
const SEED_MARKER: &str = ".claude_here_seeded";
/// Files larger than this are copied verbatim without path rewriting.
const REWRITE_LIMIT: u64 = 2 * 1024 * 1024;

pub fn run(paths: &HostPaths, args: &InitArgs) -> Result<()> {
    fs::create_dir_all(&paths.config_dir)
        .with_context(|| format!("creating {}", paths.config_dir.display()))?;
    ensure_file(&paths.global_config(), CONFIG_TEMPLATE)?;
    ensure_file(&paths.user_dockerfile(), USER_DOCKERFILE_TEMPLATE)?;
    fs::create_dir_all(paths.net_log_dir())?;
    fs::create_dir_all(paths.container_home())?;
    println!("config:     {}", paths.global_config().display());
    println!("dockerfile: {}", paths.user_dockerfile().display());

    if !args.no_token {
        setup_token(paths, args)?;
    }

    let user = ConfigFile::load(&paths.global_config())?
        .user
        .unwrap_or_else(|| DEFAULT_USER.to_string());
    seed_home(paths, &user, &args.seed, args.reseed)?;

    if !args.no_shell {
        install_fish_completions(paths)?;
    }
    offer_global_gitignore(paths, args.yes)?;
    println!("done. Try: cd <project> && claude_here");
    Ok(())
}

fn ensure_file(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

fn setup_token(paths: &HostPaths, args: &InitArgs) -> Result<()> {
    let token_file = paths.token_file();
    if args.token.is_none() && !args.token_stdin && token_file_valid(&token_file) {
        println!("token:      {} (exists, keeping)", token_file.display());
        return Ok(());
    }
    let token = if let Some(t) = &args.token {
        t.trim().to_string()
    } else if args.token_stdin {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        s.trim().to_string()
    } else {
        obtain_token_interactively()?
    };
    if token.is_empty() {
        bail!("empty token");
    }
    write_token(&token_file, &token)?;
    println!("token:      {} (written, mode 600)", token_file.display());
    Ok(())
}

/// An existing token file counts only when it holds a real-looking token.
fn token_file_valid(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|t| extract_token(&t))
        .is_some()
}

/// Runs `claude setup-token` with the terminal attached, then asks for the
/// token it printed (the flow is interactive; its output is not scraped).
fn obtain_token_interactively() -> Result<String> {
    println!("running `claude setup-token` (opens a browser; requires a Claude subscription)...");
    let status = Command::new("claude")
        .arg("setup-token")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("running `claude setup-token` (is claude installed on the host?)")?;
    if !status.success() {
        eprintln!("claude setup-token exited with {status}; you can still paste a token");
    }
    print!("paste the token printed above: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    match extract_token(&line) {
        Some(t) => Ok(t),
        None => bail!("no token found in input (expected something starting with sk-ant-)"),
    }
}

/// Find an `sk-ant-...` token in arbitrary text.
pub fn extract_token(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .find(|w| w.starts_with("sk-ant-") && w.len() > 20)
        .map(str::to_string)
}

/// Write the docker `--env-file` formatted token with mode 0600.
pub fn write_token(path: &Path, token: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("CLAUDE_CODE_OAUTH_TOKEN={token}\n"))
        .with_context(|| format!("writing {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// Host `~/.claude` (or `$CLAUDE_CONFIG_DIR`).
fn host_claude_dir(paths: &HostPaths) -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map_or_else(|| paths.home.join(".claude"), PathBuf::from)
}

fn seed_home(paths: &HostPaths, user: &str, items: &[String], reseed: bool) -> Result<()> {
    let dest = paths.container_home();
    let marker = dest.join(SEED_MARKER);
    if marker.exists() && !reseed {
        println!(
            "home:       {} (already seeded; --reseed to redo)",
            dest.display()
        );
        return Ok(());
    }
    let src_root = host_claude_dir(paths);
    let rw = PathRewriter::new(&paths.home, user);
    let mut copied = Vec::new();
    for item in items.iter().filter(|i| !i.trim().is_empty()) {
        let src = src_root.join(item);
        if !src.exists() {
            continue;
        }
        copy_tree(&src, &dest.join(item), &rw)?;
        copied.push(item.clone());
    }
    fs::write(&marker, format!("{}\n", copied.join(",")))?;
    println!(
        "home:       {} (seeded: {})",
        dest.display(),
        if copied.is_empty() {
            "nothing found".to_string()
        } else {
            copied.join(", ")
        }
    );
    if plugins_need_node(&dest.join("plugins")) {
        println!(
            "note:       seeded plugin hooks use node/npx, which the base image does not ship;\n            uncomment the nodejs line in {} to add it",
            paths.user_dockerfile().display()
        );
    }
    Ok(())
}

/// Do any seeded plugin hook definitions call node or npx?
fn plugins_need_node(plugins_dir: &Path) -> bool {
    fn walk(dir: &Path, depth: usize) -> bool {
        if depth > 6 {
            return false;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return false;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if walk(&path, depth + 1) {
                    return true;
                }
            } else if path.file_name().is_some_and(|n| n == "hooks.json")
                && fs::read_to_string(&path)
                    .is_ok_and(|t| t.contains("\"node") || t.contains("\"npx"))
            {
                return true;
            }
        }
        false
    }
    walk(&plugins_dir.join("cache"), 0)
}

/// Recursive copy; small UTF-8 files get host-home paths rewritten.
pub fn copy_tree(src: &Path, dest: &Path, rw: &PathRewriter) -> Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        // Copy the symlink target's content when it resolves; skip dangling links.
        if let Ok(resolved) = fs::canonicalize(src)
            && (resolved.is_dir() || resolved.is_file())
        {
            return copy_tree(&resolved, dest, rw);
        }
        return Ok(());
    }
    if meta.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dest.join(entry.file_name()), rw)?;
        }
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if meta.len() <= REWRITE_LIMIT {
        let bytes = fs::read(src)?;
        if let Ok(text) = std::str::from_utf8(&bytes) {
            fs::write(dest, rw.rewrite_text(text))?;
            fs::set_permissions(dest, meta.permissions())?;
            return Ok(());
        }
    }
    fs::copy(src, dest)?;
    Ok(())
}

fn install_fish_completions(paths: &HostPaths) -> Result<()> {
    let fish_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map_or_else(|| paths.home.join(".config"), PathBuf::from)
        .join("fish")
        .join("completions");
    if !fish_dir.parent().is_some_and(Path::exists) {
        return Ok(());
    }
    fs::create_dir_all(&fish_dir)?;
    for bin in ["claude_here", "claude_yolo"] {
        let text = crate::completions_text(clap_complete::Shell::Fish, bin);
        fs::write(fish_dir.join(format!("{bin}.fish")), text)?;
    }
    println!(
        "fish:       completions installed in {}",
        fish_dir.display()
    );
    Ok(())
}

fn offer_global_gitignore(paths: &HostPaths, yes: bool) -> Result<()> {
    let ignore = std::env::var_os("XDG_CONFIG_HOME")
        .map_or_else(|| paths.home.join(".config"), PathBuf::from)
        .join("git")
        .join("ignore");
    let existing = fs::read_to_string(&ignore).unwrap_or_default();
    if existing
        .lines()
        .any(|l| l.trim() == ".claude_here/" || l.trim() == ".claude_here")
    {
        return Ok(());
    }
    if !yes {
        print!(
            "add `.claude_here/` to your global git ignore ({})? [y/N] ",
            ignore.display()
        );
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        if !matches!(line.trim(), "y" | "Y" | "yes") {
            return Ok(());
        }
    }
    if let Some(parent) = ignore.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(".claude_here/\n");
    fs::write(&ignore, text)?;
    println!("gitignore:  added .claude_here/ to {}", ignore.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_token_from_noise() {
        let text = "Your token:\n\n  sk-ant-oat01-abcdefghijklmnopqrstuvwxyz0123456789  \n\nStore it safely.";
        assert_eq!(
            extract_token(text).as_deref(),
            Some("sk-ant-oat01-abcdefghijklmnopqrstuvwxyz0123456789")
        );
        assert_eq!(extract_token("nothing here"), None);
    }

    #[test]
    fn token_file_is_env_file_with_0600() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = dir.path().join("token");
        write_token(&p, "sk-ant-x")?;
        assert_eq!(
            fs::read_to_string(&p)?,
            "CLAUDE_CODE_OAUTH_TOKEN=sk-ant-x\n"
        );
        assert_eq!(fs::metadata(&p)?.permissions().mode() & 0o777, 0o600);
        Ok(())
    }

    #[test]
    fn copy_tree_rewrites_text_and_keeps_binary() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("sub"))?;
        fs::write(
            src.join("settings.json"),
            r#"{"hook":"/home/axel/.claude/x.sh"}"#,
        )?;
        fs::write(src.join("sub").join("bin"), [0u8, 159, 146, 150])?;
        let dest = dir.path().join("dest");
        copy_tree(&src, &dest, &PathRewriter::new("/home/axel", "ni"))?;
        assert_eq!(
            fs::read_to_string(dest.join("settings.json"))?,
            r#"{"hook":"/home/ni/.claude/x.sh"}"#
        );
        assert_eq!(
            fs::read(dest.join("sub").join("bin"))?,
            vec![0u8, 159, 146, 150]
        );
        Ok(())
    }
}
