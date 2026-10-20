//! The background agent process: pid file, start detached, stop, open a URL in the browser.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Pid file of the running agent.
pub fn pid_file(state_dir: &Path) -> PathBuf {
    state_dir.join("agent.pid")
}

/// Log file of a detached agent.
pub fn log_file(state_dir: &Path) -> PathBuf {
    state_dir.join("logs").join("agent.log")
}

/// Records this process as the running agent.
pub fn write_pid(state_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(state_dir)?;
    std::fs::write(pid_file(state_dir), std::process::id().to_string())
}

/// Removes the pid file if it is ours.
pub fn clear_pid(state_dir: &Path) {
    if read_pid(state_dir) == Some(std::process::id()) {
        let _ = std::fs::remove_file(pid_file(state_dir));
    }
}

/// The recorded pid.
pub fn read_pid(state_dir: &Path) -> Option<u32> {
    std::fs::read_to_string(pid_file(state_dir))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// The recorded pid if that process is alive and is a `vibefork` binary.
pub fn running_pid(state_dir: &Path) -> Option<u32> {
    let pid = read_pid(state_dir)?;
    let mut sys = sysinfo::System::new();
    let p = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[p]), true);
    let proc_ = sys.process(p)?;
    proc_
        .name()
        .to_string_lossy()
        .to_ascii_lowercase()
        .starts_with("vibefork")
        .then_some(pid)
}

/// Stops the recorded agent; `false` when none was running.
pub fn stop(state_dir: &Path) -> bool {
    let Some(pid) = running_pid(state_dir) else {
        let _ = std::fs::remove_file(pid_file(state_dir));
        return false;
    };
    let mut sys = sysinfo::System::new();
    let p = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[p]), true);
    let killed = sys.process(p).is_some_and(|proc_| proc_.kill());
    if killed {
        let _ = std::fs::remove_file(pid_file(state_dir));
    }
    killed
}

/// Starts `<exe> serve` in the background, output to [`log_file`].
pub fn start_detached(exe: &Path, state_dir: &Path) -> std::io::Result<u32> {
    let log = log_file(state_dir);
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let err = out.try_clone()?;
    let mut cmd = Command::new(exe);
    cmd.arg("serve")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // Otherwise the agent inherits this process's stdout / stderr, and a caller that reads them
        // (a script, a test runner) waits until the agent exits.
        stop_inheriting_std_handles();
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }
    Ok(cmd.spawn()?.id())
}

/// Marks this process's standard handles as not inheritable.
#[cfg(windows)]
#[allow(unsafe_code)]
fn stop_inheriting_std_handles() {
    use windows_sys::Win32::Foundation::{
        HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: `GetStdHandle` has no preconditions. `SetHandleInformation` only changes the
        // inherit flag of a handle this process owns; null and invalid handles are skipped, and a
        // failure leaves the handle as it was.
        unsafe {
            let h = GetStdHandle(id);
            if !h.is_null() && h != INVALID_HANDLE_VALUE {
                SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

/// Opens a URL in the default browser.
pub fn open_url(url: &str) -> std::io::Result<()> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(url);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("vf-pid-{}", std::process::id()));
        assert!(write_pid(&dir).is_ok());
        assert_eq!(read_pid(&dir), Some(std::process::id()));
        // This test binary is not named `vibefork…`: not taken for the agent.
        assert_eq!(running_pid(&dir), None);
        clear_pid(&dir);
        assert_eq!(read_pid(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
