//! Canary: building projects leaves nothing in the user's home. The home folder is listed before and
//! after real builds (Rust with a crate from crates.io, Go, Node with a package from npm) on a clean
//! agent; any new or changed file outside the agent's folders, the repository and the system temp
//! folder fails the test with the list.
//!
//! Downloads toolchains and packages, so it is `#[ignore]`: CI runs it with
//! `cargo test -p vf-engine --test canary -- --ignored`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    missing_docs
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vf_engine::Engine;
use vf_platform::Settings;
use vf_platform::settings::Storage;
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, Cancel};

/// Size and modification time of every file under `root`, skipping `skip` (and below).
fn listing(root: &Path, skip: &[PathBuf]) -> BTreeMap<PathBuf, (u64, Option<SystemTime>)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if skip.iter().any(|s| dir.starts_with(s)) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
            } else if !skip.iter().any(|s| path.starts_with(s)) {
                out.insert(path, (meta.len(), meta.modified().ok()));
            }
        }
    }
    out
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .expect("home folder")
}

fn write(dir: &Path, files: &[(&str, &str)]) {
    for (name, text) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
#[ignore = "downloads toolchains and packages; CI runs it on purpose"]
fn builds_leave_nothing_in_the_home_folder() {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "vf-canary-{}",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let projects = base.join("projects");
    write(
        &projects.join("rust"),
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"canary\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nitoa = \"1\"\n",
            ),
            (
                "src/main.rs",
                "fn main() { println!(\"{}\", itoa::Buffer::new().format(7)); }\n",
            ),
        ],
    );
    write(
        &projects.join("go"),
        &[
            ("go.mod", "module canary\n\ngo 1.22\n"),
            (
                "main.go",
                "package main\n\nimport \"fmt\"\n\nfunc main() { fmt.Println(7) }\n",
            ),
        ],
    );
    write(
        &projects.join("node"),
        &[(
            "package.json",
            r#"{"name":"canary","version":"1.0.0","scripts":{"build":"npm install --no-audit --no-fund is-number@7 && node -e \"require('fs').mkdirSync('dist',{recursive:true});require('fs').writeFileSync('dist/out.txt',String(require('is-number')(7)))\""}}"#,
        )],
    );

    let settings = Settings {
        state_dir: base.join("state"),
        port: 1,
        extra_origins: vec![],
        ignored_extra_origins: vec![],
        ui_bundle_url: String::new(),
        storage: Storage {
            projects: projects.clone(),
            tools: base.join("tools"),
            caches: base.join("caches"),
            artifacts: base.join("artifacts"),
        },
        simulate_clean_machine: true,
        update_url: String::new(),
        auto_update: false,
    };
    let engine = Engine::new(&settings, vec![]);

    let home = home();
    // The repository (its target holds `base`) and the system temp folder may live under the home.
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let skip = vec![repo, base.clone(), std::env::temp_dir()];
    let before = listing(&home, &skip);

    for project in ["rust", "go", "node"] {
        let mut msgs = vec![];
        engine.build(project, &mut |m| msgs.push(m), &Cancel::new());
        assert!(
            matches!(msgs.last(), Some(ServerMessage::BuildDone { code: 0, .. })),
            "{project} did not build: {msgs:?}"
        );
    }

    let after = listing(&home, &skip);
    let leaks: Vec<String> = after
        .iter()
        .filter(|(path, meta)| before.get(*path) != Some(*meta))
        .map(|(path, _)| path.display().to_string())
        .collect();
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        leaks.is_empty(),
        "{} files appeared or changed in {}:\n{}",
        leaks.len(),
        home.display(),
        leaks
            .iter()
            .take(50)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
