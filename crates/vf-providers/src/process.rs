//! Child processes: find a program, run it with streamed output, stop the whole process tree on
//! cancel or timeout.

use std::io::{BufRead as _, BufReader, Read, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use vf_protocol::Cancel;

/// Which stream a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// How a process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    /// Exit code; `-1` when killed or unknown.
    pub code: i32,
    /// Stopped by [`Cancel`].
    pub cancelled: bool,
    /// Stopped by the timeout.
    pub timed_out: bool,
}

/// Finds a program on `PATH` (with `PATHEXT` on Windows).
pub fn find_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let path =
        std::env::join_paths(std::env::split_paths(&path).chain(user_bin_dirs())).unwrap_or(path);
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter()
            .map(|ext| dir.join(format!("{name}{ext}")))
            .find(|p| p.is_file())
    })
}

/// Folders where per-user installs put programs (macOS, Linux), the ones that exist. An agent
/// started from the desktop or a `vibefork://` link gets only the system `PATH` and finds the
/// user's coding CLIs (and they their tools) here.
pub fn user_bin_dirs() -> Vec<PathBuf> {
    if cfg!(windows) {
        return Vec::new();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = [
        ".local/bin",
        ".claude/local",
        ".npm-global/bin",
        ".bun/bin",
        ".volta/bin",
        ".cargo/bin",
    ]
    .iter()
    .filter_map(|d| home.as_ref().map(|h| h.join(d)))
    .collect();
    dirs.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/home/linuxbrew/.linuxbrew/bin",
        ]
        .map(PathBuf::from),
    );
    dirs.retain(|d| d.is_dir());
    dirs
}

/// Prepares a command: no console window on Windows, its own process group on Unix (so the tree can
/// be stopped).
pub fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    detach(&mut cmd);
    cmd
}

/// Runs `cmd`, writes `stdin` to it, calls `on_line` for every output line on this thread.
pub fn run_streaming(
    mut cmd: Command,
    stdin: Option<&str>,
    cancel: &Cancel,
    timeout: Option<Duration>,
    on_line: &mut dyn FnMut(Stream, String),
) -> std::io::Result<Exit> {
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let text = text.to_owned();
        std::thread::spawn(move || {
            let _ = pipe.write_all(text.as_bytes());
        });
    }
    let (tx, rx) = mpsc::channel::<Option<(Stream, String)>>();
    let mut readers = 0;
    if let Some(out) = child.stdout.take() {
        readers += 1;
        spawn_reader(out, Stream::Stdout, tx.clone());
    }
    if let Some(err) = child.stderr.take() {
        readers += 1;
        spawn_reader(err, Stream::Stderr, tx.clone());
    }
    drop(tx);

    let started = Instant::now();
    let (mut cancelled, mut timed_out) = (false, false);
    while readers > 0 {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Some((stream, line))) => on_line(stream, line),
            Ok(None) => readers -= 1,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !cancelled && !timed_out {
            if cancel.is_cancelled() {
                cancelled = true;
                kill_tree(&mut child);
            } else if timeout.is_some_and(|t| started.elapsed() > t) {
                timed_out = true;
                kill_tree(&mut child);
            }
        }
    }
    let status = child.wait()?;
    Ok(Exit {
        code: if cancelled || timed_out {
            -1
        } else {
            status.code().unwrap_or(-1)
        },
        cancelled,
        timed_out,
    })
}

fn spawn_reader(
    pipe: impl Read + Send + 'static,
    stream: Stream,
    tx: mpsc::Sender<Option<(Stream, String)>>,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf)
                        .trim_end_matches(['\r', '\n'])
                        .to_owned();
                    if tx.send(Some((stream, line))).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = tx.send(None);
    });
}

#[cfg(windows)]
fn detach(cmd: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(unix)]
fn detach(cmd: &mut Command) {
    use std::os::unix::process::CommandExt as _;
    cmd.process_group(0);
}

#[cfg(not(any(windows, unix)))]
fn detach(_: &mut Command) {}

/// Stops a process and everything it started.
pub fn kill_tree(child: &mut Child) {
    let pid = child.id().to_string();
    #[cfg(windows)]
    {
        let mut k = command(Path::new("taskkill"));
        let _ = k
            .args(["/PID", &pid, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        // `--` is required: without it procps `kill` misreads the negative group id and signals
        // the caller's own group.
        let _ = Command::new("kill")
            .args(["-s", "TERM", "--", &format!("-{pid}")])
            .stderr(Stdio::null())
            .status();
    }
    let _ = pid;
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(script: &str) -> Command {
        let mut c = if cfg!(windows) {
            let mut c = command(Path::new("cmd"));
            c.args(["/d", "/c", script]);
            c
        } else {
            let mut c = command(Path::new("/bin/sh"));
            c.args(["-c", script]);
            c
        };
        c.current_dir(std::env::temp_dir());
        c
    }

    #[test]
    fn lines_of_both_streams_and_exit_code() {
        let mut lines = Vec::new();
        let exit = run_streaming(
            shell("echo out && echo err 1>&2 && exit 3"),
            None,
            &Cancel::new(),
            None,
            &mut |s, l| lines.push((s, l.trim().to_owned())),
        );
        assert_eq!(exit.ok().map(|e| e.code), Some(3));
        assert!(lines.contains(&(Stream::Stdout, "out".into())));
        assert!(lines.contains(&(Stream::Stderr, "err".into())));
    }

    #[test]
    fn timeout_stops_the_process() {
        let script = if cfg!(windows) {
            "ping -n 30 127.0.0.1 >nul"
        } else {
            "sleep 30"
        };
        let started = Instant::now();
        let exit = run_streaming(
            shell(script),
            None,
            &Cancel::new(),
            Some(Duration::from_millis(300)),
            &mut |_, _| {},
        );
        assert!(exit.is_ok_and(|e| e.timed_out));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn stdin_is_passed() {
        let script = if cfg!(windows) { "findstr x" } else { "cat" };
        let mut got = String::new();
        let exit = run_streaming(
            shell(script),
            Some("x marks\n"),
            &Cancel::new(),
            Some(Duration::from_secs(10)),
            &mut |_, l| got.push_str(&l),
        );
        assert!(exit.is_ok());
        assert!(got.contains("x marks"), "{got}");
    }
}
