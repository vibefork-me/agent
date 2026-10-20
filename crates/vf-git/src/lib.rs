//! Every git operation in one place, built into the agent (libgit2): nothing needs to be
//! installed on the machine.
//!
//! Project folders hold content written by models and taken from forks, so it is data (invariant 9):
//! the library runs no hooks, no `fsmonitor`, no external diff or filter programs, and has no
//! network access here. Commits are made as the agent ([`AGENT_NAME`] / [`AGENT_EMAIL`]) without
//! touching the user's git config.

use std::path::{Path, PathBuf};

use git2::{
    DiffFormat, DiffOptions, DiffStatsFormat, IndexAddOption, Oid, Repository,
    RepositoryInitOptions, ResetType, Signature, Sort, Status, StatusOptions, Tree,
};
use vf_journal::{JournalError, Vcs};

/// Author and committer name of checkpoints.
pub const AGENT_NAME: &str = "vibefork-agent";
/// Author and committer e-mail of checkpoints.
pub const AGENT_EMAIL: &str = "agent@vibefork.local";
/// Length of short commit ids.
const SHORT: usize = 7;

fn err(e: git2::Error) -> JournalError {
    JournalError(format!("git: {}", e.message()))
}

/// A git repository (a project folder).
#[derive(Debug, Clone)]
pub struct Repo {
    dir: PathBuf,
}

impl Repo {
    /// A repository in `dir` (opened on each call).
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn repo(&self) -> Result<Repository, JournalError> {
        Repository::open(&self.dir).map_err(err)
    }

    /// Whether the folder is a repository with at least one commit.
    pub fn is_ready(&self) -> bool {
        self.repo()
            .is_ok_and(|r| r.head().and_then(|h| h.peel_to_commit()).is_ok())
    }

    /// Makes a folder a repository whose first commit holds the files already there (so every step
    /// has a base and the user's own files are never counted as a model's changes).
    pub fn init(dir: impl Into<PathBuf>) -> Result<Self, JournalError> {
        let me = Self { dir: dir.into() };
        std::fs::create_dir_all(&me.dir).map_err(|e| JournalError(e.to_string()))?;
        if Repository::open(&me.dir).is_err() {
            Repository::init_opts(&me.dir, RepositoryInitOptions::new().initial_head("main"))
                .map_err(err)?;
        }
        let repo = me.repo()?;
        let tree = stage_all(&repo)?;
        let parent = repo.head().and_then(|h| h.peel_to_commit()).ok();
        let sig = signature()?;
        let tree = repo.find_tree(tree).map_err(err)?;
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, "vibefork: start", &tree, &parents)
            .map_err(err)?;
        Ok(me)
    }

    /// Resets the working tree and index to `commit`, removing untracked files (rollback of steps).
    pub fn reset_hard(&self, commit: &str) -> Result<(), JournalError> {
        let repo = self.repo()?;
        let obj = repo.revparse_single(commit).map_err(err)?;
        repo.reset(&obj, ResetType::Hard, None).map_err(err)?;
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        let statuses = repo.statuses(Some(&mut opts)).map_err(err)?;
        for entry in statuses
            .iter()
            .filter(|e| e.status().contains(Status::WT_NEW))
        {
            let Ok(rel) = entry.path() else { continue };
            let path = self.dir.join(rel);
            let _ = std::fs::remove_file(&path);
            // Remove folders that became empty, up to the project root.
            let mut parent = path.parent();
            while let Some(p) = parent.filter(|p| *p != self.dir) {
                if std::fs::remove_dir(p).is_err() {
                    break;
                }
                parent = p.parent();
            }
        }
        Ok(())
    }

    /// Short summary of changed files of the working tree (`git status --short` style).
    pub fn status_short(&self) -> Result<String, JournalError> {
        let repo = self.repo()?;
        let mut opts = StatusOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true);
        let statuses = repo.statuses(Some(&mut opts)).map_err(err)?;
        let mut out = String::new();
        for e in statuses.iter() {
            let s = e.status();
            let code = if s.contains(Status::WT_NEW) {
                "??"
            } else if s.intersects(Status::WT_DELETED | Status::INDEX_DELETED) {
                " D"
            } else {
                " M"
            };
            out.push_str(&format!("{code} {}\n", e.path().unwrap_or_default()));
        }
        Ok(out)
    }

    /// Full message of a commit.
    pub fn message(&self, commit: &str) -> Result<String, JournalError> {
        let repo = self.repo()?;
        let c = repo
            .revparse_single(commit)
            .and_then(|o| o.peel_to_commit())
            .map_err(err)?;
        Ok(c.message().unwrap_or_default().to_owned())
    }

    /// `git diff --stat` between two commits.
    pub fn diff_stat(&self, base: &str, head: &str) -> Result<String, JournalError> {
        let repo = self.repo()?;
        let (a, b) = (tree_of(&repo, base)?, tree_of(&repo, head)?);
        let diff = repo
            .diff_tree_to_tree(Some(&a), Some(&b), None)
            .map_err(err)?;
        let buf = diff
            .stats()
            .and_then(|s| s.to_buf(DiffStatsFormat::FULL, 80))
            .map_err(err)?;
        Ok(buf.as_str().unwrap_or_default().to_owned())
    }
}

/// One file changed between two commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Path, `/`-separated.
    pub path: String,
    /// Git mode (`100644`, `100755`, `120000`); empty for a deletion.
    pub mode: String,
    /// New content; `None` for a deletion.
    pub content: Option<Vec<u8>>,
}

impl Repo {
    /// Files added, changed or deleted from `base` to `head`, with their new content.
    pub fn changes(&self, base: &str, head: &str) -> Result<Vec<FileChange>, JournalError> {
        let repo = self.repo()?;
        let (a, b) = (tree_of(&repo, base)?, tree_of(&repo, head)?);
        let diff = repo
            .diff_tree_to_tree(Some(&a), Some(&b), None)
            .map_err(err)?;
        let mut out = Vec::new();
        for d in diff.deltas() {
            let new = d.new_file();
            let path = new
                .path()
                .or_else(|| d.old_file().path())
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if d.status() == git2::Delta::Deleted {
                out.push(FileChange {
                    path,
                    mode: String::new(),
                    content: None,
                });
            } else {
                let blob = repo.find_blob(new.id()).map_err(err)?;
                out.push(FileChange {
                    path,
                    mode: format!("{:o}", u32::from(new.mode())),
                    content: Some(blob.content().to_vec()),
                });
            }
        }
        Ok(out)
    }

    /// Makes `dir` (an unpacked download) a repository whose first commit holds its files.
    pub fn init_with(dir: impl Into<PathBuf>, message: &str) -> Result<Self, JournalError> {
        let me = Self { dir: dir.into() };
        Repository::init_opts(&me.dir, RepositoryInitOptions::new().initial_head("main"))
            .map_err(err)?;
        let repo = me.repo()?;
        let tree = repo.find_tree(stage_all(&repo)?).map_err(err)?;
        let sig = signature()?;
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
            .map_err(err)?;
        Ok(me)
    }
}

fn signature() -> Result<Signature<'static>, JournalError> {
    Signature::now(AGENT_NAME, AGENT_EMAIL).map_err(err)
}

fn tree_of<'r>(repo: &'r Repository, rev: &str) -> Result<Tree<'r>, JournalError> {
    repo.revparse_single(rev)
        .and_then(|o| o.peel_to_tree())
        .map_err(err)
}

/// Stages everything (new, changed, deleted; `.gitignore` respected) and writes the tree.
fn stage_all(repo: &Repository) -> Result<Oid, JournalError> {
    let mut index = repo.index().map_err(err)?;
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .map_err(err)?;
    index.update_all(["*"], None).map_err(err)?;
    index.write().map_err(err)?;
    index.write_tree().map_err(err)
}

fn short(oid: Oid) -> String {
    oid.to_string()[..SHORT].to_owned()
}

impl Vcs for Repo {
    fn head(&self) -> Result<String, JournalError> {
        let repo = self.repo()?;
        let head = repo.head().and_then(|h| h.peel_to_commit()).map_err(err)?;
        Ok(head.id().to_string())
    }

    fn diff_from(&self, base: &str) -> Result<String, JournalError> {
        let repo = self.repo()?;
        let tree = tree_of(&repo, base)?;
        let mut opts = DiffOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .show_untracked_content(true);
        let diff = repo
            .diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))
            .map_err(err)?;
        let mut out = String::new();
        diff.print(DiffFormat::Patch, |_, _, line| {
            if matches!(line.origin(), '+' | '-' | ' ') {
                out.push(line.origin());
            }
            out.push_str(&String::from_utf8_lossy(line.content()));
            true
        })
        .map_err(err)?;
        Ok(out)
    }

    fn commit_all(
        &self,
        message: &str,
        trailers: &[(&str, &str)],
    ) -> Result<Option<String>, JournalError> {
        let repo = self.repo()?;
        let tree_id = stage_all(&repo)?;
        let parent = repo.head().and_then(|h| h.peel_to_commit()).ok();
        if parent.as_ref().is_some_and(|p| p.tree_id() == tree_id) {
            return Ok(None);
        }
        let mut full = message.to_owned();
        if !trailers.is_empty() {
            full.push_str("\n\n");
            for (k, v) in trailers {
                full.push_str(&format!("{k}: {v}\n"));
            }
        }
        let sig = signature()?;
        let tree = repo.find_tree(tree_id).map_err(err)?;
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        let id = repo
            .commit(Some("HEAD"), &sig, &sig, &full, &tree, &parents)
            .map_err(err)?;
        Ok(Some(short(id)))
    }

    fn commits_between(&self, base: &str, head: &str) -> Result<Vec<String>, JournalError> {
        let repo = self.repo()?;
        let (base, head) = (
            repo.revparse_single(base).map_err(err)?.id(),
            repo.revparse_single(head).map_err(err)?.id(),
        );
        let mut walk = repo.revwalk().map_err(err)?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
            .map_err(err)?;
        walk.push(head).map_err(err)?;
        walk.hide(base).map_err(err)?;
        walk.map(|oid| oid.map(short).map_err(err)).collect()
    }

    fn files_changed(&self, base: &str, head: &str) -> Result<usize, JournalError> {
        let repo = self.repo()?;
        let (a, b) = (tree_of(&repo, base)?, tree_of(&repo, head)?);
        let diff = repo
            .diff_tree_to_tree(Some(&a), Some(&b), None)
            .map_err(err)?;
        Ok(diff.stats().map_err(err)?.files_changed())
    }
}
