//! The projects folder: one subfolder (a git repository) per project.

use std::path::{Path, PathBuf};

/// Errors of project lookup.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProjectError {
    /// The name is not `[A-Za-z0-9_.-]{1,80}` or is `.` / `..`.
    #[error("invalid project name")]
    InvalidName,
    /// No such project.
    #[error("project not found")]
    NotFound,
    /// The folder resolves outside the projects folder (a link).
    #[error("project folder is outside the projects folder")]
    Escapes,
    /// Disk error.
    #[error("{0}")]
    Io(String),
}

/// Whether a project name is safe as a single folder name on every OS.
pub fn valid_name(name: &str) -> bool {
    (1..=80).contains(&name.len())
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// The projects folder.
#[derive(Debug, Clone)]
pub struct Projects {
    root: PathBuf,
}

impl Projects {
    /// Projects in `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Names of the projects, sorted.
    pub fn list(&self) -> Vec<String> {
        let Ok(dir) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut names: Vec<String> = dir
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| valid_name(n))
            .collect();
        names.sort();
        names
    }

    /// The folder of an existing project, verified to stay inside the projects folder.
    pub fn resolve(&self, name: &str) -> Result<PathBuf, ProjectError> {
        if !valid_name(name) {
            return Err(ProjectError::InvalidName);
        }
        let path = self.root.join(name);
        if !path.is_dir() {
            return Err(ProjectError::NotFound);
        }
        let root = self
            .root
            .canonicalize()
            .map_err(|e| ProjectError::Io(e.to_string()))?;
        let real = path
            .canonicalize()
            .map_err(|e| ProjectError::Io(e.to_string()))?;
        if real.parent() != Some(root.as_path()) {
            return Err(ProjectError::Escapes);
        }
        Ok(path)
    }

    /// The folder for a new project (must not exist yet).
    pub fn new_folder(&self, name: &str) -> Result<PathBuf, ProjectError> {
        if !valid_name(name) {
            return Err(ProjectError::InvalidName);
        }
        std::fs::create_dir_all(&self.root).map_err(|e| ProjectError::Io(e.to_string()))?;
        let path = self.root.join(name);
        if path.exists() {
            return Err(ProjectError::Io("project already exists".into()));
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        for ok in ["demo", "my-app_2", "v1.2"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "",
            ".",
            "..",
            ".git",
            "a/b",
            "a\\b",
            "C:",
            "имя",
            &"x".repeat(81),
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn list_and_resolve() {
        let root = std::env::temp_dir().join(format!("vf-projects-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let p = Projects::new(&root);
        assert!(p.list().is_empty(), "missing root");
        assert!(std::fs::create_dir_all(root.join("b")).is_ok());
        assert!(std::fs::create_dir_all(root.join("a")).is_ok());
        assert!(std::fs::create_dir_all(root.join(".hidden")).is_ok());
        assert!(std::fs::write(root.join("file"), "x").is_ok());
        assert_eq!(p.list(), ["a", "b"]);
        assert!(p.resolve("a").is_ok());
        assert_eq!(p.resolve("c"), Err(ProjectError::NotFound));
        assert_eq!(p.resolve("../x"), Err(ProjectError::InvalidName));
        let _ = std::fs::remove_dir_all(&root);
    }
}
