//! What a project needs to build and run, installed into the agent's own folder via `mise` — no
//! administrator rights, nothing installed into the system.
//!
//! - [`detect`] — by layers: hints in `AGENTS.md` → version files → heuristics by build files;
//! - [`mise`] — the pinned `mise` binary (downloaded on first need, resumable, SHA-256 checked) and
//!   tool installs;
//! - [`prepare`] — installs what is needed and returns `PATH` entries and variables for the model's
//!   processes.

pub mod cli;
pub mod detect;
pub mod download;
pub mod mise;

use std::path::{Path, PathBuf};
use std::time::Instant;

use vf_protocol::Cancel;

pub use detect::{Need, detect};
pub use mise::Mise;

/// Progress of [`prepare`].
#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    /// Installing a tool (or `mise` itself).
    Installing {
        /// Tool spec.
        tool: String,
    },
    /// A tool is ready.
    Ready {
        /// Tool spec.
        tool: String,
        /// Install folder.
        path: PathBuf,
        /// Seconds spent.
        seconds: f64,
    },
    /// A tool failed.
    Failed {
        /// Tool spec.
        tool: String,
        /// Reason.
        error: String,
    },
}

/// What the model's processes get.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prepared {
    /// Folders put in front of `PATH`.
    pub paths: Vec<PathBuf>,
    /// Variables (`JAVA_HOME`, `GRADLE_USER_HOME`, …).
    pub env: Vec<(String, String)>,
}

/// Installs the tools `dir` needs; `caches` holds package caches (Gradle, …).
pub fn prepare(
    dir: &Path,
    tools: &Path,
    caches: &Path,
    cancel: &Cancel,
    progress: &mut dyn FnMut(Progress),
) -> Prepared {
    let mut out = Prepared::default();

    // Shared caches in the agent's caches folder:
    // No package manager or build tool leaks into the user's home profile.
    out.env.extend([
        (
            "GRADLE_USER_HOME".into(),
            caches.join("gradle").display().to_string(),
        ),
        ("GRADLE_OPTS".into(), "-Dorg.gradle.daemon=false".into()),
        (
            "MAVEN_OPTS".into(),
            format!("-Dmaven.repo.local={}", caches.join("maven").display()),
        ),
        (
            "npm_config_cache".into(),
            caches.join("npm").display().to_string(),
        ),
        (
            "YARN_CACHE_FOLDER".into(),
            caches.join("yarn").display().to_string(),
        ),
        (
            "pnpm_config_store_dir".into(),
            caches.join("pnpm").display().to_string(),
        ),
        (
            "COREPACK_HOME".into(),
            caches.join("corepack").display().to_string(),
        ),
        (
            "BUN_INSTALL_CACHE_DIR".into(),
            caches.join("bun").display().to_string(),
        ),
        (
            "PIP_CACHE_DIR".into(),
            caches.join("pip").display().to_string(),
        ),
        (
            "UV_CACHE_DIR".into(),
            caches.join("uv").display().to_string(),
        ),
        (
            "POETRY_CACHE_DIR".into(),
            caches.join("poetry").display().to_string(),
        ),
        (
            "GOPATH".into(),
            caches.join("go").display().to_string(),
        ),
        (
            "GOCACHE".into(),
            caches.join("go-build").display().to_string(),
        ),
        (
            "GOMODCACHE".into(),
            caches.join("go-mod").display().to_string(),
        ),
        ("GOTOOLCHAIN".into(), "local".into()),
        (
            "CARGO_HOME".into(),
            caches.join("cargo").display().to_string(),
        ),
    ]);

    let needs = detect(dir);
    if needs.is_empty() {
        return out;
    }
    let mise = Mise::new(tools);
    if !mise.is_installed() {
        progress(Progress::Installing {
            tool: "mise".into(),
        });
        let started = Instant::now();
        match mise.ensure(cancel) {
            Ok(()) => progress(Progress::Ready {
                tool: "mise".into(),
                path: mise.bin(),
                seconds: started.elapsed().as_secs_f64(),
            }),
            Err(error) => {
                progress(Progress::Failed {
                    tool: "mise".into(),
                    error,
                });
                return out;
            }
        }
    }
    for need in needs {
        if cancel.is_cancelled() {
            break;
        }
        let started = Instant::now();
        progress(Progress::Installing {
            tool: need.spec.clone(),
        });
        match mise.install(&need.spec, cancel) {
            Ok(home) => {
                out.paths.push(home.join("bin"));
                if need.name == "java" {
                    out.env
                        .push(("JAVA_HOME".into(), home.display().to_string()));
                }
                progress(Progress::Ready {
                    tool: need.spec,
                    path: home,
                    seconds: started.elapsed().as_secs_f64(),
                });
            }
            Err(error) => progress(Progress::Failed {
                tool: need.spec,
                error,
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_variables_are_isolated_in_caches_folder() {
        let temp = std::env::temp_dir().join(format!("vf-toolchains-test-{}", std::process::id()));
        let dir = temp.join("project");
        let tools = temp.join("tools");
        let caches = temp.join("caches");
        let _ = std::fs::create_dir_all(&dir);

        let cancel = Cancel::default();
        let prepared = prepare(&dir, &tools, &caches, &cancel, &mut |_| {});

        let get = |k: &str| -> Option<&str> {
            prepared
                .env
                .iter()
                .find(|(name, _)| name == k)
                .map(|(_, v)| v.as_str())
        };

        assert_eq!(
            get("GRADLE_USER_HOME"),
            Some(caches.join("gradle").display().to_string().as_str())
        );
        assert_eq!(get("GRADLE_OPTS"), Some("-Dorg.gradle.daemon=false"));
        assert_eq!(
            get("MAVEN_OPTS"),
            Some(format!("-Dmaven.repo.local={}", caches.join("maven").display()).as_str())
        );
        assert_eq!(
            get("npm_config_cache"),
            Some(caches.join("npm").display().to_string().as_str())
        );
        assert_eq!(
            get("YARN_CACHE_FOLDER"),
            Some(caches.join("yarn").display().to_string().as_str())
        );
        assert_eq!(
            get("pnpm_config_store_dir"),
            Some(caches.join("pnpm").display().to_string().as_str())
        );
        assert_eq!(
            get("COREPACK_HOME"),
            Some(caches.join("corepack").display().to_string().as_str())
        );
        assert_eq!(
            get("BUN_INSTALL_CACHE_DIR"),
            Some(caches.join("bun").display().to_string().as_str())
        );
        assert_eq!(
            get("PIP_CACHE_DIR"),
            Some(caches.join("pip").display().to_string().as_str())
        );
        assert_eq!(
            get("UV_CACHE_DIR"),
            Some(caches.join("uv").display().to_string().as_str())
        );
        assert_eq!(
            get("POETRY_CACHE_DIR"),
            Some(caches.join("poetry").display().to_string().as_str())
        );
        assert_eq!(
            get("GOPATH"),
            Some(caches.join("go").display().to_string().as_str())
        );
        assert_eq!(
            get("GOCACHE"),
            Some(caches.join("go-build").display().to_string().as_str())
        );
        assert_eq!(
            get("GOMODCACHE"),
            Some(caches.join("go-mod").display().to_string().as_str())
        );
        assert_eq!(get("GOTOOLCHAIN"), Some("local"));
        assert_eq!(
            get("CARGO_HOME"),
            Some(caches.join("cargo").display().to_string().as_str())
        );

        let _ = std::fs::remove_dir_all(&temp);
    }
}
