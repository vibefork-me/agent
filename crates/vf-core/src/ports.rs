//! Ports: traits that logic and adapters depend on; implementations live in adapter crates and are
//! connected in `vf-agent`.

/// A native OS dialog — the one UI a web page cannot click. Used for pairing and dangerous actions.
pub trait ConfirmDialog: Send + Sync {
    /// Shows `text` with an Allow / Block choice. Blocks the calling thread; returns `false` on
    /// Block, on timeout and when no dialog can be shown.
    fn confirm(&self, title: &str, text: &str) -> bool;
}
