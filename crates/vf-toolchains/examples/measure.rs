//! Measures what each tool of the catalog downloads and takes on disk, installing it for real with
//! the pinned `mise` into a clean tools folder, and checks the install progress the agent reports
//! (bytes while downloading, then unpacking, then ready).
//!
//! ```text
//! cargo run -p vf-toolchains --example measure -- <out-dir> [tool …]
//! ```
//!
//! `cargo xtask measure-tools [tool …]` runs it into `target/measure-tools`. Every tool goes into
//! `<out-dir>/<tool>/tools`, emptied first. `mise` keeps its downloads there
//! (`always_keep_download`), so their size is exact; rustup removes its own, so the files seen in
//! its download folder are summed by the largest size each one reached.
//!
//! Prints a Markdown table; exits with a failure when an install fails or its progress shows no
//! downloaded bytes or no unpacking.
#![allow(clippy::print_stdout, clippy::print_stderr)] // a command-line tool reports to the terminal

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use vf_protocol::Cancel;
use vf_toolchains::estimates::catalog;
use vf_toolchains::{Mise, Need, Progress, install};

/// Folders, relative to the tools folder, where installers download, with the deepest level of the
/// downloaded files: `mise` keeps `<tool>/<version>/<file>` (and unpacks Java next to it, deeper),
/// rustup its files in the folder itself.
const DOWNLOADS: [(&str, usize); 2] = [("mise-data/downloads", 2), ("rustup/downloads", 0)];

/// Slack for the state files beside a download that the progress counts and the download does not.
const STATE_FILES: u64 = 4096;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((out, names)) = args.split_first() else {
        eprintln!("usage: measure <out-dir> [tool …]");
        return ExitCode::FAILURE;
    };
    let out = PathBuf::from(out);
    let needs: Vec<Need> = catalog()
        .into_iter()
        .filter(|n| names.is_empty() || names.contains(&n.name))
        .collect();
    if needs.is_empty() {
        eprintln!("no such tool; the catalog has: java gradle maven node python go rust");
        return ExitCode::FAILURE;
    }

    let cancel = Cancel::default();
    let shared = Mise::new(&out.join("mise"));
    if let Err(e) = shared.ensure(&cancel) {
        eprintln!("cannot download mise: {e}");
        return ExitCode::FAILURE;
    }

    println!(
        "{} {}, mise {}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        vf_toolchains::mise::VERSION
    );
    println!();
    println!(
        "| tool | spec | version | download, bytes | installed, bytes | byte reports | last done / total | unpacking | seconds | progress |"
    );
    println!("|---|---|---|---:|---:|---:|---|---|---:|---|");
    let mut ok = true;
    for need in needs {
        match measure(&out, &shared, &need, &cancel) {
            Ok(m) => {
                ok &= m.progress_ok();
                println!("{}", m.row());
            }
            Err(e) => {
                ok = false;
                println!(
                    "| {} | {} | failed: {e} | | | | | | | FAILED |",
                    need.name, need.spec
                );
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// What one install took and reported.
struct Measured {
    need: Need,
    version: String,
    download: u64,
    installed: u64,
    byte_reports: u32,
    last_done: u64,
    last_total: Option<u64>,
    extracting: bool,
    seconds: u64,
}

impl Measured {
    /// The progress had bytes while downloading, never more than was downloaded (the small state
    /// files `mise` keeps beside a download aside), and an unpacking stage.
    fn progress_ok(&self) -> bool {
        self.byte_reports > 0
            && self.last_done <= self.download.saturating_add(STATE_FILES)
            && self.extracting
    }

    fn row(&self) -> String {
        let total = self.last_total.map_or("?".into(), |t| t.to_string());
        format!(
            "| {} | {} | {} | {} | {} | {} | {} / {} | {} | {} | {} |",
            self.need.name,
            self.need.spec,
            self.version,
            self.download,
            self.installed,
            self.byte_reports,
            self.last_done,
            total,
            if self.extracting { "yes" } else { "no" },
            self.seconds,
            if self.progress_ok() { "ok" } else { "FAILED" }
        )
    }
}

/// Installs `need` into an empty tools folder under `out` and measures it.
fn measure(out: &Path, shared: &Mise, need: &Need, cancel: &Cancel) -> Result<Measured, String> {
    let root = out.join(&need.name);
    if root.exists() {
        std::fs::remove_dir_all(&root)
            .map_err(|e| format!("cannot empty {}: {e}", root.display()))?;
    }
    let tools = root.join("tools");
    let mise = Mise::new(&tools);
    let bin_dir = mise
        .bin()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    std::fs::copy(shared.bin(), mise.bin()).map_err(|e| format!("cannot copy mise: {e}"))?;
    let config = tools.join("mise-config");
    std::fs::create_dir_all(&config).map_err(|e| e.to_string())?;
    std::fs::write(
        config.join("config.toml"),
        "[settings]\nalways_keep_download = true\n",
    )
    .map_err(|e| e.to_string())?;

    // Files seen in the download folders, by the largest size each reached.
    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (stop, tools) = (Arc::clone(&stop), tools.clone());
        std::thread::spawn(move || {
            let mut seen: HashMap<PathBuf, u64> = HashMap::new();
            loop {
                for (d, deepest) in DOWNLOADS {
                    largest_files(&tools.join(d), deepest, &mut seen);
                }
                if stop.load(Ordering::Relaxed) {
                    return seen;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        })
    };

    let started = Instant::now();
    let mut events = Vec::new();
    let prepared = install(&tools, vec![need.clone()], cancel, &mut |p| {
        eprintln!("  {:>6} ms  {p:?}", started.elapsed().as_millis());
        events.push(p);
    });
    let seconds = started.elapsed().as_secs();
    stop.store(true, Ordering::Relaxed);
    let seen = sampler.join().unwrap_or_default();

    let mut m = Measured {
        need: need.clone(),
        version: String::new(),
        download: seen.values().sum(),
        installed: 0,
        byte_reports: 0,
        last_done: 0,
        last_total: None,
        extracting: false,
        seconds,
    };
    for p in events {
        match p {
            Progress::Downloading { done, total, .. } => {
                m.byte_reports += 1;
                m.last_done = done;
                m.last_total = total;
            }
            Progress::Extracting { .. } => m.extracting = true,
            Progress::Failed { error, .. } => return Err(error),
            Progress::Installing { .. } | Progress::Ready { .. } => {}
        }
    }
    if prepared.paths.is_empty() {
        return Err("not installed".into());
    }
    m.version = versions(&tools.join("mise-data/installs").join(&need.name));
    if m.version.is_empty() {
        // Rust: `mise` links rustup's toolchain.
        m.version = versions(&tools.join("rustup/toolchains"));
    }
    // The tools folder without `mise` itself, the downloads it was told to keep and temp files.
    let leftover: u64 = ["bin", "mise-data/downloads", "tmp"]
        .iter()
        .map(|d| folder_bytes(&tools.join(d)))
        .sum();
    m.installed = folder_bytes(&tools).saturating_sub(leftover);
    for (d, deepest) in DOWNLOADS {
        for (path, bytes) in files(&tools.join(d), deepest) {
            eprintln!("  download {bytes:>12}  {}", path.display());
        }
    }
    Ok(m)
}

/// Installed versions: the real folders of the tool's install folder (aliases are links or files).
fn versions(dir: &Path) -> String {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().symlink_metadata().is_ok_and(|m| m.is_dir()))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.join(", ")
}

/// Records each file under `dir` by the largest size it reached. A file being downloaded counts as
/// the file it becomes: `mise` writes `.<name>.mise-part` (and its `.mise-part.json` state),
/// rustup `<name>.partial`.
fn largest_files(dir: &Path, deepest: usize, seen: &mut HashMap<PathBuf, u64>) {
    for (path, bytes) in files(dir, deepest) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.ends_with(".mise-part.json") {
            continue;
        }
        let done = match (
            name.strip_suffix(".mise-part"),
            name.strip_suffix(".partial"),
        ) {
            (Some(part), _) => part.strip_prefix('.').unwrap_or(part).to_owned(),
            (None, Some(part)) => part.to_owned(),
            (None, None) => name,
        };
        let largest = seen.entry(path.with_file_name(done)).or_default();
        *largest = (*largest).max(bytes);
    }
}

/// Every file under `dir` down to level `deepest` (`0` — its own files) with its size; links are
/// not followed.
fn files(dir: &Path, deepest: usize) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(meta) = path.symlink_metadata() else {
                continue;
            };
            if meta.is_file() {
                out.push((path, meta.len()));
            } else if meta.is_dir() && depth < deepest {
                stack.push((path, depth + 1));
            }
        }
    }
    out
}

/// Bytes of every file under `dir`.
fn folder_bytes(dir: &Path) -> u64 {
    files(dir, usize::MAX).iter().map(|(_, b)| b).sum()
}
