//! `vibefork install [--no-autostart]` / `vibefork uninstall` — per-user, without administrator
//! rights; `vibefork pairs [revoke <key>]` — paired browsers.

use std::process::ExitCode;
use std::sync::Arc;

use vf_core::ConfirmDialog;
use vf_platform::Settings;
use vf_platform::install::{self, Report};

fn print(report: &Report) -> ExitCode {
    for d in &report.done {
        println!("✓ {d}");
    }
    for f in &report.failed {
        println!("✗ {f}");
    }
    if report.failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

pub(crate) fn install(args: &[String]) -> ExitCode {
    let Ok(exe) = std::env::current_exe() else {
        return ExitCode::FAILURE;
    };
    // Off by default: the agent starts on demand (`vibefork://start`), no memory used until then.
    let autostart = args.iter().any(|a| a == "--autostart");
    let report = install::install(&exe, autostart);
    print(&report);
    // Only the binaries are required; the link handler, autostart and PATH are conveniences.
    if report.failed.iter().any(|f| f.starts_with("binaries")) {
        eprintln!("vibefork: installation failed");
        return ExitCode::FAILURE;
    }
    println!(
        "installed to {} — open a new terminal for `vibefork` on PATH",
        install::install_dir().display()
    );
    ExitCode::SUCCESS
}

/// `vibefork uninstall [--full [--with-projects]]`.
///
/// Without flags: the programs and registrations; data and projects stay. `--full`: also everything
/// the agent keeps (settings, pairings, journals, logs, toolchains, caches, builds, keys in the OS
/// secret store). `--with-projects`: also the projects folder, after the user types `DELETE`.
pub(crate) fn uninstall(args: &[String]) -> ExitCode {
    let s = Settings::from_env();
    let with_projects = args.iter().any(|a| a == "--with-projects");
    let full = with_projects || args.iter().any(|a| a == "--full");
    vf_platform::agent_process::stop(&s.state_dir);
    vf_sandbox::remove();
    let mut code = print(&install::uninstall());
    if !full {
        println!(
            "your projects and data stay in {} and {} (`vibefork uninstall --full` removes the data too)",
            s.storage.projects.display(),
            s.state_dir.display()
        );
        return code;
    }
    for name in vf_platform::secrets::NAMES {
        if let Err(e) = vf_platform::secrets::remove(name) {
            println!("✗ key {name}: {e}");
            code = ExitCode::FAILURE;
        }
    }
    println!("✓ keys");
    let st = &s.storage;
    for (what, dir) in [
        ("toolchains", &st.tools),
        ("caches", &st.caches),
        ("builds", &st.artifacts),
        ("settings and journals", &s.state_dir),
    ] {
        if remove_owned(what, dir).is_err() {
            code = ExitCode::FAILURE;
        }
    }
    if with_projects {
        let projects = vf_platform::Projects::new(&st.projects).list();
        if !projects.is_empty() {
            println!(
                "\nThese projects will be deleted with all your changes in them ({}):",
                st.projects.display()
            );
            for p in &projects {
                println!("  {p}");
            }
            println!("Changes not saved to GitHub are lost. Type DELETE to delete them:");
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            if line.trim() != "DELETE" {
                println!("projects kept in {}", st.projects.display());
                return code;
            }
        }
        if remove_owned("projects", &st.projects).is_err() {
            code = ExitCode::FAILURE;
        }
    } else {
        println!(
            "your projects stay in {} (`--with-projects` removes them)",
            st.projects.display()
        );
    }
    code
}

/// Removes a folder of the agent; refuses the home folder, a drive root and very short paths
/// (a wrong setting must never delete the user's files).
fn remove_owned(what: &str, dir: &std::path::Path) -> Result<(), ()> {
    if !dir.exists() {
        return Ok(());
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);
    let real = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let is_home = home
        .as_ref()
        .is_some_and(|h| h.canonicalize().map(|h| h == real).unwrap_or(false));
    let normal_parts = real
        .components()
        .filter(|c| matches!(c, std::path::Component::Normal(_)))
        .count();
    if is_home || normal_parts < 2 {
        println!(
            "✗ {what}: {} looks like a folder the agent does not own — not deleted",
            dir.display()
        );
        return Err(());
    }
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {
            println!("✓ {what} ({})", dir.display());
            Ok(())
        }
        Err(e) => {
            println!("✗ {what}: {e}");
            Err(())
        }
    }
}

/// `vibefork autostart [on | off | status]` — start the agent (the tray) at login.
pub(crate) fn autostart(args: &[String]) -> ExitCode {
    let report = |r: Result<(), String>, done: &str| match r {
        Ok(()) => {
            println!("{done}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("vibefork: {e}");
            ExitCode::FAILURE
        }
    };
    match args.first().map(String::as_str) {
        None | Some("status") => {
            println!(
                "start at login: {}",
                if install::autostart_enabled() {
                    "on"
                } else {
                    "off"
                }
            );
            ExitCode::SUCCESS
        }
        Some("on") => report(
            install::set_autostart(&install::install_dir()),
            "start at login: on",
        ),
        Some("off") => report(install::remove_autostart(), "start at login: off"),
        Some(_) => {
            eprintln!("usage: vibefork autostart [on | off | status]");
            ExitCode::from(2)
        }
    }
}

/// The CLI never shows a pairing dialog.
struct NoDialog;
impl ConfirmDialog for NoDialog {
    fn confirm(&self, _: &str, _: &str) -> bool {
        false
    }
}

pub(crate) fn pairs(args: &[String]) -> ExitCode {
    let s = Settings::from_env();
    let auth = match vf_auth::Auth::load(s.state_dir.clone(), Arc::new(NoDialog)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("vibefork: {e}");
            return ExitCode::FAILURE;
        }
    };
    match args {
        [] => {
            let pairs = auth.pairs();
            if pairs.is_empty() {
                println!("no paired browsers");
            }
            for p in pairs {
                println!("{}  {:<28} {}", p.key_id, p.origin, p.name);
            }
            ExitCode::SUCCESS
        }
        [cmd, key] if cmd == "revoke" => match auth.revoke(key) {
            Ok(()) => {
                println!("revoked {key}");
                // The running agent holds sessions in memory: restart it so they end now.
                crate::agent::restart_if_running(&s);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("vibefork: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: vibefork pairs [revoke <key id>]");
            ExitCode::from(2)
        }
    }
}
