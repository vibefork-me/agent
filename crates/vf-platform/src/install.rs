//! Per-user installation without administrator rights: the binaries in a user folder, the
//! `vibefork://` handler, autostart at login and the user's `PATH`.
//!
//! | | Windows | macOS | Linux |
//! |---|---|---|---|
//! | Binaries | `%LOCALAPPDATA%\Programs\vibefork` | `~/Applications/Vibefork.app/Contents/MacOS` | `~/.local/bin` |
//! | `vibefork://` | `HKCU\Software\Classes\vibefork` | `CFBundleURLTypes` in `Info.plist` | `~/.local/share/applications/vibefork.desktop` + `xdg-mime` |
//! | Autostart (off by default: the agent starts on demand from a `vibefork://start` link) | `HKCU\…\Run` value `vibefork` | `~/Library/LaunchAgents/me.vibefork.agent.plist` | `~/.config/autostart/vibefork.desktop` |
//! | Notifications (AppUserModelID [`APP_USER_MODEL_ID`]) | `HKCU\Software\Classes\AppUserModelId\me.vibefork.agent` | bundle id of the app | — (D-Bus) |
//! | `PATH` | user `Path` variable | — (`~/Applications/…` link not needed for the app) | `~/.local/bin` is on `PATH` by default |

use std::path::{Path, PathBuf};
use std::process::Command;

/// Binaries that make up an installation.
pub const BINARIES: &[&str] = &["vibefork", "vibefork-tray"];

/// The app's identity for notifications: the Windows AppUserModelID of the tray's toasts (and the
/// macOS bundle id).
pub const APP_USER_MODEL_ID: &str = "me.vibefork.agent";

/// Folder the binaries are installed into.
pub fn install_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("VF_INSTALL_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    let home = home();
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"))
            .join("Programs")
            .join("vibefork")
    } else if cfg!(target_os = "macos") {
        home.join("Applications")
            .join("Vibefork.app")
            .join("Contents")
            .join("MacOS")
    } else {
        home.join(".local").join("bin")
    }
}

/// File name of a binary on this OS.
pub fn exe_name(binary: &str) -> String {
    if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_owned()
    }
}

/// What an installation did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Done steps, for the user.
    pub done: Vec<String>,
    /// Steps that failed (the rest still ran).
    pub failed: Vec<String>,
}

impl Report {
    fn step(&mut self, what: &str, r: Result<(), String>) {
        match r {
            Ok(()) => self.done.push(what.to_owned()),
            Err(e) => self.failed.push(format!("{what}: {e}")),
        }
    }
}

/// Copies the binaries found next to `from` into [`install_dir`] and registers the handler,
/// autostart and `PATH`.
pub fn install(from: &Path, autostart: bool) -> Report {
    let mut r = Report::default();
    let dir = install_dir();
    let src_dir = from.parent().map(Path::to_path_buf).unwrap_or_default();
    r.step(
        &format!("binaries → {}", dir.display()),
        copy_binaries(&src_dir, &dir),
    );
    let exe = dir.join(exe_name("vibefork"));
    #[cfg(target_os = "macos")]
    r.step("app bundle", macos::write_bundle(&dir));
    r.step("vibefork:// handler", register_protocol(&exe));
    #[cfg(windows)]
    r.step("notifications", register_app_user_model_id());
    if autostart {
        r.step("start at login", set_autostart(&dir));
    }
    #[cfg(windows)]
    r.step("PATH", windows::add_to_path(&dir));
    r
}

/// Removes what [`install`] added (data folders stay).
pub fn uninstall() -> Report {
    let mut r = Report::default();
    r.step("vibefork:// handler", unregister_protocol());
    r.step("start at login", remove_autostart());
    #[cfg(windows)]
    r.step(
        "notifications",
        windows::reg(&["delete", windows::APP_ID_KEY, "/f"]).or_else(windows::missing_is_ok),
    );
    let dir = install_dir();
    #[cfg(windows)]
    r.step("PATH", windows::remove_from_path(&dir));
    // A running binary cannot be deleted on Windows; it is renamed and removed on the next run.
    r.step("binaries", remove_binaries(&dir));
    r
}

fn copy_binaries(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let mut copied = 0;
    for b in BINARIES {
        let from = src.join(exe_name(b));
        let to = dst.join(exe_name(b));
        if !from.is_file() || same_file(&from, &to) {
            continue;
        }
        if to.exists() {
            // Replace a running binary by renaming it first.
            let old = to.with_extension("old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&to, &old).map_err(|e| e.to_string())?;
        }
        std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        copied += 1;
    }
    if copied == 0 && !dst.join(exe_name("vibefork")).is_file() {
        return Err("no binaries found next to this program".into());
    }
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

fn remove_binaries(dir: &Path) -> Result<(), String> {
    for b in BINARIES {
        let p = dir.join(exe_name(b));
        if p.exists() && std::fs::remove_file(&p).is_err() {
            let trash = p.with_extension("old");
            let _ = std::fs::remove_file(&trash);
            std::fs::rename(&p, &trash).map_err(|e| e.to_string())?;
        }
        let _ = std::fs::remove_file(dir.join(format!(
            "{}.previous{}",
            b,
            if cfg!(windows) { ".exe" } else { "" }
        )));
    }
    if cfg!(windows) {
        let _ = std::fs::remove_dir(dir);
    }
    Ok(())
}

/// Registers `vibefork://` to run `<exe> handle <url>`.
pub fn register_protocol(exe: &Path) -> Result<(), String> {
    #[cfg(windows)]
    return windows::register_protocol(exe);
    #[cfg(target_os = "linux")]
    return linux::register_protocol(exe);
    #[cfg(target_os = "macos")]
    return macos::register_protocol(exe);
    #[allow(unreachable_code)]
    {
        let _ = exe;
        Err("not supported on this OS".into())
    }
}

/// Removes the `vibefork://` handler.
pub fn unregister_protocol() -> Result<(), String> {
    #[cfg(windows)]
    return windows::reg(&["delete", windows::PROTOCOL_KEY, "/f"]).or_else(windows::missing_is_ok);
    #[cfg(target_os = "linux")]
    return remove_file_ok(&linux::desktop_file());
    #[allow(unreachable_code)]
    Ok(())
}

/// Registers [`APP_USER_MODEL_ID`] under `HKCU\Software\Classes\AppUserModelId` with the
/// display name "Vibefork": Windows shows toasts of an unpackaged app only for a registered id.
/// Does nothing when the name is already there.
#[cfg(windows)]
pub fn register_app_user_model_id() -> Result<(), String> {
    if windows::query(windows::APP_ID_KEY, Some("DisplayName")).as_deref() == Some("Vibefork") {
        return Ok(());
    }
    windows::reg(&[
        "add",
        windows::APP_ID_KEY,
        "/v",
        "DisplayName",
        "/d",
        "Vibefork",
        "/f",
    ])
}

/// Starts the tray (or the agent) at login.
pub fn set_autostart(dir: &Path) -> Result<(), String> {
    let tray = dir.join(exe_name("vibefork-tray"));
    let (program, args): (PathBuf, &[&str]) = if tray.is_file() {
        (tray, &[])
    } else {
        (dir.join(exe_name("vibefork")), &["start"])
    };
    #[cfg(windows)]
    {
        let line = std::iter::once(format!("\"{}\"", program.display()))
            .chain(args.iter().map(|a| (*a).to_owned()))
            .collect::<Vec<_>>()
            .join(" ");
        return windows::reg(&[
            "add",
            windows::RUN_KEY,
            "/v",
            "vibefork",
            "/t",
            "REG_SZ",
            "/d",
            &line,
            "/f",
        ]);
    }
    #[cfg(target_os = "linux")]
    return linux::autostart(&program, args);
    #[cfg(target_os = "macos")]
    return macos::autostart(&program, args);
    #[allow(unreachable_code)]
    {
        let _ = (program, args);
        Err("not supported on this OS".into())
    }
}

/// Removes autostart.
pub fn remove_autostart() -> Result<(), String> {
    #[cfg(windows)]
    return windows::reg(&["delete", windows::RUN_KEY, "/v", "vibefork", "/f"])
        .or_else(windows::missing_is_ok);
    #[cfg(target_os = "linux")]
    return remove_file_ok(&linux::autostart_file());
    #[cfg(target_os = "macos")]
    return remove_file_ok(&macos::launch_agent());
    #[allow(unreachable_code)]
    Ok(())
}

/// Whether the agent starts at login.
pub fn autostart_enabled() -> bool {
    #[cfg(windows)]
    return windows::query(windows::RUN_KEY, Some("vibefork")).is_some();
    #[cfg(target_os = "linux")]
    return linux::autostart_file().exists();
    #[cfg(target_os = "macos")]
    return macos::launch_agent().exists();
    #[allow(unreachable_code)]
    false
}

/// Whether the handler is registered to an existing program (for `doctor`).
pub fn protocol_target() -> Option<String> {
    #[cfg(windows)]
    return windows::query(windows::PROTOCOL_COMMAND_KEY, None);
    #[cfg(target_os = "linux")]
    return std::fs::read_to_string(linux::desktop_file())
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("Exec="))
                .map(str::to_owned)
        });
    #[allow(unreachable_code)]
    None
}

#[allow(dead_code)]
fn remove_file_ok(p: &Path) -> Result<(), String> {
    match std::fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

#[cfg(any(windows, target_os = "macos"))]
fn run(cmd: &mut Command) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

#[cfg(windows)]
mod windows {
    use super::*;

    pub(super) const PROTOCOL_KEY: &str = r"HKCU\Software\Classes\vibefork";
    pub(super) const PROTOCOL_COMMAND_KEY: &str =
        r"HKCU\Software\Classes\vibefork\shell\open\command";
    pub(super) const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    pub(super) const APP_ID_KEY: &str = r"HKCU\Software\Classes\AppUserModelId\me.vibefork.agent";

    pub(super) fn reg(args: &[&str]) -> Result<(), String> {
        run(Command::new("reg").args(args)).map(|_| ())
    }

    pub(super) fn missing_is_ok(e: String) -> Result<(), String> {
        if e.is_empty() || e.to_ascii_lowercase().contains("unable to find") {
            Ok(())
        } else {
            Err(e)
        }
    }

    pub(super) fn query(key: &str, value: Option<&str>) -> Option<String> {
        let mut args = vec!["query", key];
        match value {
            Some(v) => args.extend(["/v", v]),
            None => args.push("/ve"),
        }
        let out = run(Command::new("reg").args(&args)).ok()?;
        out.lines()
            .find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_owned()))
    }

    pub(super) fn register_protocol(exe: &Path) -> Result<(), String> {
        let command = format!("\"{}\" handle \"%1\"", exe.display());
        reg(&["add", PROTOCOL_KEY, "/ve", "/d", "URL:Vibefork", "/f"])?;
        reg(&["add", PROTOCOL_KEY, "/v", "URL Protocol", "/d", "", "/f"])?;
        reg(&["add", PROTOCOL_COMMAND_KEY, "/ve", "/d", &command, "/f"])
    }

    /// The user `Path`, read and written through .NET (keeps `REG_EXPAND_SZ`, notifies Explorer).
    fn user_path() -> Result<String, String> {
        run(Command::new("powershell").args([
            "-NoProfile",
            "-Command",
            "[Environment]::GetEnvironmentVariable('Path','User')",
        ]))
        .map(|s| s.trim().to_owned())
    }

    fn set_user_path(value: &str) -> Result<(), String> {
        run(Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "[Environment]::SetEnvironmentVariable('Path', $env:VF_NEW_PATH, 'User')",
            ])
            .env("VF_NEW_PATH", value))
        .map(|_| ())
    }

    pub(super) fn add_to_path(dir: &Path) -> Result<(), String> {
        let d = dir.display().to_string();
        let current = user_path()?;
        if current.split(';').any(|p| p.eq_ignore_ascii_case(&d)) {
            return Ok(());
        }
        let new = if current.is_empty() {
            d
        } else {
            format!("{current};{d}")
        };
        set_user_path(&new)
    }

    pub(super) fn remove_from_path(dir: &Path) -> Result<(), String> {
        let d = dir.display().to_string();
        let current = user_path()?;
        let kept: Vec<&str> = current
            .split(';')
            .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case(&d))
            .collect();
        if kept.len() == current.split(';').filter(|p| !p.is_empty()).count() {
            return Ok(());
        }
        set_user_path(&kept.join(";"))
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    pub(super) fn desktop_file() -> PathBuf {
        home().join(".local/share/applications/vibefork.desktop")
    }

    pub(super) fn autostart_file() -> PathBuf {
        home().join(".config/autostart/vibefork.desktop")
    }

    pub(super) fn register_protocol(exe: &Path) -> Result<(), String> {
        let f = desktop_file();
        if let Some(p) = f.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let body = format!(
            "[Desktop Entry]\nType=Application\nName=Vibefork\nExec=\"{}\" handle %u\nNoDisplay=true\nMimeType=x-scheme-handler/vibefork;\n",
            exe.display()
        );
        std::fs::write(&f, body).map_err(|e| e.to_string())?;
        // Without `xdg-mime` (no desktop, e.g. a server) the `.desktop` file is all there is.
        match Command::new("xdg-mime")
            .args(["default", "vibefork.desktop", "x-scheme-handler/vibefork"])
            .output()
        {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
            Ok(o) if o.status.success() => Ok(()),
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_owned()),
        }
    }

    pub(super) fn autostart(program: &Path, args: &[&str]) -> Result<(), String> {
        let f = autostart_file();
        if let Some(p) = f.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let body = format!(
            "[Desktop Entry]\nType=Application\nName=Vibefork\nExec=\"{}\" {}\nX-GNOME-Autostart-enabled=true\n",
            program.display(),
            args.join(" ")
        );
        std::fs::write(f, body).map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub(super) fn launch_agent() -> PathBuf {
        home().join("Library/LaunchAgents/me.vibefork.agent.plist")
    }

    /// `Info.plist` of the bundle: the URL scheme is declared here and picked up by Launch Services.
    pub(super) fn write_bundle(macos_dir: &Path) -> Result<(), String> {
        let contents = macos_dir.parent().ok_or("bad bundle path")?;
        let plist = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>CFBundleIdentifier</key><string>me.vibefork.agent</string>\n<key>CFBundleName</key><string>Vibefork</string>\n<key>CFBundleExecutable</key><string>vibefork-tray</string>\n<key>LSUIElement</key><true/>\n<key>LSMinimumSystemVersion</key><string>10.15</string>\n<key>CFBundleURLTypes</key><array><dict><key>CFBundleURLName</key><string>vibefork</string><key>CFBundleURLSchemes</key><array><string>vibefork</string></array></dict></array>\n</dict></plist>\n";
        std::fs::write(contents.join("Info.plist"), plist).map_err(|e| e.to_string())
    }

    pub(super) fn register_protocol(_exe: &Path) -> Result<(), String> {
        let app = install_dir()
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .ok_or("bad bundle path")?;
        let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
        run(Command::new(lsregister).args(["-f", &app.display().to_string()])).map(|_| ())
    }

    pub(super) fn autostart(program: &Path, args: &[&str]) -> Result<(), String> {
        let f = launch_agent();
        if let Some(p) = f.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let items: String = std::iter::once(program.display().to_string())
            .chain(args.iter().map(|a| (*a).to_owned()))
            .map(|a| format!("<string>{a}</string>"))
            .collect();
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>me.vibefork.agent</string><key>ProgramArguments</key><array>{items}</array><key>RunAtLoad</key><true/></dict></plist>\n"
        );
        std::fs::write(f, body).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_folders() {
        assert!(install_dir().is_absolute() || std::env::var_os("VF_INSTALL_DIR").is_some());
        assert!(exe_name("vibefork").starts_with("vibefork"));
    }

    #[test]
    fn copy_and_remove_binaries_in_a_temp_folder() {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "vf-install-{}-{nanos}-{counter}",
            std::process::id()
        ));
        let (src, dst) = (base.join("src"), base.join("dst"));
        assert!(std::fs::create_dir_all(&src).is_ok());
        assert!(copy_binaries(&src, &dst).is_err(), "nothing to install");
        assert!(std::fs::write(src.join(exe_name("vibefork")), "v1").is_ok());
        assert!(copy_binaries(&src, &dst).is_ok());
        assert!(std::fs::write(src.join(exe_name("vibefork")), "v2").is_ok());
        assert!(
            copy_binaries(&src, &dst).is_ok(),
            "replaces an installed binary"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join(exe_name("vibefork")))
                .ok()
                .as_deref(),
            Some("v2")
        );
        assert!(remove_binaries(&dst).is_ok());
        assert!(!dst.join(exe_name("vibefork")).exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
