//! What the tools of a project take on disk and download, before anything is installed.
//!
//! Sizes are measured on Windows x64 with the pinned `mise` ([`crate::mise::VERSION`]) into an
//! agent tools folder; elsewhere they serve as estimates ([`ToolEstimate::measured`] is `false`).
//!
//! - Installed: `du -sb <tools>/mise-data/installs/<tool>` after `mise install <spec>`:
//!   `node@lts` (24.22.1) 107 345 970, `python@3.12` (3.12.15) 63 228 563, `go@1.23` (1.23.12)
//!   243 869 155, `java@temurin-21` (21.0.12+101) 343 823 876, `gradle@8` (8.14.6) 154 630 180,
//!   `maven@3` (3.10.0) 11 784 219 bytes. `rust@stable`: `du -sb <tools>/rustup <tools>/cargo`
//!   (rustup's toolchain with its default components and the cargo home) — see [`RUST`].
//! - Downloaded: `Content-Length` of the files `mise` fetches for those versions:
//!   `node-v24.22.1-win-x64.zip` 37 748 064, `cpython-3.12.15+…-x86_64-pc-windows-msvc-install_only_stripped.tar.gz`
//!   21 990 627, `go1.23.12.windows-amd64.zip` 81 978 626, Temurin JDK 21.0.12+1 x64 Windows zip
//!   205 073 461, `gradle-8.14.6-bin.zip` 139 420 601, `apache-maven-3.10.0-bin.zip` 10 102 082
//!   bytes. Rust's download (rustup's components) is not measured: `0`, unknown.

use std::path::Path;

use vf_protocol::http::{EnvironmentResponse, ToolEstimate};

use crate::detect::{Need, detect};

/// Disk space and download of one tool, bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Disk space once installed.
    pub installed: u64,
    /// What is downloaded; `0` when not known.
    pub download: u64,
}

/// Rust (`rust@stable`): rustup's stable toolchain with its default components and the cargo home.
pub const RUST: Size = Size {
    installed: 1_407_250_000,
    download: 0,
};

/// Sizes of a tool by its name (`java`, `node`, …); `None` for a tool without a measurement.
pub fn size(name: &str) -> Option<Size> {
    let (installed, download) = match name {
        "node" => (107_345_970, 37_748_064),
        "python" => (63_228_563, 21_990_627),
        "go" => (243_869_155, 81_978_626),
        "java" => (343_823_876, 205_073_461),
        "gradle" => (154_630_180, 139_420_601),
        "maven" => (11_784_219, 10_102_082),
        "rust" => return Some(RUST),
        _ => return None,
    };
    Some(Size {
        installed,
        download,
    })
}

/// Whether the sizes of [`size`] were measured on this kind of machine.
pub fn measured_here() -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
}

/// Every tool the agent can install, with the spec it installs when a project does not say.
pub fn catalog() -> Vec<Need> {
    [
        ("java", "java@temurin-21"),
        ("gradle", "gradle@8"),
        ("maven", "maven@3"),
        ("node", "node@lts"),
        ("python", "python@3.12"),
        ("go", "go@latest"),
        ("rust", "rust@stable"),
    ]
    .into_iter()
    .map(|(name, spec)| Need {
        name: name.into(),
        spec: spec.into(),
    })
    .collect()
}

/// Whether `need` is in the tools folder: `mise` keeps each installed version and its aliases
/// (`lts`, `temurin-21`, `3.12`, `stable`) under `mise-data/installs/<tool>/`.
pub fn is_installed(tools: &Path, need: &Need) -> bool {
    let Some((_, version)) = need.spec.split_once('@') else {
        return false;
    };
    if version.is_empty() || version.contains(['/', '\\']) || version.starts_with('.') {
        return false;
    }
    tools
        .join("mise-data")
        .join("installs")
        .join(&need.name)
        .join(version)
        .symlink_metadata()
        .is_ok()
}

/// The tools `needs` and what the missing ones take.
pub fn estimate(tools: &Path, needs: Vec<Need>) -> EnvironmentResponse {
    let measured = measured_here();
    let mut out = EnvironmentResponse {
        tools: Vec::with_capacity(needs.len()),
        total_bytes: 0,
        total_download_bytes: 0,
    };
    for need in needs {
        let installed = is_installed(tools, &need);
        let size = match size(&need.name) {
            Some(s) if !installed => s,
            _ => Size {
                installed: 0,
                download: 0,
            },
        };
        out.total_bytes = out.total_bytes.saturating_add(size.installed);
        out.total_download_bytes = out.total_download_bytes.saturating_add(size.download);
        out.tools.push(ToolEstimate {
            name: need.name,
            spec: need.spec,
            installed,
            bytes: size.installed,
            download_bytes: size.download,
            measured,
        });
    }
    out
}

/// What `dir` needs ([`detect`]) and what the missing tools take.
pub fn estimate_project(dir: &Path, tools: &Path) -> EnvironmentResponse {
    estimate(tools, detect(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vf-estimates-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn project(root: &Path, files: &[&str]) -> std::path::PathBuf {
        let dir = root.join("notes-app");
        std::fs::create_dir_all(&dir).unwrap();
        for f in files {
            std::fs::write(dir.join(f), "").unwrap();
        }
        dir
    }

    #[test]
    fn a_project_without_build_files_needs_nothing() {
        let root = temp("empty");
        let dir = project(&root, &["README.md"]);
        let env = estimate_project(&dir, &root.join("tools"));
        assert!(env.tools.is_empty());
        assert_eq!((env.total_bytes, env.total_download_bytes), (0, 0));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_maven_project_needs_java_and_maven() {
        let root = temp("maven");
        let dir = project(&root, &["pom.xml"]);
        let env = estimate_project(&dir, &root.join("tools"));
        let specs: Vec<&str> = env.tools.iter().map(|t| t.spec.as_str()).collect();
        assert_eq!(specs, ["java@temurin-21", "maven@3"]);
        assert!(env.tools.iter().all(|t| !t.installed));
        assert_eq!(env.total_bytes, 343_823_876 + 11_784_219);
        assert_eq!(env.total_download_bytes, 205_073_461 + 10_102_082);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_installed_tool_takes_nothing() {
        let root = temp("installed");
        let dir = project(&root, &["package.json", "pyproject.toml"]);
        let tools = root.join("tools");
        // As `mise` leaves it: the version folder and an alias file.
        let node = tools.join("mise-data").join("installs").join("node");
        std::fs::create_dir_all(node.join("24.0.0")).unwrap();
        std::fs::write(node.join("lts"), "./24.0.0").unwrap();
        let env = estimate_project(&dir, &tools);
        assert_eq!(env.tools.len(), 2);
        let (node, python) = (&env.tools[0], &env.tools[1]);
        assert_eq!((node.spec.as_str(), node.installed), ("node@lts", true));
        assert_eq!((node.bytes, node.download_bytes), (0, 0));
        assert_eq!(
            (python.spec.as_str(), python.installed),
            ("python@3.12", false)
        );
        assert_eq!(env.total_bytes, 63_228_563);
        assert_eq!(env.total_download_bytes, 21_990_627);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_catalog_has_a_size_for_every_tool() {
        let root = temp("catalog");
        let env = estimate(&root, catalog());
        assert_eq!(env.tools.len(), 7);
        assert!(env.tools.iter().all(|t| t.bytes > 0));
        assert_eq!(
            env.total_bytes,
            env.tools.iter().map(|t| t.bytes).sum::<u64>()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn specs_that_leave_the_tools_folder_are_not_installed() {
        let root = temp("escape");
        for spec in ["node@", "node@../x", "node@..", "node@a/b", "node"] {
            let need = Need {
                name: "node".into(),
                spec: spec.into(),
            };
            assert!(!is_installed(&root, &need), "{spec}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
