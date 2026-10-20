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

/// Starts `<exe> serve --log-file <log>` in the background; the agent writes its own log to
/// [`log_file`].
pub fn start_detached(exe: &Path, state_dir: &Path) -> std::io::Result<u32> {
    let log = log_file(state_dir);
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    spawn_detached(
        exe,
        &["serve".as_ref(), "--log-file".as_ref(), log.as_os_str()],
    )
}

/// Windows: `CreateProcessW` without handle inheritance. With inheritance (what `Command` does) the
/// agent would keep pipes of the caller open — a script reading the output of `vibefork start`
/// would wait until the agent exits.
#[cfg(windows)]
#[allow(unsafe_code)]
fn spawn_detached(exe: &Path, args: &[&std::ffi::OsStr]) -> std::io::Result<u32> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CreateProcessW, DETACHED_PROCESS,
        PROCESS_INFORMATION, STARTUPINFOW,
    };
    // Every part quoted: paths may have spaces (none of them has quotes).
    let mut line: Vec<u16> = Vec::new();
    for (i, part) in std::iter::once(exe.as_os_str())
        .chain(args.iter().copied())
        .enumerate()
    {
        if i > 0 {
            line.push(u16::from(b' '));
        }
        line.push(u16::from(b'"'));
        line.extend(part.encode_wide());
        line.push(u16::from(b'"'));
    }
    line.push(0);
    // SAFETY: both are plain C structs for which all-zero is a valid value; `cb` is set as required.
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = u32::try_from(std::mem::size_of::<STARTUPINFOW>()).unwrap_or(u32::MAX);
    // SAFETY: as above.
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `line` is a writable, NUL-terminated UTF-16 buffer that outlives the call; the other
    // pointers are null (defaults) or point to the initialized structs above. Handle inheritance is
    // off (`0`).
    let ok = unsafe {
        CreateProcessW(
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
            std::ptr::null(),
            std::ptr::null(),
            &si,
            &mut pi,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the handles were returned by a successful `CreateProcessW` and are closed once.
    unsafe {
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
    }
    Ok(pi.dwProcessId)
}

#[cfg(unix)]
fn spawn_detached(exe: &Path, args: &[&std::ffi::OsStr]) -> std::io::Result<u32> {
    use std::os::unix::process::CommandExt as _;
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    Ok(cmd.spawn()?.id())
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
