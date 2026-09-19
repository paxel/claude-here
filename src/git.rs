//! Host-side discovery of git directories that must be bind-mounted
//! read-only in `GitMode::Ro`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

/// Directory names skipped when scanning for nested repositories.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "build",
    ".gradle",
    ".cache",
    ".venv",
];
/// Maximum scan depth below the working directory.
const MAX_DEPTH: usize = 6;

/// Result of scanning a working directory for git state.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GitLayout {
    /// Directories that hold git object/ref data and must be read-only.
    pub protected_dirs: BTreeSet<PathBuf>,
    /// Human readable remarks printed at startup.
    pub notes: Vec<String>,
}

/// Inspect `cwd` (and nested repositories below it) for git directories.
pub fn scan(cwd: &Path) -> Result<GitLayout> {
    let mut layout = GitLayout::default();
    if let Some(top) = toplevel(cwd)
        && top != cwd
        && cwd.starts_with(&top)
    {
        layout.notes.push(format!(
            "cwd is inside repository {} but only cwd is mounted; git commands inside will not see a repository",
            top.display()
        ));
    }
    let mut found = Vec::new();
    walk(cwd, 0, &mut found);
    for dot_git in found {
        resolve_git_entry(&dot_git, &mut layout);
    }
    Ok(layout)
}

fn toplevel(cwd: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" {
            found.push(path);
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() && !ft.is_symlink() {
            let n = name.to_string_lossy();
            if n.starts_with('.') || SKIP_DIRS.contains(&n.as_ref()) {
                continue;
            }
            subdirs.push(path);
        }
    }
    if depth < MAX_DEPTH {
        for sub in subdirs {
            walk(&sub, depth + 1, found);
        }
    }
}

/// `.git` is either a directory (plain repo) or a file with `gitdir: <path>`
/// (worktree or submodule).
fn resolve_git_entry(dot_git: &Path, layout: &mut GitLayout) {
    if dot_git.is_dir() {
        layout.protected_dirs.insert(dot_git.to_path_buf());
        return;
    }
    let Some(gitdir) = read_gitdir_pointer(dot_git) else {
        return;
    };
    layout.protected_dirs.insert(gitdir.clone());
    if let Some(common) = common_dir(&gitdir) {
        layout.protected_dirs.insert(common);
    }
    layout.notes.push(format!(
        "{} is a worktree/submodule pointer; protecting {}",
        dot_git.display(),
        gitdir.display()
    ));
}

/// Parse `gitdir: <path>` from a `.git` file; relative paths are resolved
/// against the file's parent directory.
pub fn read_gitdir_pointer(dot_git_file: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(dot_git_file).ok()?;
    let raw = text.trim().strip_prefix("gitdir:")?.trim();
    let path = PathBuf::from(raw);
    let abs = if path.is_absolute() {
        path
    } else {
        dot_git_file.parent()?.join(path)
    };
    fs::canonicalize(&abs).ok().or(Some(abs))
}

fn common_dir(gitdir: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .arg("--git-dir")
        .arg(gitdir)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return None;
    }
    let p = PathBuf::from(&s);
    let abs = if p.is_absolute() { p } else { gitdir.join(p) };
    fs::canonicalize(&abs).ok().or(Some(abs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_repo_dir_is_protected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::create_dir(dir.path().join(".git"))?;
        fs::create_dir_all(dir.path().join("sub").join("nested").join(".git"))?;
        fs::create_dir_all(dir.path().join("node_modules").join("x").join(".git"))?;
        let layout = scan(dir.path())?;
        assert!(layout.protected_dirs.contains(&dir.path().join(".git")));
        assert!(
            layout
                .protected_dirs
                .contains(&dir.path().join("sub").join("nested").join(".git"))
        );
        assert_eq!(
            layout.protected_dirs.len(),
            2,
            "{:?}",
            layout.protected_dirs
        );
        Ok(())
    }

    #[test]
    fn gitdir_pointer_resolves_relative() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let real = dir.path().join("real.git");
        fs::create_dir(&real)?;
        let sub = dir.path().join("wt");
        fs::create_dir(&sub)?;
        fs::write(sub.join(".git"), "gitdir: ../real.git\n")?;
        let got = read_gitdir_pointer(&sub.join(".git"));
        assert_eq!(got, fs::canonicalize(&real).ok());
        Ok(())
    }

    #[test]
    fn non_repo_has_nothing() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let layout = scan(dir.path())?;
        assert!(layout.protected_dirs.is_empty());
        Ok(())
    }
}
