//! Everything OS-specific: storage paths, settings, projects folder, journal files, native dialogs,
//! secret store, per-user install (`vibefork://`, autostart, `PATH`), the background agent process.

pub mod access;
pub mod agent_process;
pub mod canvas;
pub mod dialog;
pub mod install;
pub mod journal;
pub mod projects;
pub mod prompts;
pub mod secrets;
pub mod settings;
pub mod storage;
pub mod system_lang;

pub use canvas::CanvasFiles;
pub use dialog::NativeConfirmDialog;
pub use journal::JournalFiles;
pub use projects::{ProjectError, Projects};
pub use settings::Settings;
pub use system_lang::system_language;
