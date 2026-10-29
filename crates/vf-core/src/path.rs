//! [`ProjectPath`]: a relative path that cannot escape the project root.
//!
//! This is the lexical half of invariant 8 and does no IO. Symlink escapes are checked by the
//! adapter that touches the disk (`vf-platform`) after resolving the real path.

use std::path::{Component, Path, PathBuf};

/// A path relative to a project root, guaranteed (lexically) not to leave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPath(PathBuf);

/// Why a path was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathError {
    /// Absolute paths, drive prefixes (`C:`) and UNC paths are not allowed.
    #[error("path must be relative to the project root")]
    NotRelative,
    /// `..` would leave the project root.
    #[error("path must not leave the project root")]
    Escapes,
    /// Empty path.
    #[error("path is empty")]
    Empty,
}

impl ProjectPath {
    /// Parses an untrusted relative path (e.g. from the network or from a model's tool call).
    pub fn parse(raw: &str) -> Result<Self, PathError> {
        if raw.trim().is_empty() {
            return Err(PathError::Empty);
        }
        let mut clean = PathBuf::new();
        for component in Path::new(raw).components() {
            match component {
                // `:` — a drive (`C:` joined to a root replaces it on Windows) or an NTFS stream;
                // NUL — truncates the path in OS calls.
                Component::Normal(part) if part.to_string_lossy().contains([':', '\0']) => {
                    return Err(PathError::NotRelative);
                }
                Component::Normal(part) => clean.push(part),
                Component::CurDir => {}
                Component::ParentDir => {
                    if !clean.pop() {
                        return Err(PathError::Escapes);
                    }
                }
                Component::RootDir | Component::Prefix(_) => return Err(PathError::NotRelative),
            }
        }
        Ok(Self(clean))
    }

    /// The path joined to a project root. The result stays inside `root` lexically.
    pub fn under(&self, root: &Path) -> PathBuf {
        root.join(&self.0)
    }

    /// The relative path itself.
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Whether the path is a coding CLI's configuration in the project ([`CLI_CONFIG`]) or inside
    /// one. Case-insensitive; trailing dots and spaces of a component are ignored, as Windows does.
    pub fn is_cli_config(&self) -> bool {
        is_cli_config(&self.0)
    }
}

/// Coding CLI configuration a project may carry: hooks, helper commands, environment and MCP
/// servers the CLI runs by itself. The model never writes these paths.
pub const CLI_CONFIG: &[&str] = &[
    ".claude",
    ".codex",
    ".mcp.json",
    ".github/hooks",
    ".github/copilot",
];

/// [`ProjectPath::is_cli_config`] for a relative path that is already clean.
pub fn is_cli_config(rel: &Path) -> bool {
    let parts: Vec<String> = rel
        .components()
        .map(|c| {
            c.as_os_str()
                .to_string_lossy()
                .trim_end_matches(['.', ' '])
                .to_ascii_lowercase()
        })
        .collect();
    CLI_CONFIG.iter().any(|entry| {
        let entry: Vec<&str> = entry.split('/').collect();
        parts.len() >= entry.len() && parts.iter().zip(&entry).all(|(p, e)| p == e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_paths_inside() {
        assert_eq!(
            ProjectPath::parse("src/main.rs").map(|p| p.0),
            Ok(PathBuf::from("src/main.rs"))
        );
        assert_eq!(
            ProjectPath::parse("./a/../b").map(|p| p.0),
            Ok(PathBuf::from("b"))
        );
    }

    #[test]
    fn rejects_escapes_and_absolute() {
        assert_eq!(ProjectPath::parse("../secret"), Err(PathError::Escapes));
        assert_eq!(ProjectPath::parse("a/../../b"), Err(PathError::Escapes));
        assert_eq!(
            ProjectPath::parse("/etc/passwd"),
            Err(PathError::NotRelative)
        );
        assert_eq!(ProjectPath::parse(""), Err(PathError::Empty));
    }

    #[test]
    fn rejects_drives_streams_and_nul_inside() {
        // Found by fuzzing: on Windows `.?\C:\` parsed to the component `C:`, and
        // `root.join("C:")` left the project.
        for bad in [".?\\C:\\", "a/C:", "a/b:stream", "a\0b"] {
            assert_eq!(
                ProjectPath::parse(bad),
                Err(PathError::NotRelative),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn cli_config_is_recognized_in_any_spelling() {
        for config in [
            ".claude/settings.json",
            ".claude",
            ".CLAUDE/settings.local.json",
            ".claude./settings.json",
            "./x/../.codex/config.toml",
            ".mcp.json",
            ".MCP.JSON ",
            ".github/hooks/hooks.json",
            ".github/copilot/settings.json",
        ] {
            assert!(
                ProjectPath::parse(config).is_ok_and(|p| p.is_cli_config()),
                "{config:?}"
            );
        }
        for plain in [
            "src/.claude/x",
            "claude/settings.json",
            ".mcp.json.txt",
            ".github/workflows/ci.yml",
            ".claudex/a",
        ] {
            assert!(
                ProjectPath::parse(plain).is_ok_and(|p| !p.is_cli_config()),
                "{plain:?}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_prefixes() {
        assert_eq!(
            ProjectPath::parse(r"C:\Windows"),
            Err(PathError::NotRelative)
        );
        assert_eq!(
            ProjectPath::parse(r"\\server\share"),
            Err(PathError::NotRelative)
        );
    }
}
