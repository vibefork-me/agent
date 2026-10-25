//! `vibefork storage` — where the heavy folders are and how big they are; `vibefork storage move
//! <kind> <folder>` moves one of them (the agent must be stopped).

use std::path::Path;
use std::process::ExitCode;

use vf_platform::Settings;
use vf_platform::storage::{Kind, move_folder, usage};

pub(crate) fn run(args: &[String]) -> ExitCode {
    let mut settings = Settings::from_env();
    let lang = vf_platform::system_language();
    match args {
        [] => {
            let files_lbl = vf_core::catalog::cli_storage_header_files(lang);
            for kind in Kind::ALL {
                let path = kind.path(&settings.storage);
                let u = usage(path);
                println!(
                    "{:<10} {:>10}  {:>8} {files_lbl}  {}",
                    kind.name(),
                    size(u.bytes),
                    u.files,
                    path.display()
                );
            }
            ExitCode::SUCCESS
        }
        [cmd, kind, to] if cmd == "move" => {
            let Some(kind) = Kind::parse(kind) else {
                eprintln!(
                    "{}",
                    vf_core::catalog::cli_storage_unknown_folder(lang, kind)
                );
                return ExitCode::from(2);
            };
            if agent_running(settings.port) {
                eprintln!(
                    "{}",
                    vf_core::catalog::cli_storage_stop_agent_first(lang, settings.port)
                );
                return ExitCode::FAILURE;
            }
            match move_folder(&mut settings, kind, Path::new(to)) {
                Ok(u) => {
                    println!(
                        "{}",
                        vf_core::catalog::cli_storage_moved(
                            lang,
                            kind.name(),
                            &size(u.bytes),
                            u.files,
                            &kind.path(&settings.storage).display().to_string()
                        )
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("vibefork: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("usage: vibefork storage [move <projects|tools|caches|artifacts> <folder>]");
            ExitCode::from(2)
        }
    }
}

fn agent_running(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300)).is_ok()
}

fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}
