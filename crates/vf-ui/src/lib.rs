//! `vf-ui`: the agent's own windows on egui — a confirm prompt, text and secret prompts, the first
//! run and the agent's status — in the Vibefork look (dark theme, the editor's accents).
//!
//! The crate knows nothing about the agent: every string comes from the caller (already in the
//! person's language), every answer goes back as a value. Each window exists twice:
//! - a **view** ([`DialogView`], [`FirstRunView`], [`StatusView`]) that draws into any egui [`egui::Ui`],
//!   for a program that already runs egui;
//! - a **standalone window** ([`Shell`]) that opens a native window, blocks until it closes and
//!   returns the answer.
//!
//! Text shown in a window is plain text: control characters and layout tricks (bidi controls,
//! zero-width characters) are removed and the length is capped ([`text`]); egui has no markup.
//! Screen readers get every label, button and field through AccessKit (a secret field reads as a
//! password field, without its content).

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod dialog;
pub mod first_run;
pub mod status;
pub mod text;
pub mod theme;
mod window;

pub use dialog::{Answer, Dialog, DialogKind, DialogView};
pub use eframe::egui;
pub use first_run::{FirstRun, FirstRunAnswer, FirstRunText, FirstRunView};
pub use status::{StatusEvent, StatusPage, StatusView, Tone};
pub use window::Shell;

/// Why a standalone window could not be shown.
#[derive(Debug, thiserror::Error)]
pub enum UiError {
    /// No display, no graphics driver, or the window system refused the window. The caller falls
    /// back to another way of asking (e.g. a system dialog).
    #[error("cannot open a window: {0}")]
    Open(String),
}
