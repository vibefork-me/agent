//! Isolation of untrusted execution as plain data: what a sandboxed process may touch, and how a
//! command is wrapped to run inside the sandbox. The sandbox itself is in `vf-sandbox`.

use std::ffi::OsString;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What a sandboxed process may touch. Everything else on disk is closed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxPolicy {
    /// Readable (and executable) folders and files.
    pub read: Vec<PathBuf>,
    /// Readable and writable folders and files.
    pub write: Vec<PathBuf>,
    /// Temporary folder inside the policy (Windows: the user's temp folder is outside it).
    #[serde(default)]
    pub temp: Option<PathBuf>,
}

/// How strong the isolation is — shown to the user as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationLevel {
    /// No isolation: runs with the user's rights.
    None,
    /// Weak: part of the system is protected (e.g. writes only).
    Weak,
    /// OS sandbox: files outside the policy are closed.
    Os,
    /// Separate kernel: microVM.
    MicroVm,
}

impl IsolationLevel {
    /// The wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Weak => "weak",
            Self::Os => "os",
            Self::MicroVm => "micro_vm",
        }
    }
}

/// A command prefix that runs the real program inside the sandbox:
/// `<program> <prefix…> <real program> <real args…>`, with extra variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrapper {
    /// The launcher.
    pub program: PathBuf,
    /// Arguments before the real program.
    pub prefix: Vec<OsString>,
    /// Variables for the launcher (removed before the real program starts).
    pub env: Vec<(OsString, OsString)>,
}
