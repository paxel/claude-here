//! Building a session's summary from its capture when it is first asked for.
//! A session only leaves its capture behind, so ending it stays fast.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::SessionFiles;

/// The summary script inside the base image.
pub const SUMMARY_SCRIPT: &str = "/usr/local/lib/claude_here/net-summary.sh";

/// Runs one `docker` command and returns its exit code.
pub trait DockerRun {
    fn run(&self, args: &[String]) -> Result<i32>;
}

impl DockerRun for crate::docker::Docker {
    fn run(&self, args: &[String]) -> Result<i32> {
        self.run_inherit(args)
    }
}

/// Where and as whom the summary is built.
pub struct Builder<'a, R: DockerRun> {
    pub docker: &'a R,
    /// The base image, which has tshark and the summary script.
    pub image: String,
    /// `uid:gid` of the host user, so the summary is theirs.
    pub user: String,
}

/// `docker` arguments that summarize `pcap` into `out/<session_id>.summary.json`:
/// no network, the host user, the capture read-only.
pub fn summary_args(
    image: &str,
    user: &str,
    pcap: &Path,
    out: &Path,
    session_id: &str,
) -> Vec<String> {
    [
        "run",
        "--rm",
        "--network",
        "none",
        "--user",
        user,
        "-e",
        "HOME=/tmp",
        "-v",
        &format!("{}:/capture.pcap:ro", pcap.display()),
        "-v",
        &format!("{}:/out", out.display()),
        "--entrypoint",
        SUMMARY_SCRIPT,
        image,
        "/capture.pcap",
        "/out",
        session_id,
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

impl<R: DockerRun> Builder<'_, R> {
    /// Give `files` a summary when it has a capture but none yet. Returns
    /// whether one was built. The container writes into a directory of its
    /// own, never into the log directory with the other sessions' captures.
    pub fn ensure_summary(&self, files: &SessionFiles) -> Result<bool> {
        if files.summary.exists() || !files.pcap.exists() {
            return Ok(false);
        }
        let dir = files
            .summary
            .parent()
            .context("summary path has no directory")?;
        let work = dir.join(format!(".summary-{}", files.session_id));
        fs::create_dir_all(&work).with_context(|| format!("creating {}", work.display()))?;
        eprintln!(
            "claude_here: building the network summary of {} from its capture",
            files.session_id
        );
        let result = self.build_into(files, &work);
        let _ = fs::remove_dir_all(&work);
        result.map(|()| true)
    }

    fn build_into(&self, files: &SessionFiles, work: &Path) -> Result<()> {
        let args = summary_args(
            &self.image,
            &self.user,
            &files.pcap,
            work,
            &files.session_id,
        );
        let code = self.docker.run(&args)?;
        if code != 0 {
            bail!(
                "building the summary of {} exited with {code}",
                files.session_id
            );
        }
        let built = work.join(format!("{}.summary.json", files.session_id));
        if !built.exists() {
            bail!("building the summary of {} wrote nothing", files.session_id);
        }
        fs::rename(&built, &files.summary)
            .with_context(|| format!("moving the summary to {}", files.summary.display()))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use super::*;

    /// Records the arguments and writes what the script would write.
    struct FakeDocker {
        code: i32,
        write: Option<&'static str>,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl FakeDocker {
        fn new(code: i32, write: Option<&'static str>) -> Self {
            Self {
                code,
                write,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl DockerRun for FakeDocker {
        fn run(&self, args: &[String]) -> Result<i32> {
            self.calls.borrow_mut().push(args.to_vec());
            if let Some(text) = self.write {
                let out = args
                    .iter()
                    .find_map(|a| a.strip_suffix(":/out"))
                    .map(PathBuf::from)
                    .context("no /out mount")?;
                let sid = args.last().context("no session id")?;
                fs::write(out.join(format!("{sid}.summary.json")), text)?;
            }
            Ok(self.code)
        }
    }

    fn builder(docker: &FakeDocker) -> Builder<'_, FakeDocker> {
        Builder {
            docker,
            image: "claude_here:base-u1000".into(),
            user: "1000:1000".into(),
        }
    }

    #[test]
    fn args_isolate_the_build() {
        let args = summary_args(
            "img",
            "1000:1000",
            Path::new("/l/s.pcap"),
            Path::new("/l/.summary-s"),
            "s",
        );
        let joined = args.join(" ");
        assert!(joined.starts_with("run --rm --network none --user 1000:1000 "));
        assert!(joined.contains("-v /l/s.pcap:/capture.pcap:ro"));
        assert!(joined.contains("-v /l/.summary-s:/out"));
        assert!(joined.ends_with(&format!(
            "--entrypoint {SUMMARY_SCRIPT} img /capture.pcap /out s"
        )));
    }

    #[test]
    fn builds_a_missing_summary_from_the_capture() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let files = SessionFiles::new(dir.path(), "s1");
        fs::write(&files.pcap, "pcap")?;
        let docker = FakeDocker::new(0, Some("{\"session_id\":\"s1\"}"));
        assert!(builder(&docker).ensure_summary(&files)?);
        assert_eq!(
            fs::read_to_string(&files.summary)?,
            "{\"session_id\":\"s1\"}"
        );
        assert!(!dir.path().join(".summary-s1").exists(), "work dir removed");
        assert_eq!(docker.calls.borrow().len(), 1);
        // A second call finds the summary and runs nothing.
        assert!(!builder(&docker).ensure_summary(&files)?);
        assert_eq!(docker.calls.borrow().len(), 1);
        Ok(())
    }

    #[test]
    fn nothing_to_build_without_a_capture() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let files = SessionFiles::new(dir.path(), "s1");
        let docker = FakeDocker::new(0, None);
        assert!(!builder(&docker).ensure_summary(&files)?);
        assert!(docker.calls.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn a_failed_build_is_an_error_and_leaves_no_summary() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let files = SessionFiles::new(dir.path(), "s1");
        fs::write(&files.pcap, "pcap")?;

        let failing = FakeDocker::new(125, None);
        let err = builder(&failing)
            .ensure_summary(&files)
            .err()
            .map(|e| e.to_string());
        assert_eq!(
            err.as_deref(),
            Some("building the summary of s1 exited with 125")
        );

        let silent = FakeDocker::new(0, None);
        let err = builder(&silent)
            .ensure_summary(&files)
            .err()
            .map(|e| e.to_string());
        assert_eq!(
            err.as_deref(),
            Some("building the summary of s1 wrote nothing")
        );

        assert!(!files.summary.exists());
        assert!(!dir.path().join(".summary-s1").exists());
        Ok(())
    }
}
