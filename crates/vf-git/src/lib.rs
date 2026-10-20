//! Every git operation in one place, through the `git` command line.
//!
//! Project folders hold content written by models and taken from forks, so it is data (invariant 9):
//! every call disables hooks, `core.fsmonitor`, commit signing and prompts; diffs skip external
//! drivers and text conversion; `GIT_*` variables of the agent's own environment are dropped. Commits are made as the agent
//! ([`AGENT_NAME`] / [`AGENT_EMAIL`]) without touching the user's git config.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use vf_journal::{JournalError, Vcs};

/// Author and committer name of checkpoints.
pub const AGENT_NAME: &str = "vibefork-agent";
/// Author and committer e-mail of checkpoints.
pub const AGENT_EMAIL: &str = "agent@vibefork.local";

#[cfg(windows)]
const NO_HOOKS: &str = "NUL";
#[cfg(not(windows))]
const NO_HOOKS: &str = "/dev/null";

/// A git repository (a project folder).
#[derive(Debug, Clone)]
pub struct Repo {
    dir: PathBuf,
}

impl Repo {
    /// Opens an existing repository.
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether the folder is a repository with at least one commit.
    pub fn is_ready(&self) -> bool {
        self.git(["rev-parse", "--verify", "-q", "HEAD"]).is_ok()
    }

    /// Makes a folder a repository whose first commit holds the files already there (so every step
    /// has a base and the user's own files are never counted as a model's changes).
    pub fn init(dir: impl Into<PathBuf>) -> Result<Self, JournalError> {
        let repo = Self { dir: dir.into() };
        std::fs::create_dir_all(&repo.dir).map_err(io)?;
        if !repo.dir.join(".git").exists() {
            repo.git(["init", "-q", "-b", "main"])?;
        }
        repo.git(["add", "-A", "."])?;
        repo.git([
            "commit",
            "-q",
            "--allow-empty",
            "--no-verify",
            "-m",
            "vibefork: start",
        ])?;
        Ok(repo)
    }

    /// `git diff --stat` between two commits.
    pub fn diff_stat(&self, base: &str, head: &str) -> Result<String, JournalError> {
        self.git(["diff", "--no-ext-diff", "--stat", base, head])
    }

    /// Clones `url` into `dir`, the source remote is named `upstream`.
    pub fn clone_from(url: &str, dir: impl Into<PathBuf>) -> Result<Self, JournalError> {
        let dir = dir.into();
        let parent = dir.parent().map(Path::to_path_buf).unwrap_or_default();
        std::fs::create_dir_all(&parent).map_err(io)?;
        let tmp = Self { dir: parent };
        tmp.git([
            OsStr::new("clone"),
            OsStr::new("-q"),
            OsStr::new("--origin"),
            OsStr::new("upstream"),
            OsStr::new("--"),
            OsStr::new(url),
            dir.as_os_str(),
        ])?;
        Ok(Self { dir })
    }

    /// Resets the working tree and index to `commit`, removing untracked files (rollback of steps).
    pub fn reset_hard(&self, commit: &str) -> Result<(), JournalError> {
        self.git(["reset", "-q", "--hard", commit])?;
        self.git(["clean", "-q", "-fd"])?;
        Ok(())
    }

    /// Short summary of changed files of the working tree (`git status --short`).
    pub fn status_short(&self) -> Result<String, JournalError> {
        self.git(["status", "--short"])
    }

    /// Full message of a commit.
    pub fn message(&self, commit: &str) -> Result<String, JournalError> {
        self.git(["log", "-1", "--format=%B", commit])
    }

    /// Runs git with the safety options; returns stdout.
    fn git<I, S>(&self, args: I) -> Result<String, JournalError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let out = self.run(args)?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(JournalError(format!(
                "git: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    }

    fn run<I, S>(&self, args: I) -> Result<Output, JournalError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.dir)
            .args(["-c", &format!("core.hooksPath={NO_HOOKS}")])
            .args(["-c", "core.fsmonitor=false", "-c", "commit.gpgsign=false"])
            .args([
                "-c",
                &format!("user.name={AGENT_NAME}"),
                "-c",
                &format!("user.email={AGENT_EMAIL}"),
            ])
            .args(args)
            .stdin(Stdio::null())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_")
                && key != "GIT_TERMINAL_PROMPT"
                && key != "GIT_CONFIG_NOSYSTEM"
            {
                cmd.env_remove(key);
            }
        }
        no_window(&mut cmd);
        cmd.output()
            .map_err(|e| JournalError(format!("git is not available: {e}")))
    }
}

impl Vcs for Repo {
    fn head(&self) -> Result<String, JournalError> {
        Ok(self.git(["rev-parse", "HEAD"])?.trim().to_owned())
    }

    fn diff_from(&self, base: &str) -> Result<String, JournalError> {
        self.git(["add", "-A", "-N", "."])?;
        self.git(["diff", "--no-ext-diff", "--no-textconv", "--no-color", base])
    }

    fn commit_all(
        &self,
        message: &str,
        trailers: &[(&str, &str)],
    ) -> Result<Option<String>, JournalError> {
        self.git(["add", "-A", "."])?;
        let staged = self.run(["diff", "--cached", "--quiet"])?;
        if staged.status.success() {
            return Ok(None);
        }
        let mut full = message.to_owned();
        if !trailers.is_empty() {
            full.push_str("\n\n");
            for (k, v) in trailers {
                full.push_str(&format!("{k}: {v}\n"));
            }
        }
        self.git(["commit", "-q", "--no-verify", "-m", &full])?;
        Ok(Some(
            self.git(["rev-parse", "--short", "HEAD"])?
                .trim()
                .to_owned(),
        ))
    }

    fn commits_between(&self, base: &str, head: &str) -> Result<Vec<String>, JournalError> {
        let out = self.git(["log", "--format=%h", &format!("{base}..{head}")])?;
        Ok(out.lines().map(str::to_owned).collect())
    }

    fn files_changed(&self, base: &str, head: &str) -> Result<usize, JournalError> {
        Ok(self
            .git(["diff", "--no-ext-diff", "--name-only", base, head])?
            .lines()
            .count())
    }
}

fn io(e: std::io::Error) -> JournalError {
    JournalError(e.to_string())
}

#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_window(_: &mut Command) {}
