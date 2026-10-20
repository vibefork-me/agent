//! What a project needs to build and run, installed into the agent's own folder via `mise` — no
//! administrator rights, nothing installed into the system.
//!
//! - [`detect`] — by layers: hints in `AGENTS.md` → version files → heuristics by build files;
//! - [`mise`] — the pinned `mise` binary (downloaded on first need, resumable, SHA-256 checked) and
//!   tool installs;
//! - [`prepare`] — installs what is needed and returns `PATH` entries and variables for the model's
//!   processes.

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
    let needs = detect(dir);
    let mut out = Prepared::default();
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
    // Shared Gradle cache in the agent's caches folder; no Gradle daemon left running after a task.
    out.env.push((
        "GRADLE_USER_HOME".into(),
        caches.join("gradle").display().to_string(),
    ));
    out.env
        .push(("GRADLE_OPTS".into(), "-Dorg.gradle.daemon=false".into()));
    out
}
