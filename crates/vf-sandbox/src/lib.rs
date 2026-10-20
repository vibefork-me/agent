//! 🔒 **Security boundary.** Isolation of untrusted execution (builds, commands of a model).
//!
//! **Known limitation:** no isolation yet. The only implementation, [`NoopSandbox`], runs commands
//! with the user's rights. Planned implementations: microVM, OS sandbox, weak mode — behind the same
//! trait.
//!
//! `unsafe` is allowed in this crate only with a `// SAFETY:` justification at each site.

use std::path::Path;

/// How strong the isolation is — shown to the user honestly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    /// No isolation: runs with the user's rights.
    None,
    /// Weak: restricted token / environment only (Windows without admin).
    Weak,
    /// OS sandbox: Seatbelt, bubblewrap + Landlock, sandbox user + firewall.
    Os,
    /// Separate kernel: microVM.
    MicroVm,
}

/// What a sandboxed command may touch.
#[derive(Debug, Clone)]
pub struct Policy<'a> {
    /// The only writable folder.
    pub project_root: &'a Path,
    /// Network allowed (package registries, model API).
    pub network: bool,
}

/// Runs commands under some isolation. Implementations live here; callers depend on the trait.
pub trait Sandbox {
    /// The isolation this implementation really provides.
    fn level(&self) -> IsolationLevel;
}

/// The current implementation: **no isolation** (deferred). Kept explicit so it is visible in review.
#[derive(Debug, Default)]
pub struct NoopSandbox;

impl Sandbox for NoopSandbox {
    fn level(&self) -> IsolationLevel {
        IsolationLevel::None
    }
}
