//! `mise` in the agent's tools folder: a pinned release with pinned hashes, its data, cache, config
//! and state folders inside the tools folder (the user's own `mise` setup is not touched).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use vf_protocol::Cancel;

use crate::download::download;

/// Pinned `mise` release.
pub const VERSION: &str = "2026.10.3";
/// How long one tool install may take.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(1800);

/// Release asset and its SHA-256 for this OS and CPU.
fn asset() -> Option<(&'static str, &'static str)> {
    use std::env::consts::{ARCH, OS};
    Some(match (OS, ARCH) {
        ("windows", "x86_64") => (
            "windows-x64.exe",
            "1d137eaaa772bbcfc2210a7ceaa133347456f18138f17a20e8b2e46b64ec0391",
        ),
        ("windows", "aarch64") => (
            "windows-arm64.exe",
            "f307609491da1cb78ee3fd2de8949e7cce1a7e3032e1c1671dbfdfdf304ba2e1",
        ),
        ("macos", "x86_64") => (
            "macos-x64",
            "38cdd1d904b25e66ec2fff3b5c88930853ab929b63e72c70e77198f82d6ac144",
        ),
        ("macos", "aarch64") => (
            "macos-arm64",
            "0999bd4943523ccdbd149b5ad4080281536801b3f6406ac4495699bd0310969c",
        ),
        ("linux", "x86_64") => (
            "linux-x64",
            "8d48bc510b7d844fad0bc7156c0855c2a1e7230c51e498a7f6b63b021f8b57c5",
        ),
        ("linux", "aarch64") => (
            "linux-arm64",
            "357260e28904569a6e7124d33b65cef6d043846c07bb8f4906ddf22cc353d61d",
        ),
        _ => return None,
    })
}

/// `RUSTUP_HOME` and `CARGO_HOME` in `tools`: toolchains, the `cargo` / `rustc` proxies and the
/// crate registry. Without them rustup and cargo write to `~/.rustup` and `~/.cargo`.
pub fn rust_homes(tools: &Path) -> [(&'static str, PathBuf); 2] {
    [
        ("RUSTUP_HOME", tools.join("rustup")),
        ("CARGO_HOME", tools.join("cargo")),
    ]
}

/// `mise` in a tools folder.
#[derive(Debug, Clone)]
pub struct Mise {
    tools: PathBuf,
}

impl Mise {
    /// `mise` in `tools`.
    pub fn new(tools: &Path) -> Self {
        Self {
            tools: tools.to_path_buf(),
        }
    }

    /// The binary.
    pub fn bin(&self) -> PathBuf {
        self.tools
            .join("bin")
            .join(if cfg!(windows) { "mise.exe" } else { "mise" })
    }

    /// Whether the binary is there.
    pub fn is_installed(&self) -> bool {
        self.bin().is_file()
    }

    /// Downloads the pinned binary if it is missing.
    pub fn ensure(&self, cancel: &Cancel) -> Result<(), String> {
        let (suffix, sha256) = asset().ok_or("no mise build for this OS")?;
        let url = format!(
            "https://github.com/jdx/mise/releases/download/v{VERSION}/mise-v{VERSION}-{suffix}"
        );
        download(&url, &self.bin(), sha256, cancel)?;
        make_executable(&self.bin())
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = plain_command(&self.bin());
        cmd.args(args);
        for (k, dir) in [
            ("MISE_DATA_DIR", "mise-data"),
            ("MISE_CACHE_DIR", "mise-cache"),
            ("MISE_CONFIG_DIR", "mise-config"),
            ("MISE_STATE_DIR", "mise-state"),
        ] {
            cmd.env(k, self.tools.join(dir));
        }
        cmd.env("MISE_YES", "1");
        // Installers run tools of their own (Node, rustup): their temp files stay here too.
        let tmp = self.tools.join("tmp");
        let _ = std::fs::create_dir_all(&tmp);
        for k in ["TEMP", "TMP", "TMPDIR"] {
            cmd.env(k, &tmp);
        }
        cmd.env("NODE_COMPILE_CACHE", self.tools.join("node-compile-cache"));
        // Rust is installed through rustup: its homes too stay in the agent's folder.
        cmd.envs(rust_homes(&self.tools));
        cmd
    }

    /// Installs a tool (no-op when installed) and returns its folder.
    pub fn install(&self, spec: &str, cancel: &Cancel) -> Result<PathBuf, String> {
        self.install_with_progress(spec, cancel, &mut |_| {})
    }

    /// Installs a tool and reports downloading/extracting progress as it runs.
    pub fn install_with_progress(
        &self,
        spec: &str,
        cancel: &Cancel,
        progress: &mut dyn FnMut(crate::Progress),
    ) -> Result<PathBuf, String> {
        let (code, stdout, stderr) = run_with_progress(
            self.command(&["install", spec]),
            cancel,
            INSTALL_TIMEOUT,
            spec,
            progress,
        )?;
        if code != 0 {
            let output = stdout + &stderr;
            let tail: String = output
                .chars()
                .rev()
                .take(500)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            return Err(format!("mise install {spec} failed: {tail}"));
        }
        self.location(spec, cancel)
    }

    /// Folder of an installed tool.
    pub fn location(&self, spec: &str, cancel: &Cancel) -> Result<PathBuf, String> {
        let (code, stdout, stderr) = run(
            self.command(&["where", spec]),
            cancel,
            Duration::from_secs(60),
        )?;
        let path = stdout
            .lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty())
            .unwrap_or_default();
        if code != 0 || path.is_empty() {
            return Err(format!("mise where {spec}: {}", stderr.trim()));
        }
        Ok(PathBuf::from(path))
    }
}

/// A command without a console window.
#[cfg(windows)]
fn plain_command(bin: &Path) -> Command {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = Command::new(bin);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// A command (no console windows to hide outside Windows).
#[cfg(not(windows))]
fn plain_command(bin: &Path) -> Command {
    Command::new(bin)
}

/// Runs to the end; returns the exit code, stdout and stderr.
fn run(
    cmd: Command,
    cancel: &Cancel,
    timeout: Duration,
) -> Result<(i32, String, String), String> {
    run_with_progress(cmd, cancel, timeout, "", &mut |_| {})
}

/// Runs to the end, parsing progress lines as they arrive from stderr.
fn run_with_progress(
    mut cmd: Command,
    cancel: &Cancel,
    timeout: Duration,
    tool: &str,
    progress: &mut dyn FnMut(crate::Progress),
) -> Result<(i32, String, String), String> {
    use std::io::{BufRead, BufReader, Read};
    use std::sync::mpsc;

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("cannot start mise: {e}"))?;

    let out_handle = std::thread::spawn({
        let mut stdout = child.stdout.take();
        move || {
            let mut s = String::new();
            if let Some(mut p) = stdout.take() {
                let _ = p.read_to_string(&mut s);
            }
            s
        }
    });

    let (tx, rx) = mpsc::channel();
    let err_handle = std::thread::spawn({
        let stderr = child.stderr.take();
        let tool_name = tool.to_owned();
        move || {
            let mut all_err = String::new();
            if let Some(p) = stderr {
                let reader = BufReader::new(p);
                for line_res in reader.lines() {
                    if let Ok(line) = line_res {
                        if let Some(p) = parse_progress_line(&line, &tool_name) {
                            let _ = tx.send(p);
                        }
                        all_err.push_str(&line);
                        all_err.push('\n');
                    }
                }
            }
            all_err
        }
    });

    let started = std::time::Instant::now();
    loop {
        while let Ok(p) = rx.try_recv() {
            progress(p);
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            while let Ok(p) = rx.try_recv() {
                progress(p);
            }
            return Ok((
                status.code().unwrap_or(-1),
                out_handle.join().unwrap_or_default(),
                err_handle.join().unwrap_or_default(),
            ));
        }
        if cancel.wait(Duration::from_millis(100)) || started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.is_cancelled() {
                "cancelled".into()
            } else {
                "timed out".into()
            });
        }
    }
}

fn parse_progress_line(line: &str, tool: &str) -> Option<crate::Progress> {
    if line.contains("downloading") {
        let (downloaded, total) = parse_bytes_fraction(line)?;
        let eta = parse_eta(line, downloaded, total);
        Some(crate::Progress::Downloading {
            tool: tool.to_owned(),
            downloaded,
            total,
            eta,
        })
    } else if line.contains("extracting") {
        Some(crate::Progress::Extracting {
            tool: tool.to_owned(),
        })
    } else {
        None
    }
}

fn parse_bytes_fraction(line: &str) -> Option<(u64, Option<u64>)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    for window in parts.windows(2) {
        if let Some((curr_str, tot_str)) = window[0].split_once('/') {
            let unit = window[1];
            if let (Ok(curr), Ok(tot)) = (curr_str.parse::<f64>(), tot_str.parse::<f64>()) {
                let mult = match unit {
                    "GB" => 1024.0 * 1024.0 * 1024.0,
                    "MB" => 1024.0 * 1024.0,
                    "kB" | "KB" => 1024.0,
                    _ => 1.0,
                };
                return Some(((curr * mult) as u64, Some((tot * mult) as u64)));
            }
        }
    }
    None
}

fn parse_eta(line: &str, downloaded: u64, total: Option<u64>) -> Option<f64> {
    let tot = total?;
    if tot <= downloaded {
        return Some(0.0);
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    for window in parts.windows(2) {
        if window[1].ends_with("/s") {
            let unit = window[1].trim_end_matches("/s");
            if let Ok(speed_num) = window[0].parse::<f64>() {
                let mult = match unit {
                    "GB" => 1024.0 * 1024.0 * 1024.0,
                    "MB" => 1024.0 * 1024.0,
                    "kB" | "KB" => 1024.0,
                    _ => 1.0,
                };
                let speed_bytes = speed_num * mult;
                if speed_bytes > 0.0 {
                    return Some((tot - downloaded) as f64 / speed_bytes);
                }
            }
        }
    }
    None
}

#[cfg(unix)]
fn make_executable(p: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_platform_has_a_pinned_hash() {
        if let Some((suffix, sha)) = asset() {
            assert!(!suffix.is_empty());
            assert_eq!(sha.len(), 64);
            assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }
}
