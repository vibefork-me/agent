//! What a save sends to the copy: coding CLI configuration only as it was at the project's start.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use vf_git::Repo;
use vf_journal::Vcs;

#[test]
fn cli_configuration_goes_only_in_its_original_version() {
    let dir = std::env::temp_dir().join(format!("vf-git-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".claude")).unwrap();
    std::fs::write(dir.join("app.txt"), "v1").unwrap();
    std::fs::write(dir.join(".claude/settings.json"), "{}").unwrap();
    std::fs::write(dir.join(".mcp.json"), "{}").unwrap();
    // An imported project: its start is the import, not the first commit.
    Repo::init_with(&dir, "first").unwrap();
    std::fs::write(dir.join("CLAUDE.md"), "notes").unwrap();
    let repo = Repo::open(&dir);
    repo.commit_all("import", &[]).unwrap();
    let start = repo.head().unwrap();

    std::fs::write(dir.join("app.txt"), "v2").unwrap();
    std::fs::write(dir.join(".claude/settings.json"), r#"{"hooks":{}}"#).unwrap();
    std::fs::remove_file(dir.join(".mcp.json")).unwrap();
    std::fs::create_dir_all(dir.join(".Codex")).unwrap();
    std::fs::write(dir.join(".Codex/config.toml"), "x").unwrap();
    repo.commit_all("model", &[]).unwrap();
    let head = repo.head().unwrap();

    // On top of the start: the work goes, every change of CLI configuration is held back.
    let out = repo.changes_to_save(&start, &start, &head).unwrap();
    let sent: Vec<&str> = out.changes.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(sent, ["app.txt"]);
    let mut held = out.held_back.clone();
    held.sort();
    assert_eq!(
        held,
        [".Codex/config.toml", ".claude/settings.json", ".mcp.json"]
    );

    // Into an empty repository: every file, CLI configuration as it was at the start.
    let out = repo.changes_to_save(&start, "", &head).unwrap();
    let file = |p: &str| {
        out.changes
            .iter()
            .find(|c| c.path == p)
            .and_then(|c| c.content.clone())
    };
    assert_eq!(file("app.txt").as_deref(), Some(&b"v2"[..]));
    assert_eq!(file("CLAUDE.md").as_deref(), Some(&b"notes"[..]));
    assert_eq!(file(".claude/settings.json").as_deref(), Some(&b"{}"[..]));
    assert_eq!(file(".mcp.json").as_deref(), Some(&b"{}"[..]));
    assert_eq!(file(".Codex/config.toml"), None);

    // CLI configuration back to the original is not held back.
    std::fs::write(dir.join(".claude/settings.json"), "{}").unwrap();
    repo.commit_all("undo", &[]).unwrap();
    let undo = repo.head().unwrap();
    let out = repo.changes_to_save(&start, &head, &undo).unwrap();
    assert_eq!(out.changes.len(), 1);
    assert!(out.held_back.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
