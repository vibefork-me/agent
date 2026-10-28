//! `vibefork-tray`: the tray icon, menu and notifications of the agent.
//!
//! A separate process: the core has no GUI dependencies and runs headless; a tray crash does not stop
//! the core. It starts the agent when launched (autostart at login points here), shows its state
//! (`GET /v1/hello` and the projects' session streams), notifies when a task ends, a build fails or
//! the agent waits for a decision, and acts through the `vibefork` binary next to it.
//!
//! Backends: Windows / macOS — the system tray (`tray-icon` + `tao`); Linux — StatusNotifierItem
//! over D-Bus in pure Rust (`ksni`), no GTK. Notifications — `notify-rust` (D-Bus via zbus on Linux).

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod icon;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod native;
mod status;

fn main() {
    let app = app::App::new();
    #[cfg(windows)]
    {
        // Toasts of an unpackaged app need its AppUserModelID registered.
        let _ = vf_platform::install::register_app_user_model_id();
    }
    app.start_agent();
    #[cfg(target_os = "linux")]
    linux::run(app);
    #[cfg(not(target_os = "linux"))]
    native::run(app);
}
