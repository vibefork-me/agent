//! Read-only access to project files for the code viewer. Paths come from the browser and are
//! untrusted: they are parsed as [`ProjectPath`], must stay inside the project after following
//! links, and never reach git data or files that usually hold secrets.

use std::path::{Path, PathBuf};

use vf_core::ProjectPath;
use vf_git::Repo;
use vf_protocol::BackendError;
use vf_protocol::http::{FileDiffResponse, FileResponse};

use crate::Engine;

/// Files larger than this are described, not sent.
pub(crate) const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Whether a project-relative path is git data or a file that usually holds secrets.
/// Windows reads `name::$DATA`, `name.` and `name ` as `name`: such paths are refused outright.
pub(crate) fn is_denied(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('/').collect();
    let name = parts.last().copied().unwrap_or_default();
    lower.contains(':')
        || parts.iter().any(|p| p.ends_with('.') || p.ends_with(' '))
        || parts.iter().any(|p| *p == ".git" || *p == ".ssh")
        || name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".env")
        || name == ".dev.vars"
        || name.starts_with(".dev.vars.")
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name.starts_with("id_ecdsa")
        || name == ".npmrc"
        || name == ".netrc"
        || name == "credentials"
        || name == "credentials.json"
        || [".pem", ".key", ".p12", ".pfx", ".keystore", ".jks"]
            .iter()
            .any(|e| name.ends_with(e))
}

/// Text of a file, or why it is not sent.
struct Content {
    text: Option<String>,
    size: u64,
    too_large: bool,
    binary: bool,
}

fn content(bytes: Vec<u8>, size: u64) -> Content {
    match String::from_utf8(bytes) {
        Ok(text) => Content {
            text: Some(text),
            size,
            too_large: false,
            binary: false,
        },
        Err(_) => Content {
            text: None,
            size,
            too_large: false,
            binary: true,
        },
    }
}

impl Engine {
    /// The project folder and the checked relative path (`/`-separated).
    fn checked(&self, project: &str, raw: &str) -> Result<(PathBuf, String), BackendError> {
        let root = self
            .projects
            .resolve(project)
            .map_err(|_| BackendError::NotFound("no such project".into()))?;
        let rel = ProjectPath::parse(raw)
            .map_err(|_| BackendError::BadRequest("not a path inside the project".into()))?;
        let rel = rel.as_path().to_string_lossy().replace('\\', "/");
        if is_denied(&rel) {
            return Err(BackendError::Forbidden("this file is not shown".into()));
        }
        Ok((root, rel))
    }

    /// The working-tree file, after checking that links do not lead out of the project.
    fn working_file(root: &Path, rel: &str) -> Result<Option<Content>, BackendError> {
        let path = root.join(rel);
        let Ok(real) = path.canonicalize() else {
            return Ok(None);
        };
        let real_root = root
            .canonicalize()
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        if !real.starts_with(&real_root) {
            return Err(BackendError::Forbidden(
                "the path leads out of the project".into(),
            ));
        }
        let rel_real = real
            .strip_prefix(&real_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if is_denied(&rel_real) {
            return Err(BackendError::Forbidden("this file is not shown".into()));
        }
        let meta = std::fs::metadata(&real).map_err(|e| BackendError::Internal(e.to_string()))?;
        if !meta.is_file() {
            return Err(BackendError::BadRequest("not a file".into()));
        }
        if meta.len() > MAX_FILE_BYTES {
            return Ok(Some(Content {
                text: None,
                size: meta.len(),
                too_large: true,
                binary: false,
            }));
        }
        let bytes = std::fs::read(&real).map_err(|e| BackendError::Internal(e.to_string()))?;
        Ok(Some(content(bytes, meta.len())))
    }

    pub(crate) fn read_project_file(
        &self,
        project: &str,
        path: &str,
    ) -> Result<FileResponse, BackendError> {
        let (root, rel) = self.checked(project, path)?;
        let c = Self::working_file(&root, &rel)?
            .ok_or_else(|| BackendError::NotFound("no such file".into()))?;
        Ok(FileResponse {
            path: rel,
            content: c.text,
            size: c.size,
            too_large: c.too_large,
            binary: c.binary,
        })
    }

    /// The file as the project was taken (the start commit) and as it is now.
    pub(crate) fn read_project_diff(
        &self,
        project: &str,
        path: &str,
    ) -> Result<FileDiffResponse, BackendError> {
        let (root, rel) = self.checked(project, path)?;
        let now = Self::working_file(&root, &rel)?;
        let base = self
            .load_meta(project)
            .map(|m| m.local_base)
            .filter(|b| !b.is_empty());
        let before = match &base {
            Some(commit) => Repo::open(root.clone())
                .file_at(commit, &rel, MAX_FILE_BYTES)
                .map_err(|e| BackendError::Internal(e.0))?
                .map(|(bytes, size)| {
                    if size > MAX_FILE_BYTES {
                        Content {
                            text: None,
                            size,
                            too_large: true,
                            binary: false,
                        }
                    } else {
                        content(bytes, size)
                    }
                }),
            None => None,
        };
        let too_large = now.as_ref().is_some_and(|c| c.too_large)
            || before.as_ref().is_some_and(|c| c.too_large);
        let binary =
            now.as_ref().is_some_and(|c| c.binary) || before.as_ref().is_some_and(|c| c.binary);
        Ok(FileDiffResponse {
            path: rel,
            base,
            original: before.and_then(|c| c.text),
            modified: now.and_then(|c| c.text),
            too_large,
            binary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::is_denied;

    #[test]
    fn git_data_and_secret_files_are_denied() {
        for p in [
            ".git/config",
            "sub/.git/HEAD",
            ".env",
            "app/.env.local",
            "prod.env",
            ".dev.vars",
            "keys/id_rsa",
            "certs/server.KEY",
            "store.p12",
            ".npmrc",
            "home/.ssh/known_hosts",
            "credentials.json",
            ".env::$DATA",
            ".env ",
            ".git./config",
            "src/main.rs:stream",
        ] {
            assert!(is_denied(p), "{p}");
        }
        for p in [
            "src/main.rs",
            "README.md",
            "envelope.txt",
            "docs/keys.md",
            "environment.ts",
        ] {
            assert!(!is_denied(p), "{p}");
        }
    }
}
