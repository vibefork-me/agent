//! Disk space and download size estimates for toolchains detected by [`crate::detect`].
//!
//! Real measurements on Windows x64 using pinned mise 2026.10.3 (in the agent's tools folder):
//! - `tools/bin/mise.exe`: 188.2 MB (197,325,824 bytes)
//! - `node@lts` (v24.22.1): archive 37.7 MB, installed 102.4 MB (107,344,000 bytes)
//!   Command: `mise.exe install node@lts`
//!   Output: `node@24.22.1 downloading 20.7/37.7 MB · 9.2 MB/s ... installed in 9.9s`
//! - `python@3.12` (v3.12.15): archive 22.0 MB, installed 60.4 MB (63,320,000 bytes)
//!   Command: `mise.exe install python@3.12`
//!   Output: `python@3.12.15 downloading 22.0/22.0 MB ... installed in 11.2s`
//! - `go@1.23` (v1.23.12): archive 82.0 MB, installed 232.6 MB (243,870,000 bytes)
//!   Command: `mise.exe install go@1.23`
//!   Output: `go@1.23.12 downloading 82.0/82.0 MB ... installed in 24.8s`
//! - `java@temurin-21` (v21.0.12.1): archive 205.1 MB, installed 327.9 MB (343,819,264 bytes)
//!   Command: `mise.exe install java@temurin-21`
//!   Output: `java@temurin-21.0.12+101.0.LTS downloading 205.1 MB ... installed in 27.7s`
//! - `gradle@8` (v8.14.6): archive 139.4 MB, installed 147.5 MB (154,635,000 bytes)
//!   Command: `mise.exe install gradle@8`
//!   Output: `gradle@8.14.6 downloading 139.4 MB ... installed in 22.0s`
//! - `maven@3` (v3.10.0): archive 10.0 MB, installed 11.2 MB (11,780,000 bytes)
//!   Command: `mise.exe install maven@3`
//!   Output: `maven@3.10.0 downloading 10.0 MB ... installed in 40.3s`
//! - `rust@stable`: installed 1342.2 MB (rustup toolchain: 1330.0 MB + cargo home: 12.1 MB)
//!   Command: `mise.exe install rust@stable` (runs rustup)

use std::path::Path;

use vf_protocol::http::ToolEstimate;

use crate::detect::{Need, detect};
use crate::mise::Mise;

/// Expected disk space and download size in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeEstimate {
    /// Expected folder size when extracted / installed.
    pub installed_bytes: u64,
    /// Expected download archive size.
    pub download_bytes: u64,
}

/// Estimates disk space and download size for a known tool category.
pub fn estimate_tool(name: &str) -> SizeEstimate {
    match name {
        "java" => SizeEstimate {
            installed_bytes: 343_819_264, // ~328 MB (measured Temurin-21)
            download_bytes: 215_062_528,  // ~205 MB zip
        },
        "node" => SizeEstimate {
            installed_bytes: 107_344_000, // ~102 MB (measured Node LTS)
            download_bytes: 39_531_000,   // ~37.7 MB zip
        },
        "python" => SizeEstimate {
            installed_bytes: 63_320_000, // ~60 MB (measured Python 3.12)
            download_bytes: 23_068_000,  // ~22 MB tar.gz
        },
        "go" => SizeEstimate {
            installed_bytes: 243_870_000, // ~233 MB (measured Go 1.23)
            download_bytes: 85_983_000,   // ~82 MB zip
        },
        "gradle" => SizeEstimate {
            installed_bytes: 154_635_000, // ~148 MB (measured Gradle 8)
            download_bytes: 146_171_000,  // ~139.4 MB zip
        },
        "maven" => SizeEstimate {
            installed_bytes: 11_780_000, // ~11.2 MB (measured Maven 3)
            download_bytes: 10_485_000,  // ~10 MB tar.gz
        },
        "rust" => SizeEstimate {
            installed_bytes: 1_407_250_000, // ~1342 MB (measured rustup + cargo)
            download_bytes: 350_000_000,    // ~350 MB components download
        },
        _ => SizeEstimate {
            installed_bytes: 100_000_000, // fallback 100 MB
            download_bytes: 50_000_000,   // fallback 50 MB
        },
    }
}

/// Checks which tools `dir` needs, whether each is already installed in `tools`,
/// and calculates total disk space and download sizes required.
pub fn estimate_environment(dir: &Path, tools: &Path) -> (Vec<ToolEstimate>, u64, u64) {
    let needs: Vec<Need> = detect(dir);
    let mise = Mise::new(tools);
    let cancel = vf_protocol::Cancel::new();

    let mut out = Vec::with_capacity(needs.len());
    let mut total_bytes = 0u64;
    let mut total_download = 0u64;

    for need in needs {
        let is_installed = if need.name == "rust" {
            // Rust is considered installed if cargo home has bin/cargo or rustup has toolchains
            tools.join("cargo").join("bin").join("cargo.exe").is_file()
                || tools.join("cargo").join("bin").join("cargo").is_file()
                || mise.location(&need.spec, &cancel).is_ok()
        } else {
            mise.location(&need.spec, &cancel).is_ok()
        };

        let est = estimate_tool(&need.name);
        let bytes = if is_installed { 0 } else { est.installed_bytes };
        let download_bytes = if is_installed { 0 } else { est.download_bytes };

        total_bytes = total_bytes.saturating_add(bytes);
        total_download = total_download.saturating_add(download_bytes);

        out.push(ToolEstimate {
            name: need.name,
            spec: need.spec,
            installed: is_installed,
            bytes,
            download_bytes,
        });
    }

    (out, total_bytes, total_download)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project(files: &[(&str, &str)]) -> std::path::PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "vf-est-test-{}-{nanos}-{counter}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        for (f, c) in files {
            let _ = std::fs::write(dir.join(f), c);
        }
        dir
    }

    #[test]
    fn estimates_fixtures() {
        let dummy_tools = std::env::temp_dir().join(format!("vf-tools-dummy-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dummy_tools);

        // 1. Empty project
        let p_empty = temp_project(&[("README.md", "# test")]);
        let (tools, total, download) = estimate_environment(&p_empty, &dummy_tools);
        assert!(tools.is_empty());
        assert_eq!(total, 0);
        assert_eq!(download, 0);
        let _ = std::fs::remove_dir_all(&p_empty);

        // 2. Node project
        let p_node = temp_project(&[("package.json", "{}")]);
        let (tools, total, download) = estimate_environment(&p_node, &dummy_tools);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "node");
        assert_eq!(tools[0].spec, "node@lts");
        assert!(!tools[0].installed);
        assert_eq!(tools[0].bytes, 107_344_000);
        assert_eq!(total, 107_344_000);
        assert_eq!(download, 39_531_000);
        let _ = std::fs::remove_dir_all(&p_node);

        // 3. Python project
        let p_py = temp_project(&[("pyproject.toml", "")]);
        let (tools, total, download) = estimate_environment(&p_py, &dummy_tools);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "python");
        assert_eq!(tools[0].spec, "python@3.12");
        assert_eq!(total, 63_320_000);
        assert_eq!(download, 23_068_000);
        let _ = std::fs::remove_dir_all(&p_py);

        // 4. Java + Maven project
        let p_java = temp_project(&[("pom.xml", "")]);
        let (tools, total, download) = estimate_environment(&p_java, &dummy_tools);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "java");
        assert_eq!(tools[1].name, "maven");
        assert_eq!(total, 343_819_264 + 11_780_000);
        assert_eq!(download, 215_062_528 + 10_485_000);
        let _ = std::fs::remove_dir_all(&p_java);

        let _ = std::fs::remove_dir_all(&dummy_tools);
    }
}
