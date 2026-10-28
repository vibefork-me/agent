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
/// The error of a `mise` run stopped by its time limit.
pub const TIMED_OUT: &str = "timed out";
/// The error of a `mise` run stopped by the user.
pub const CANCELLED: &str = "cancelled";

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

/// Whether there is a pinned `mise` build for this OS and CPU.
pub fn is_supported() -> bool {
    asset().is_some()
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
        cmd.env("MISE_YES", "1").env("MISE_QUIET", "1");
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
        self.install_with(spec, cancel, &mut || {})
    }

    /// [`Mise::install`] that calls `tick` about every 200 ms while `mise` runs.
    pub fn install_with(
        &self,
        spec: &str,
        cancel: &Cancel,
        tick: &mut dyn FnMut(),
    ) -> Result<PathBuf, String> {
        let (code, stdout, stderr) = run(
            self.command(&["install", spec]),
            cancel,
            INSTALL_TIMEOUT,
            tick,
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
            &mut || {},
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

/// Runs to the end, calling `tick` between checks; returns the exit code, stdout and stderr.
fn run(
    mut cmd: Command,
    cancel: &Cancel,
    timeout: Duration,
    tick: &mut dyn FnMut(),
) -> Result<(i32, String, String), String> {
    use std::io::Read;
    fn reader(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
        std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_string(&mut s);
            }
            s
        })
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("cannot start mise: {e}"))?;
    let out = reader(child.stdout.take());
    let err = reader(child.stderr.take());
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok((
                status.code().unwrap_or(-1),
                out.join().unwrap_or_default(),
                err.join().unwrap_or_default(),
            ));
        }
        if cancel.wait(Duration::from_millis(200)) || started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.is_cancelled() {
                CANCELLED.into()
            } else {
                TIMED_OUT.into()
            });
        }
        tick();
    }
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
