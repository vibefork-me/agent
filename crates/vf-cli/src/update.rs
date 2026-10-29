//! `vibefork update [--check | --rollback]` — signed self-update of the installed binaries.

use std::process::ExitCode;

use vf_platform::Settings;
use vf_platform::install::{BINARIES, Kind, current_kind, exe_name};
use vf_updater::release::{
    Release, appimage_name, asset_name, install, install_msi, is_newer, msi_name, rollback,
};

/// Outcome of an update check.
pub enum Outcome {
    /// Already the newest (the current version).
    Current(String),
    /// Updated to a version.
    Updated(String),
    /// A newer version exists (check only).
    Available(String),
}

/// A background check installs only when automatic updates are on. The signature checks are the
/// same either way; `vibefork update` still installs when this returns `true`.
pub fn background_check_only(auto_update: bool) -> bool {
    !auto_update
}

/// Checks the channel and, unless `check_only`, installs a newer version the way the running binaries
/// were installed ([`Kind`]): the Windows installer runs its newer version, an AppImage replaces
/// itself, other installations replace the binaries next to the running one.
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
    match current_kind() {
        // The installer keeps "Installed apps" and the uninstaller in step with the version.
        Kind::Msi if release.manifest.files.contains_key(&msi_name()) => {
            let msi = rt
                .block_on(release.download(&msi_name()))
                .map_err(|e| e.to_string())?;
            let names: Vec<String> = BINARIES.iter().map(|b| exe_name(b)).collect();
            let park = settings.state_dir.join("update");
            install_msi(&dir, &names, &msi, &version, &park).map_err(|e| e.to_string())?;
            return Ok(Outcome::Updated(version));
        }
        Kind::AppImage(image) => {
            let bytes = rt
                .block_on(release.download(&appimage_name()))
                .map_err(|e| e.to_string())?;
            install(&image, &bytes, &version).map_err(|e| e.to_string())?;
            return Ok(Outcome::Updated(version));
        }
        Kind::Msi | Kind::Portable | Kind::Files => {}
    }
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
        Some("--auto") => set_auto(args.get(1).map(String::as_str)),
        Some("--rollback") => {
            let exe = match current_kind() {
                Kind::AppImage(image) => image,
                _ => match std::env::current_exe() {
                    Ok(exe) => exe,
                    Err(_) => return ExitCode::FAILURE,
                },
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
                vf_platform::settings::clear_update_notice(&s.state_dir);
                println!("up to date ({v})");
                ExitCode::SUCCESS
            }
            Ok(Outcome::Available(v)) => {
                if let Err(e) = vf_platform::settings::write_update_notice(&s.state_dir, &v) {
                    eprintln!("vibefork: {e}");
                }
                println!("version {v} is available — `vibefork update`");
                ExitCode::SUCCESS
            }
            Ok(Outcome::Updated(v)) => {
                vf_platform::settings::clear_update_notice(&s.state_dir);
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
            eprintln!("usage: vibefork update [--check | --rollback | --auto on|off]");
            ExitCode::from(2)
        }
    }
}

fn set_auto(value: Option<&str>) -> ExitCode {
    let enabled = match value {
        Some("on") => true,
        Some("off") => false,
        _ => {
            eprintln!("usage: vibefork update --auto on|off");
            return ExitCode::from(2);
        }
    };
    let s = Settings::from_env();
    if let Err(e) = s.save_auto_update(enabled) {
        eprintln!("vibefork: {e}");
        return ExitCode::FAILURE;
    }
    println!("automatic updates: {}", if enabled { "on" } else { "off" });
    if std::env::var("VF_AUTO_UPDATE").is_ok() {
        println!("VF_AUTO_UPDATE overrides the saved setting in this process");
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::background_check_only;

    #[test]
    fn turned_off_checks_without_installing() {
        assert!(background_check_only(false));
        assert!(!background_check_only(true));
    }
}
