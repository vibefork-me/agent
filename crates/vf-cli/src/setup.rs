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
    let autostart = !args.iter().any(|a| a == "--no-autostart");
    let code = print(&install::install(&exe, autostart));
    println!(
        "installed to {} — open a new terminal for `vibefork` on PATH",
        install::install_dir().display()
    );
    code
}

pub(crate) fn uninstall() -> ExitCode {
    let s = Settings::from_env();
    vf_platform::agent_process::stop(&s.state_dir);
    let code = print(&install::uninstall());
    println!(
        "your projects and data stay in {} and {}",
        s.storage.projects.display(),
        s.state_dir.display()
    );
    code
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
