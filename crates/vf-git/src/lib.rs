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

    /// The repository that contains `dir` (the folder itself or a parent), like `git` finds it.
    pub fn discover(dir: &Path) -> Option<Self> {
        let repo = Repository::discover(dir).ok()?;
        Some(Self::open(repo.workdir()?.to_path_buf()))
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

    /// The read-only subset of `git` exposed to the processes of a task. `add` and `commit` are
    /// acknowledged (the agent records changes itself); other changing commands are refused, so a
    /// tool never takes a refused `checkout` or `reset` for a done one.
    pub fn shim(&self, args: &[String]) -> Result<ShimOutput, JournalError> {
        let Some(command) = args.first().map(String::as_str) else {
            return Ok(ShimOutput::Unsupported("missing command".into()));
        };
        if matches!(command, "add" | "commit") {
            return Ok(ShimOutput::Recorded);
        }
        if matches!(
            command,
            "stash"
                | "reset"
                | "checkout"
                | "switch"
                | "restore"
                | "rm"
                | "mv"
                | "merge"
                | "rebase"
                | "cherry-pick"
                | "tag"
                | "clean"
                | "push"
                | "pull"
                | "fetch"
                | "clone"
                | "init"
        ) {
            return Ok(ShimOutput::Unsupported(format!(
                "`git {command}` is not available here: change the files directly; Vibefork records every change and can roll back"
            )));
        }
        let repo = self.repo()?;
        match command {
            "status" => Ok(ShimOutput::Text(self.status_short()?)),
            "diff" => {
                if args.iter().any(|a| a == "--stat") {
                    let commit = repo.head().and_then(|h| h.peel_to_commit()).map_err(err)?;
                    let mut opts = DiffOptions::new();
                    opts.include_untracked(true)
                        .recurse_untracked_dirs(true)
                        .show_untracked_content(true);
                    let diff = repo
                        .diff_tree_to_workdir_with_index(
                            Some(&commit.tree().map_err(err)?),
                            Some(&mut opts),
                        )
                        .map_err(err)?;
                    let stats = diff.stats().map_err(err)?;
                    Ok(ShimOutput::Text(
                        stats
                            .to_buf(DiffStatsFormat::FULL, 80)
                            .map_err(err)?
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    ))
                } else {
                    let commit = repo.head().and_then(|h| h.peel_to_commit()).map_err(err)?;
                    let mut opts = DiffOptions::new();
                    opts.include_untracked(true)
                        .recurse_untracked_dirs(true)
                        .show_untracked_content(true);
                    let diff = repo
                        .diff_tree_to_workdir_with_index(
                            Some(&commit.tree().map_err(err)?),
                            Some(&mut opts),
                        )
                        .map_err(err)?;
                    let mut text = String::new();
                    diff.print(DiffFormat::Patch, |_, _, line| {
                        if matches!(line.origin(), '+' | '-' | ' ') {
                            text.push(line.origin());
                        }
                        text.push_str(&String::from_utf8_lossy(line.content()));
                        true
                    })
                    .map_err(err)?;
                    Ok(ShimOutput::Text(text))
                }
            }
            "rev-parse" if args.iter().any(|a| a == "--show-toplevel") => Ok(ShimOutput::Text(
                format!("{}\n", repo.workdir().unwrap_or(self.dir()).display()),
            )),
            "rev-parse" if args.iter().any(|a| a == "HEAD") => Ok(ShimOutput::Text(format!(
                "{}\n",
                repo.head()
                    .and_then(|h| h.peel_to_commit())
                    .map_err(err)?
                    .id()
            ))),
            "branch" if args.iter().any(|a| a == "--show-current") => {
                let name = repo
                    .head()
                    .map_err(err)?
                    .shorthand()
                    .unwrap_or_default()
                    .to_owned();
                Ok(ShimOutput::Text(format!("{name}\n")))
            }
            "log" => {
                let limit = args
                    .windows(2)
                    .find(|w| w[0] == "-n")
                    .and_then(|w| w[1].parse::<usize>().ok())
                    .unwrap_or(10);
                let mut walk = repo.revwalk().map_err(err)?;
                walk.push_head().map_err(err)?;
                let mut text = String::new();
                for oid in walk.take(limit) {
                    let c = repo.find_commit(oid.map_err(err)?).map_err(err)?;
                    if args.iter().any(|a| a == "--oneline") {
                        text.push_str(&format!(
                            "{} {}\n",
                            short(c.id()),
                            c.summary().ok().flatten().unwrap_or_default()
                        ));
                    } else {
                        text.push_str(&format!(
                            "commit {}\n\n    {}\n\n",
                            c.id(),
                            c.message().unwrap_or_default().replace('\n', "\n    ")
                        ));
                    }
                }
                Ok(ShimOutput::Text(text))
            }
            "show" => {
                let c = repo
                    .revparse_single(args.get(1).map(String::as_str).unwrap_or("HEAD"))
                    .and_then(|o| o.peel_to_commit())
                    .map_err(err)?;
                let mut text = format!(
                    "commit {}\nAuthor: {}\n\n    {}\n\n",
                    c.id(),
                    c.author(),
                    c.message().unwrap_or_default().trim()
                );
                let parent = c.parent(0).ok();
                let before = parent.as_ref().and_then(|p| p.tree().ok());
                let after = c.tree().map_err(err)?;
                let diff = repo
                    .diff_tree_to_tree(before.as_ref(), Some(&after), None)
                    .map_err(err)?;
                diff.print(DiffFormat::Patch, |_, _, line| {
                    if matches!(line.origin(), '+' | '-' | ' ') {
                        text.push(line.origin());
                    }
                    text.push_str(&String::from_utf8_lossy(line.content()));
                    true
                })
                .map_err(err)?;
                Ok(ShimOutput::Text(text))
            }
            other => Ok(ShimOutput::Unsupported(format!(
                "unsupported command `{other}`"
            ))),
        }
    }
}

/// Result from the safe Git command subset.
pub enum ShimOutput {
    /// Command output.
    Text(String),
    /// Vibefork records changes automatically.
    Recorded,
    /// Command is not supported.
    Unsupported(String),
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
    /// Files added, changed or deleted from `base` to `head`, with their new content; `base` empty —
    /// every file of `head`.
    pub fn changes(&self, base: &str, head: &str) -> Result<Vec<FileChange>, JournalError> {
        let repo = self.repo()?;
        let a = if base.is_empty() {
            None
        } else {
            Some(tree_of(&repo, base)?)
        };
        let b = tree_of(&repo, head)?;
        let diff = repo
            .diff_tree_to_tree(a.as_ref(), Some(&b), None)
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

    /// Git blob ids of `paths` at `commit` (a path that is not a file there is absent; `commit`
    /// empty — none).
    pub fn blob_ids(
        &self,
        commit: &str,
        paths: &[String],
    ) -> Result<std::collections::HashMap<String, String>, JournalError> {
        let mut out = std::collections::HashMap::new();
        if commit.is_empty() {
            return Ok(out);
        }
        let repo = self.repo()?;
        let tree = tree_of(&repo, commit)?;
        for p in paths {
            if let Ok(e) = tree.get_path(Path::new(p))
                && e.kind() == Some(git2::ObjectType::Blob)
            {
                out.insert(p.clone(), e.id().to_string());
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

#[cfg(test)]
mod shim_tests {
    use super::*;

    #[test]
    #[allow(clippy::expect_used)]
    fn shim_reads_status_diff_and_log_without_changing_the_repository() {
        let dir = std::env::temp_dir().join(format!("vf-git-shim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(std::fs::create_dir_all(&dir).is_ok());
        assert!(std::fs::write(dir.join("file.txt"), "before\n").is_ok());
        let repo = Repo::init_with(&dir, "initial").expect("repository initializes");
        assert!(std::fs::write(dir.join("file.txt"), "after\n").is_ok());

        let status = repo
            .shim(&["status".into(), "--short".into()])
            .expect("status");
        assert!(matches!(status, ShimOutput::Text(ref text) if text.contains("file.txt")));
        let diff = repo.shim(&["diff".into(), "HEAD".into()]).expect("diff");
        assert!(matches!(diff, ShimOutput::Text(ref text) if text.contains("+after")));
        let log = repo
            .shim(&["log".into(), "-n".into(), "1".into(), "--oneline".into()])
            .expect("log");
        assert!(matches!(log, ShimOutput::Text(ref text) if text.contains("initial")));
        assert!(matches!(
            repo.shim(&["commit".into()])
                .expect("commit acknowledgement"),
            ShimOutput::Recorded
        ));
        assert!(matches!(
            repo.shim(&["checkout".into(), "file.txt".into()])
                .expect("checkout refusal"),
            ShimOutput::Unsupported(_)
        ));
        assert_eq!(
            std::fs::read_to_string(dir.join("file.txt"))
                .ok()
                .as_deref(),
            Some("after\n"),
            "a refused command changes nothing"
        );
        std::fs::create_dir_all(dir.join("sub")).expect("subfolder");
        let found = Repo::discover(&dir.join("sub")).expect("found from a subfolder");
        assert!(
            matches!(found.shim(&["status".into()]), Ok(ShimOutput::Text(ref t)) if t.contains("file.txt"))
        );
        assert!(
            repo.shim(&["not-a-command".into()])
                .is_ok_and(|o| matches!(o, ShimOutput::Unsupported(_)))
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
