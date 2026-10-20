//! Everything OS-specific: storage paths, settings, projects folder, journal files, native dialogs,
//! secret store, per-user install (`vibefork://`, autostart, `PATH`), the background agent process.

pub mod agent_process;
pub mod dialog;
pub mod install;
pub mod journal;
pub mod projects;
pub mod secrets;
pub mod settings;
pub mod storage;

pub use dialog::NativeConfirmDialog;
pub use journal::JournalFiles;
pub use projects::{ProjectError, Projects};
pub use settings::Settings;
