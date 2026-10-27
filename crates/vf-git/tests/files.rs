//! The file list of a working tree: what git sees, bounded in number and depth.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use vf_git::Repo;

#[test]
fn working_files_are_bounded_by_number_and_depth() {
    let dir = std::env::temp_dir().join(format!("vf-git-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a/b/c")).unwrap();
    for f in ["one.txt", "two.txt", "a/three.txt", "a/b/c/deep.txt"] {
        std::fs::write(dir.join(f), "x").unwrap();
    }
    let repo = Repo::init_with(&dir, "start").unwrap();
    let (all, truncated) = repo.working_files(100, 10).unwrap();
    assert_eq!(all, ["a/b/c/deep.txt", "a/three.txt", "one.txt", "two.txt"]);
    assert!(!truncated);
    // Two folders deep at most: `a/b/c/deep.txt` is left out.
    let (shallow, truncated) = repo.working_files(100, 2).unwrap();
    assert_eq!(shallow, ["a/three.txt", "one.txt", "two.txt"]);
    assert!(truncated);
    let (few, truncated) = repo.working_files(2, 10).unwrap();
    assert_eq!(few.len(), 2);
    assert!(truncated);
    let _ = std::fs::remove_dir_all(&dir);
}
