//! Checkpoints on a real repository: the journal records every commit of a step, including the
//! model's own; trailers and the agent identity are set; hooks of the project never run.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::cell::RefCell;
use std::path::PathBuf;

use vf_git::{AGENT_EMAIL, Repo};
use vf_journal::{JournalError, JournalStore, StepReport, StepStart, Vcs};
use vf_protocol::http::JournalEntry;

#[derive(Default)]
struct Memory(RefCell<Vec<JournalEntry>>);
impl JournalStore for Memory {
    fn read(&self, _: &str) -> Result<Vec<JournalEntry>, JournalError> {
        Ok(self.0.borrow().clone())
    }
    fn append(&self, _: &str, entry: &JournalEntry) -> Result<(), JournalError> {
        self.0.borrow_mut().push(entry.clone());
        Ok(())
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "vf-git-{tag}-{}-{nanos}-{counter}",
        std::process::id()
    ))
}

fn report(label: &str) -> StepReport {
    StepReport {
        provider: "claude".into(),
        model: "haiku".into(),
        prompt: label.into(),
        label: label.into(),
        customization: Some("dark-mode".into()),
        summary: "ok".into(),
        ..StepReport::default()
    }
}

#[test]
fn step_records_model_commits_and_checkpoint() {
    let dir = temp_dir("step");
    let repo = Repo::init(&dir).unwrap();
    let store = Memory::default();
    let step = StepStart::begin(&repo).unwrap();

    // The model commits by itself, then leaves more changes uncommitted.
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    repo.commit_all("model: add a", &[]).unwrap().unwrap();
    std::fs::write(dir.join("b.txt"), "b\n").unwrap();

    let out = step
        .finish(
            &repo,
            &store,
            "p",
            "2026-10-20T00:00:00Z".into(),
            report("add files"),
        )
        .unwrap();
    assert_eq!(out.entry.commits.len(), 2, "model commit + checkpoint");
    assert_eq!(
        out.entry.commit.as_deref(),
        out.entry.commits.first().map(String::as_str)
    );
    assert_eq!(out.entry.files, "2 files");
    assert!(
        out.diff.contains("a.txt") && out.diff.contains("b.txt"),
        "diff from the step start"
    );
    let msg = repo.message("HEAD").unwrap();
    assert!(
        msg.starts_with("vibefork: add files [claude/haiku]"),
        "{msg}"
    );
    assert!(msg.contains("Vibefork-Customization: dark-mode"));
    assert_eq!(store.read("p").unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn step_without_changes_has_no_commit_and_rollback_works() {
    let dir = temp_dir("empty");
    let repo = Repo::init(&dir).unwrap();
    let store = Memory::default();
    let base = repo.head().unwrap();
    let out = StepStart::begin(&repo)
        .unwrap()
        .finish(&repo, &store, "p", "t".into(), report("nothing"))
        .unwrap();
    assert!(out.entry.commits.is_empty() && out.entry.commit.is_none() && out.diff.is_empty());

    std::fs::write(dir.join("c.txt"), "c\n").unwrap();
    StepStart::begin(&repo)
        .unwrap()
        .finish(&repo, &store, "p", "t".into(), report("c"))
        .unwrap();
    std::fs::write(dir.join("junk.txt"), "x").unwrap();
    repo.reset_hard(&base).unwrap();
    assert!(!dir.join("c.txt").exists() && !dir.join("junk.txt").exists());
    assert_eq!(repo.head().unwrap(), base);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn project_hooks_never_run_and_agent_identity_is_used() {
    let dir = temp_dir("hooks");
    let repo = Repo::init(&dir).unwrap();
    let marker = dir.join("hook-ran");
    let hook = format!(
        "#!/bin/sh\necho x > '{}'\nexit 1\n",
        marker.display().to_string().replace('\\', "/")
    );
    for name in ["pre-commit", "commit-msg", "post-commit"] {
        std::fs::write(dir.join(".git/hooks").join(name), &hook).unwrap();
    }
    std::fs::write(dir.join("d.txt"), "d\n").unwrap();
    assert!(repo.commit_all("x", &[]).unwrap().is_some());
    assert!(!marker.exists(), "hooks are disabled");
    let out = std::process::Command::new("git")
        .current_dir(&dir)
        .args(["log", "-1", "--format=%ae %ce"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("{AGENT_EMAIL} {AGENT_EMAIL}")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changes_from_nothing_and_blob_ids() {
    let dir = temp_dir("blobs");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    std::fs::write(dir.join("src/b.txt"), "two\n").unwrap();
    let repo = Repo::init(&dir).unwrap();
    let start = repo.head().unwrap();

    let mut all: Vec<String> = repo
        .changes("", &start)
        .unwrap()
        .into_iter()
        .map(|c| c.path)
        .collect();
    all.sort();
    assert_eq!(all, ["a.txt", "src/b.txt"]);

    std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
    repo.commit_all("edit", &[]).unwrap();
    let head = repo.head().unwrap();
    let paths = vec![
        "a.txt".to_owned(),
        "src/b.txt".to_owned(),
        "missing".to_owned(),
    ];
    let (before, after) = (
        repo.blob_ids(&start, &paths).unwrap(),
        repo.blob_ids(&head, &paths).unwrap(),
    );
    assert_ne!(before["a.txt"], after["a.txt"]);
    assert_eq!(before["src/b.txt"], after["src/b.txt"]);
    assert!(!after.contains_key("missing"));
    // Git's own blob id of "two\n".
    assert_eq!(
        after["src/b.txt"],
        "f719efd430d52bcfc8566a43b2eb655688d38871"
    );
    assert!(repo.blob_ids("", &paths).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
