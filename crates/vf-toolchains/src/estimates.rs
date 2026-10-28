//! What the tools of a project take on disk and download, before anything is installed.
//!
//! Sizes are measured by a real install with the pinned `mise` ([`crate::mise::VERSION`]) of each
//! spec of [`catalog`] into an empty tools folder: `cargo xtask measure-tools` (the `measure`
//! example of this crate; the `measure-tools` workflow runs it on Linux, macOS and Windows).
//! - Installed: what the tools folder grows by, without `mise` itself, the downloads and temp
//!   files (for Rust: rustup's toolchain with its default components and the cargo home).
//! - Downloaded: the files `mise` (and, for Rust, rustup) downloads.
//!
//! Measured on 2026-10-28 with `mise` 2026.10.3 (versions: Java Temurin 21.0.12 on Windows and
//! 21.0.13 elsewhere, Gradle 8.14.6, Maven 3.10.0, Node 24.22.1, Python 3.12.16, Go 1.27.3, Rust
//! 1.99.1). Windows x64, Linux x64 and macOS arm64 are measured; another CPU of
//! the same OS takes those sizes as estimates ([`ToolEstimate::measured`] is `false`).

use std::path::Path;

use vf_protocol::http::{EnvironmentResponse, ToolEstimate};

use crate::detect::{Need, detect};

/// Disk space and download of one tool, bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Disk space once installed.
    pub installed: u64,
    /// What is downloaded.
    pub download: u64,
}

/// A kind of machine sizes were measured on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Windows, x86-64.
    WindowsX64,
    /// Linux, x86-64.
    LinuxX64,
    /// macOS, Apple silicon.
    MacosArm64,
}

/// Measured sizes: tool, installed bytes, downloaded bytes.
type Table = [(&'static str, u64, u64); 7];

/// `cargo xtask measure-tools` on Windows 11 x64.
const WINDOWS_X64: Table = [
    ("java", 344_346_496, 205_073_461),
    ("gradle", 154_702_400, 139_420_601),
    ("maven", 11_790_756, 9_980_013),
    ("node", 107_637_207, 37_751_580),
    ("python", 63_354_589, 22_014_416),
    ("go", 247_167_630, 79_010_309),
    ("rust", 1_420_125_724, 141_317_500),
];

/// `cargo xtask measure-tools` on Linux x64 (glibc).
const LINUX_X64: Table = [
    ("java", 362_606_125, 207_858_352),
    ("gradle", 154_700_890, 139_420_601),
    ("maven", 11_786_535, 9_980_013),
    ("node", 202_041_386, 58_293_990),
    ("python", 116_334_446, 34_505_639),
    ("go", 244_143_779, 70_588_678),
    ("rust", 1_404_079_583, 158_629_980),
];

/// The `measure-tools` workflow on `macos-latest` (Apple silicon).
const MACOS_ARM64: Table = [
    ("java", 350_900_662, 199_754_598),
    ("gradle", 154_700_890, 139_420_601),
    ("maven", 11_786_535, 9_980_013),
    ("node", 197_706_894, 53_066_317),
    ("python", 83_131_077, 25_035_343),
    ("go", 240_009_243, 68_196_737),
    ("rust", 1_236_928_298, 119_034_004),
];

/// The sizes measured on `platform` for a tool by its name (`java`, `node`, …); `None` for a tool
/// without a measurement.
pub fn size_on(platform: Platform, name: &str) -> Option<Size> {
    let table = match platform {
        Platform::WindowsX64 => &WINDOWS_X64,
        Platform::LinuxX64 => &LINUX_X64,
        Platform::MacosArm64 => &MACOS_ARM64,
    };
    table
        .iter()
        .find(|(tool, ..)| *tool == name)
        .map(|&(_, installed, download)| Size {
            installed,
            download,
        })
}

/// The measured platform closest to this machine (its OS), and whether it is this very kind of
/// machine.
pub fn this_platform() -> (Platform, bool) {
    use std::env::consts::{ARCH, OS};
    match (OS, ARCH) {
        ("windows", arch) => (Platform::WindowsX64, arch == "x86_64"),
        ("macos", arch) => (Platform::MacosArm64, arch == "aarch64"),
        (_, arch) => (Platform::LinuxX64, OS == "linux" && arch == "x86_64"),
    }
}

/// Sizes of a tool by its name on this machine ([`this_platform`]).
pub fn size(name: &str) -> Option<Size> {
    size_on(this_platform().0, name)
}

/// Whether the sizes of [`size`] were measured on this kind of machine.
pub fn measured_here() -> bool {
    this_platform().1
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
        let (java, maven) = (size("java").unwrap(), size("maven").unwrap());
        assert_eq!(env.total_bytes, java.installed + maven.installed);
        assert_eq!(env.total_download_bytes, java.download + maven.download);
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
        let python_size = size("python").unwrap();
        assert_eq!(env.total_bytes, python_size.installed);
        assert_eq!(env.total_download_bytes, python_size.download);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_platform_has_a_size_for_every_tool_of_the_catalog() {
        for platform in [
            Platform::WindowsX64,
            Platform::LinuxX64,
            Platform::MacosArm64,
        ] {
            for need in catalog() {
                let s = size_on(platform, &need.name).unwrap();
                assert!(
                    s.download > 0 && s.installed > 0,
                    "{platform:?} {}",
                    need.name
                );
            }
        }
        assert_eq!(size("deno"), None);
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
