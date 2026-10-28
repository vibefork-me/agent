//! Progress of one `mise install` read from the tools folder, not from `mise`'s output (without a
//! terminal `mise` prints no progress, and its wording is not an interface).
//!
//! `mise` downloads a tool into `mise-data/downloads/<tool>/<version>/` (as `.<file>.mise-part`,
//! renamed when complete) and unpacks it into a new folder of `mise-data/installs/<tool>/` (a
//! `.tmp…` folder, then the version folder); Java is unpacked next to its archive first
//! (`downloads/java/<version>/jdk-…/`). Before downloading, `mise` already writes
//! `.mise.backend.toml` into the install folder. rustup (Rust) downloads its components into
//! `rustup/downloads/` and unpacks each as it arrives. So:
//! - bytes of the downloaded files (not of folders unpacked next to them) added since the install
//!   started — downloading, `done` bytes;
//! - a file in a new folder of the tool's install folder, or in a new folder next to a downloaded
//!   archive — unpacking (a new file or an empty folder there is not).
//!
//! The expected download size comes from [`crate::estimates`]; the time left goes by the average
//! speed so far ([`vf_core::progress`]). Nothing is reported for a stage that is not seen.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::Progress;
use crate::detect::Need;

/// Least time between two download reports, ms.
const REPORT_EVERY_MS: u64 = 1000;
/// Deepest folder level searched for an unpacked file.
const MAX_DEPTH: usize = 4;

/// Where an install is, as seen in the tools folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    Nothing,
    Downloading,
    Extracting,
}

/// Watches the tools folder during the install of one tool.
#[derive(Debug)]
pub(crate) struct InstallWatch {
    spec: String,
    /// Download folders with the deepest level of their downloaded files.
    downloads: Vec<(PathBuf, usize)>,
    /// `mise`'s download folder of the tool.
    mise_downloads: PathBuf,
    installs: PathBuf,
    base_bytes: u64,
    base_entries: HashSet<OsString>,
    base_unpacked: HashSet<PathBuf>,
    expected: u64,
    seen: Seen,
    last_done: u64,
    last_report_ms: Option<u64>,
}

impl InstallWatch {
    /// Starts watching before `need` is installed into `tools`; `expected` is its download size in
    /// bytes, `0` when not known.
    pub(crate) fn new(tools: &Path, need: &Need, expected: u64) -> Self {
        let data = tools.join("mise-data");
        let mise_downloads = data.join("downloads").join(&need.name);
        // `<tool>/<version>/<file>`; rustup keeps its files in the folder itself.
        let mut downloads = vec![(mise_downloads.clone(), 1)];
        if need.name == "rust" {
            downloads.push((tools.join("rustup").join("downloads"), 0));
        }
        let installs = data.join("installs").join(&need.name);
        Self {
            spec: need.spec.clone(),
            base_bytes: downloaded(&downloads),
            base_entries: entries(&installs),
            base_unpacked: unpacked_next_to_archives(&mise_downloads),
            downloads,
            mise_downloads,
            installs,
            expected,
            seen: Seen::Nothing,
            last_done: 0,
            last_report_ms: None,
        }
    }

    /// Looks at the folders `elapsed_ms` after the install started; the progress to report, if any.
    pub(crate) fn sample(&mut self, elapsed_ms: u64) -> Option<Progress> {
        if self.seen == Seen::Extracting {
            return None;
        }
        let installing = entries(&self.installs)
            .iter()
            .filter(|e| !self.base_entries.contains(*e))
            .any(|e| has_file(&self.installs.join(e), 0));
        if installing
            || unpacked_next_to_archives(&self.mise_downloads)
                .iter()
                .filter(|d| !self.base_unpacked.contains(*d))
                .any(|d| has_file(d, 0))
        {
            self.seen = Seen::Extracting;
            return Some(Progress::Extracting {
                tool: self.spec.clone(),
            });
        }
        let now = downloaded(&self.downloads);
        let done = now.saturating_sub(self.base_bytes);
        let due = self
            .last_report_ms
            .is_none_or(|at| elapsed_ms.saturating_sub(at) >= REPORT_EVERY_MS);
        if done <= self.last_done || !due {
            return None;
        }
        self.seen = Seen::Downloading;
        self.last_done = done;
        self.last_report_ms = Some(elapsed_ms);
        let total = (self.expected > 0).then_some(self.expected.max(done));
        Some(Progress::Downloading {
            tool: self.spec.clone(),
            done,
            total,
            eta: total.and_then(|t| vf_core::progress::eta(done, t, elapsed_ms)),
        })
    }
}

/// Names in a folder (none when it is missing).
fn entries(dir: &Path) -> HashSet<OsString> {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default()
}

/// Whether `dir` is a folder with a file in it, `MAX_DEPTH` levels deep; links are not followed.
fn has_file(dir: &Path, depth: usize) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten()
        .any(|entry| match entry.path().symlink_metadata() {
            Ok(meta) if meta.is_file() => true,
            Ok(meta) if meta.is_dir() && depth < MAX_DEPTH => has_file(&entry.path(), depth + 1),
            _ => false,
        })
}

/// Bytes of the downloaded files in `downloads`.
fn downloaded(downloads: &[(PathBuf, usize)]) -> u64 {
    downloads
        .iter()
        .map(|(dir, deepest)| folder_bytes(dir, 0, *deepest))
        .fold(0, u64::saturating_add)
}

/// Bytes of the files in a folder down to level `deepest` (`0` — the folder's own files); links
/// are not followed.
fn folder_bytes(dir: &Path, depth: usize, deepest: usize) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut sum = 0u64;
    for entry in rd.flatten() {
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };
        if meta.is_file() {
            sum = sum.saturating_add(meta.len());
        } else if meta.is_dir() && depth < deepest {
            sum = sum.saturating_add(folder_bytes(&entry.path(), depth + 1, deepest));
        }
    }
    sum
}

/// Folders next to the archives of `mise`'s download folder of a tool (`<version>/<folder>`).
fn unpacked_next_to_archives(dir: &Path) -> HashSet<PathBuf> {
    let subfolders = |d: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(d)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.symlink_metadata().is_ok_and(|m| m.is_dir()))
                    .collect()
            })
            .unwrap_or_default()
    };
    subfolders(dir)
        .iter()
        .flat_map(|version| subfolders(version))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vf-watch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn need(name: &str, spec: &str) -> Need {
        Need {
            name: name.into(),
            spec: spec.into(),
        }
    }

    #[test]
    fn a_download_then_an_unpack_are_reported() {
        let tools = tools("stages");
        let downloads = tools.join("mise-data/downloads/node/24.0.0");
        let installs = tools.join("mise-data/installs/node");
        std::fs::create_dir_all(&downloads).unwrap();
        // An older version stays as it was.
        std::fs::create_dir_all(installs.join("22.0.0")).unwrap();
        std::fs::write(downloads.join("old.zip"), vec![0u8; 500]).unwrap();

        let mut watch = InstallWatch::new(&tools, &need("node", "node@lts"), 4000);
        assert_eq!(watch.sample(100), None);
        // What `mise` writes before it downloads is not unpacking.
        std::fs::write(installs.join(".mise.backend.toml"), "short = \"node\"").unwrap();
        std::fs::create_dir_all(installs.join("24.0.0")).unwrap();
        assert_eq!(watch.sample(500), None);

        std::fs::write(downloads.join(".node.zip.mise-part"), vec![0u8; 1000]).unwrap();
        assert_eq!(
            watch.sample(2000),
            Some(Progress::Downloading {
                tool: "node@lts".into(),
                done: 1000,
                total: Some(4000),
                // 1000 bytes in 2 s: 3000 left at 500 B/s.
                eta: Some(6),
            })
        );
        // Grown, but reported less than a second ago.
        std::fs::write(downloads.join(".node.zip.mise-part"), vec![0u8; 2000]).unwrap();
        assert_eq!(watch.sample(2500), None);
        // Larger than expected: the total follows.
        std::fs::write(downloads.join(".node.zip.mise-part"), vec![0u8; 5000]).unwrap();
        assert_eq!(
            watch.sample(3000),
            Some(Progress::Downloading {
                tool: "node@lts".into(),
                done: 5000,
                total: Some(5000),
                eta: Some(0),
            })
        );

        // Unpacking into a temporary folder.
        let unpacked = installs.join(".tmpAbC123").join("node-v24.0.0");
        std::fs::create_dir_all(&unpacked).unwrap();
        std::fs::write(unpacked.join("node.exe"), "").unwrap();
        assert_eq!(
            watch.sample(4000),
            Some(Progress::Extracting {
                tool: "node@lts".into()
            })
        );
        assert_eq!(watch.sample(5000), None);
        let _ = std::fs::remove_dir_all(&tools);
    }

    #[test]
    fn an_archive_unpacked_next_to_itself_is_unpacking_not_download() {
        let tools = tools("java");
        let version = tools.join("mise-data/downloads/java/temurin-21.0.0");
        let mut watch = InstallWatch::new(&tools, &need("java", "java@temurin-21"), 3000);
        std::fs::create_dir_all(&version).unwrap();
        std::fs::write(version.join("jdk.tar.gz"), vec![0u8; 3000]).unwrap();
        assert_eq!(
            watch.sample(1000),
            Some(Progress::Downloading {
                tool: "java@temurin-21".into(),
                done: 3000,
                total: Some(3000),
                eta: Some(0),
            })
        );
        let jdk = version.join("jdk-21.0.0").join("bin");
        std::fs::create_dir_all(&jdk).unwrap();
        std::fs::write(jdk.join("java"), vec![0u8; 9000]).unwrap();
        assert_eq!(
            watch.sample(3000),
            Some(Progress::Extracting {
                tool: "java@temurin-21".into()
            })
        );
        let _ = std::fs::remove_dir_all(&tools);
    }

    #[test]
    fn an_unknown_download_size_has_bytes_but_no_eta() {
        let tools = tools("unknown");
        let mut watch = InstallWatch::new(&tools, &need("rust", "rust@stable"), 0);
        let downloads = tools.join("rustup/downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::write(downloads.join("rustc.tar.xz"), vec![0u8; 700]).unwrap();
        assert_eq!(
            watch.sample(1000),
            Some(Progress::Downloading {
                tool: "rust@stable".into(),
                done: 700,
                total: None,
                eta: None,
            })
        );
        let _ = std::fs::remove_dir_all(&tools);
    }
}
