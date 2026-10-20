//! Heavy folders: size and moving to another place or disk (`vibefork storage`).

use std::path::{Path, PathBuf};

use crate::settings::{Settings, Storage};

/// One of the heavy folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Projects.
    Projects,
    /// Toolchains.
    Tools,
    /// Package caches.
    Caches,
    /// Build artifacts.
    Artifacts,
}

impl Kind {
    /// All kinds.
    pub const ALL: [Kind; 4] = [Kind::Projects, Kind::Tools, Kind::Caches, Kind::Artifacts];

    /// Name used on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Projects => "projects",
            Kind::Tools => "tools",
            Kind::Caches => "caches",
            Kind::Artifacts => "artifacts",
        }
    }

    /// Kind by name.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The folder of this kind.
    pub fn path(self, storage: &Storage) -> &Path {
        match self {
            Kind::Projects => &storage.projects,
            Kind::Tools => &storage.tools,
            Kind::Caches => &storage.caches,
            Kind::Artifacts => &storage.artifacts,
        }
    }

    fn path_mut(self, storage: &mut Storage) -> &mut PathBuf {
        match self {
            Kind::Projects => &mut storage.projects,
            Kind::Tools => &mut storage.tools,
            Kind::Caches => &mut storage.caches,
            Kind::Artifacts => &mut storage.artifacts,
        }
    }
}

/// Files and bytes in a folder (links are not followed).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Number of files.
    pub files: u64,
    /// Total size, bytes.
    pub bytes: u64,
}

/// Size of a folder; a missing folder is empty.
pub fn usage(dir: &Path) -> Usage {
    let mut total = Usage::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return total;
    };
    for e in entries.filter_map(Result::ok) {
        let Ok(meta) = e.path().symlink_metadata() else {
            continue;
        };
        if meta.is_dir() {
            let sub = usage(&e.path());
            total.files += sub.files;
            total.bytes += sub.bytes;
        } else {
            total.files += 1;
            total.bytes += meta.len();
        }
    }
    total
}

/// Errors of a move.
#[derive(Debug, thiserror::Error)]
pub enum MoveError {
    /// The target is inside the source or the other way round.
    #[error("the new folder must not be inside the old one or contain it")]
    Nested,
    /// The target exists and is not empty.
    #[error("the new folder is not empty")]
    NotEmpty,
    /// The copy differs from the source; the source is kept.
    #[error("copy check failed: {0:?} vs {1:?}; the old folder is kept")]
    Mismatch(Usage, Usage),
    /// Disk error.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Moves a heavy folder: copy, compare, store the new path, then delete the old folder.
pub fn move_folder(settings: &mut Settings, kind: Kind, to: &Path) -> Result<Usage, MoveError> {
    let from = kind.path(&settings.storage).to_path_buf();
    let to = absolute(to)?;
    let from_abs = absolute(&from)?;
    if to.starts_with(&from_abs) || from_abs.starts_with(&to) {
        return Err(MoveError::Nested);
    }
    if to.exists() && std::fs::read_dir(&to)?.next().is_some() {
        return Err(MoveError::NotEmpty);
    }
    std::fs::create_dir_all(&to)?;
    if from.exists() {
        copy_dir(&from, &to)?;
    }
    let (a, b) = (usage(&from), usage(&to));
    if a != b {
        return Err(MoveError::Mismatch(a, b));
    }
    let mut storage = settings.storage.clone();
    *kind.path_mut(&mut storage) = to;
    settings.save_storage(&storage)?;
    settings.storage = storage;
    if from.exists() {
        std::fs::remove_dir_all(&from)?;
    }
    Ok(b)
}

fn absolute(p: &Path) -> std::io::Result<PathBuf> {
    // Compare by real paths (links, `8.3` names, case on Windows): the nearest existing ancestor is
    // resolved, the missing rest is appended.
    let p = std::path::absolute(p)?;
    let mut rest = Vec::new();
    let mut cur = p.as_path();
    loop {
        if let Ok(real) = cur.canonicalize() {
            return Ok(rest.iter().rev().fold(real, |acc, part| acc.join(part)));
        }
        match (cur.parent(), cur.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                cur = parent;
            }
            _ => return Ok(p),
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let meta = e.path().symlink_metadata()?;
        let target = to.join(e.file_name());
        if meta.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else if meta.file_type().is_symlink() {
            copy_link(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_link(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(std::fs::read_link(from)?, to)
}

#[cfg(not(unix))]
fn copy_link(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::copy(from, to).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_copies_checks_and_deletes() {
        let base = std::env::temp_dir().join(format!("vf-storage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let state = base.join("state");
        let old = base.join("old-caches");
        assert!(std::fs::create_dir_all(old.join("gradle/x")).is_ok());
        assert!(std::fs::write(old.join("gradle/x/a.jar"), vec![7u8; 1000]).is_ok());
        assert!(std::fs::write(old.join("b"), "b").is_ok());
        let mut s = Settings {
            state_dir: state.clone(),
            port: 1,
            extra_origins: Vec::new(),
            ui_bundle_url: String::new(),
            storage: Storage {
                projects: base.join("p"),
                tools: base.join("t"),
                caches: old.clone(),
                artifacts: base.join("a"),
            },
            simulate_clean_machine: false,
            update_url: String::new(),
            auto_update: false,
        };
        assert!(matches!(
            move_folder(&mut s, Kind::Caches, &old.join("inner")),
            Err(MoveError::Nested)
        ));
        let new = base.join("new-caches");
        let moved = move_folder(&mut s, Kind::Caches, &new);
        assert_eq!(
            moved.ok(),
            Some(Usage {
                files: 2,
                bytes: 1001
            })
        );
        assert!(!old.exists());
        assert!(new.join("gradle/x/a.jar").exists());
        let stored = crate::settings::read_config(&state)
            .storage
            .and_then(|st| st.caches);
        assert_eq!(
            stored.map(|p| p.canonicalize().ok()),
            Some(new.canonicalize().ok())
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
