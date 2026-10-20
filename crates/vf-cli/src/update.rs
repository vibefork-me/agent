//! `vibefork update [--check | --rollback]` — signed self-update of the installed binaries.

use std::process::ExitCode;

use vf_platform::Settings;
use vf_platform::install::{BINARIES, exe_name};
use vf_updater::release::{Release, asset_name, install, is_newer, rollback};

/// Outcome of an update check.
pub enum Outcome {
    /// Already the newest (the current version).
    Current(String),
    /// Updated to a version.
    Updated(String),
    /// A newer version exists (check only).
    Available(String),
}

/// Checks the channel and, unless `check_only`, installs a newer version next to the running binary.
pub fn update(settings: &Settings, check_only: bool) -> Result<Outcome, String> {
    let key = vf_updater::signing::public_key(vf_updater::RELEASE_PUBLIC_KEY)
        .map_err(|e| e.to_string())?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let release = rt
        .block_on(Release::fetch(&settings.update_url, &key))
        .map_err(|e| e.to_string())?;
    let current = env!("CARGO_PKG_VERSION");
    let version = release.version().to_owned();
    if !is_newer(&version, current) {
        return Ok(Outcome::Current(current.to_owned()));
    }
    if check_only {
        return Ok(Outcome::Available(version));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    for binary in BINARIES {
        let path = dir.join(exe_name(binary));
        let asset = asset_name(binary);
        // The tray is updated only when it is installed and the release has it.
        if *binary != "vibefork" && (!path.exists() || !release.manifest.files.contains_key(&asset))
        {
            continue;
        }
        let bytes = rt
            .block_on(release.download(&asset))
            .map_err(|e| e.to_string())?;
        if *binary == "vibefork" {
            install(&path, &bytes, &version).map_err(|e| e.to_string())?;
        } else {
            // The tray does not answer `version` the same way: replace it after the core passed.
            let old = vf_updater::release::previous(&path);
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&path, &old).map_err(|e| e.to_string())?;
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        }
    }
    Ok(Outcome::Updated(version))
}

pub(crate) fn run(args: &[String]) -> ExitCode {
    let s = Settings::from_env();
    match args.first().map(String::as_str) {
        Some("--rollback") => {
            let Ok(exe) = std::env::current_exe() else {
                return ExitCode::FAILURE;
            };
            match rollback(&exe) {
                Ok(()) => {
                    println!("rolled back to the previous version");
                    crate::agent::restart_if_running(&s);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("vibefork: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        flag @ (None | Some("--check")) => match update(&s, flag.is_some()) {
            Ok(Outcome::Current(v)) => {
                println!("up to date ({v})");
                ExitCode::SUCCESS
            }
            Ok(Outcome::Available(v)) => {
                println!("version {v} is available — `vibefork update`");
                ExitCode::SUCCESS
            }
            Ok(Outcome::Updated(v)) => {
                println!("updated to {v}");
                crate::agent::restart_if_running(&s);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("vibefork: update failed: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: vibefork update [--check | --rollback]");
            ExitCode::from(2)
        }
    }
}
