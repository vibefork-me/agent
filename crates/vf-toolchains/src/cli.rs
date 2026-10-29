//! The official AI CLIs (Claude Code, Codex, GitHub Copilot), installed for the user when missing: an unmodified
//! release binary of a pinned version, checked against a pinned SHA-256, kept in the agent's tools
//! folder. Nothing is added to the user's shell or `PATH`.

use std::path::{Path, PathBuf};

use vf_protocol::Cancel;

use crate::download::download;

/// Claude Code release.
pub const CLAUDE_VERSION: &str = "2.1.310";
/// Codex release.
pub const CODEX_VERSION: &str = "0.164.0";
/// GitHub Copilot CLI release.
pub const COPILOT_VERSION: &str = "1.0.105";

/// Where to download a CLI and its hash for this machine.
struct Asset {
    url: String,
    sha256: &'static str,
    /// How the binary is packed.
    archive: Archive,
}

/// Packing of a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Archive {
    /// The binary itself.
    None,
    /// A `.tar.gz` with one binary inside (Codex, Copilot on macOS and Linux).
    TarGz,
    /// A `.zip` with one binary inside (Copilot on Windows).
    Zip,
}

fn claude_asset() -> Option<Asset> {
    use std::env::consts::{ARCH, OS};
    let (platform, sha256) = match (OS, ARCH) {
        ("windows", "x86_64") => (
            "win32-x64",
            "f9a3920634ae3b961ee92c2eb58d0051f5f8852b120983d0a4f57b6a5efeba61",
        ),
        ("windows", "aarch64") => (
            "win32-arm64",
            "91da8a2247a8806162319bcdf3e70c6ffeeb58b568481a9827fbff167a694edb",
        ),
        ("macos", "x86_64") => (
            "darwin-x64",
            "28fdbed5a84ef7d4205b5751c7f021e79151c4e319c4a401e48fe6aae86bbb39",
        ),
        ("macos", "aarch64") => (
            "darwin-arm64",
            "ffb64331bc0ee1b69efb7c2c428bd3ab516bc31b75bf60886b356c97c1c25ffd",
        ),
        ("linux", "x86_64") => (
            "linux-x64",
            "b73676430af12f97316a798b8371f81d640f90a58bdcb8f3ce56a835bb47df56",
        ),
        ("linux", "aarch64") => (
            "linux-arm64",
            "9c64d9a7442436b65fcd7cc02c1aa0051bb5a7041e7788576ed3b5f29c447d54",
        ),
        _ => return None,
    };
    let file = if OS == "windows" {
        "claude.exe"
    } else {
        "claude"
    };
    Some(Asset {
        url: format!(
            "https://downloads.claude.ai/claude-code-releases/{CLAUDE_VERSION}/{platform}/{file}"
        ),
        sha256,
        archive: Archive::None,
    })
}

fn codex_target() -> Option<(&'static str, &'static str)> {
    use std::env::consts::{ARCH, OS};
    Some(match (OS, ARCH) {
        ("windows", "x86_64") => (
            "x86_64-pc-windows-msvc.exe",
            "90cf31818ddd8b29f956ad1df5f99af86c19296186be9a117d07b02b2c9bf0e8",
        ),
        ("windows", "aarch64") => (
            "aarch64-pc-windows-msvc.exe",
            "72242e993435790e7c91c4f8ae7037d9809b185c030ca4e3d4f0e049e47992e1",
        ),
        ("macos", "x86_64") => (
            "x86_64-apple-darwin",
            "de6bbc013c56337f6ddd3f26dfbfd62aad5924cb68cd5ef6430538922bfa0e75",
        ),
        ("macos", "aarch64") => (
            "aarch64-apple-darwin",
            "aa8299c169d80111aac05543ad2621c9bdbedb9bc252c905fe3f5a5baf9a4142",
        ),
        ("linux", "x86_64") => (
            "x86_64-unknown-linux-musl",
            "90dd0b03b28afcb7fa733a4f73b413db2916a47347efd43fb669cc1c774ee8e4",
        ),
        ("linux", "aarch64") => (
            "aarch64-unknown-linux-musl",
            "29a507a2a3fd8f6cd94b96b762d281d299d15d7902dee6132d314b78cf9108eb",
        ),
        _ => return None,
    })
}

fn codex_asset() -> Option<Asset> {
    let (target, sha256) = codex_target()?;
    Some(Asset {
        url: format!(
            "https://github.com/openai/codex/releases/download/rust-v{CODEX_VERSION}/codex-{target}.tar.gz"
        ),
        sha256,
        archive: Archive::TarGz,
    })
}

fn copilot_asset() -> Option<Asset> {
    use std::env::consts::{ARCH, OS};
    let (file, sha256, archive) = match (OS, ARCH) {
        ("windows", "x86_64") => (
            "copilot-win32-x64.zip",
            "34e16e9cf47de0e1534920171d6f39fa246638bddbb6ece820870b382bcc061a",
            Archive::Zip,
        ),
        ("windows", "aarch64") => (
            "copilot-win32-arm64.zip",
            "647876bca9ca6ea77f5a28d002e1d135d1bb899c12bcd3f2eb1e860d6199ed90",
            Archive::Zip,
        ),
        ("macos", "x86_64") => (
            "copilot-darwin-x64.tar.gz",
            "de5684eeb118ad1e54f9a01f4535cf74e121b8226d26473550b01a7565ce54f5",
            Archive::TarGz,
        ),
        ("macos", "aarch64") => (
            "copilot-darwin-arm64.tar.gz",
            "34aacd03fb478d47e4b3cfc5ca045c37278c5d8d86239466521a16ea54ac84aa",
            Archive::TarGz,
        ),
        ("linux", "x86_64") => (
            "copilot-linux-x64.tar.gz",
            "aba1f06f27bc0447656647f76e0a4cf58b4b2a341c75ff04a750982b6aeb6bd2",
            Archive::TarGz,
        ),
        ("linux", "aarch64") => (
            "copilot-linux-arm64.tar.gz",
            "e7b488cc04ac3e2ed800e1bf0aace33ada42dcf167529b3d2f48a61e1535a847",
            Archive::TarGz,
        ),
        _ => return None,
    };
    Some(Asset {
        url: format!(
            "https://github.com/github/copilot-cli/releases/download/v{COPILOT_VERSION}/{file}"
        ),
        sha256,
        archive,
    })
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// Folder and binary of an installed CLI: `<tools>/cli/<id>-<version>/<id>[.exe]`.
fn location(tools: &Path, id: &str) -> Option<PathBuf> {
    let version = match id {
        "claude" => CLAUDE_VERSION,
        "codex" => CODEX_VERSION,
        "copilot" => COPILOT_VERSION,
        _ => return None,
    };
    Some(
        tools
            .join("cli")
            .join(format!("{id}-{version}"))
            .join(exe(id)),
    )
}

/// The CLI the agent installed earlier, if it is there.
pub fn installed(tools: &Path, id: &str) -> Option<PathBuf> {
    location(tools, id).filter(|p| p.is_file())
}

/// Installs the official CLI `id` (`claude`, `codex`, `copilot`) for this machine and returns its binary.
pub fn install(tools: &Path, id: &str, cancel: &Cancel) -> Result<PathBuf, String> {
    let bin = location(tools, id).ok_or("unknown CLI")?;
    if bin.is_file() {
        return Ok(bin);
    }
    let asset = match id {
        "claude" => claude_asset(),
        "codex" => codex_asset(),
        "copilot" => copilot_asset(),
        _ => None,
    }
    .ok_or("no release of this CLI for this OS")?;
    let dir = bin.parent().ok_or("bad tools folder")?;
    match asset.archive {
        Archive::None => download(&asset.url, &bin, asset.sha256, cancel)?,
        Archive::TarGz => {
            let archive = dir.join("release.tar.gz");
            download(&asset.url, &archive, asset.sha256, cancel)?;
            extract_single(&archive, &bin)?;
            let _ = std::fs::remove_file(&archive);
        }
        Archive::Zip => {
            let archive = dir.join("release.zip");
            download(&asset.url, &archive, asset.sha256, cancel)?;
            extract_single_zip(&archive, &bin)?;
            let _ = std::fs::remove_file(&archive);
        }
    }
    make_executable(&bin)?;
    Ok(bin)
}

/// Writes the only regular file of a `.tar.gz` (the release binary) to `dest`.
fn extract_single(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let part = dest.with_extension("part");
    let mut found = false;
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        if found {
            let _ = std::fs::remove_file(&part);
            return Err("unexpected files in the release archive".into());
        }
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        found = true;
    }
    if !found {
        return Err("the release archive is empty".into());
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// Writes the only file of a `.zip` (the release binary) to `dest`.
fn extract_single_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let files: Vec<usize> = (0..zip.len())
        .filter(|&i| zip.by_index(i).is_ok_and(|f| f.is_file()))
        .collect();
    let [only] = files[..] else {
        return Err(if files.is_empty() {
            "the release archive is empty".into()
        } else {
            "unexpected files in the release archive".into()
        });
    };
    let mut entry = zip.by_index(only).map_err(|e| e.to_string())?;
    let part = dest.with_extension("part");
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    if let Err(e) = std::io::copy(&mut entry, &mut out) {
        let _ = std::fs::remove_file(&part);
        return Err(e.to_string());
    }
    drop(out);
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
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
    fn pinned_releases_come_from_the_vendors_with_hashes() {
        for a in [claude_asset(), codex_asset(), copilot_asset()]
            .into_iter()
            .flatten()
        {
            assert!(
                a.url.starts_with("https://downloads.claude.ai/")
                    || a.url
                        .starts_with("https://github.com/openai/codex/releases/download/")
                    || a.url
                        .starts_with("https://github.com/github/copilot-cli/releases/download/"),
                "{}",
                a.url
            );
            assert_eq!(a.sha256.len(), 64);
            assert!(a.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn unknown_cli_is_refused_and_nothing_is_installed_yet() {
        let tools = std::env::temp_dir().join(format!("vf-cli-test-{}", std::process::id()));
        assert!(location(&tools, "rm").is_none());
        assert!(installed(&tools, "claude").is_none());
        assert!(install(&tools, "bash", &Cancel::new()).is_err());
    }

    fn make_tar(path: &Path, names: &[&str]) -> std::io::Result<()> {
        let gz = flate2::write::GzEncoder::new(
            std::fs::File::create(path)?,
            flate2::Compression::fast(),
        );
        let mut b = tar::Builder::new(gz);
        for n in names {
            let mut h = tar::Header::new_gnu();
            h.set_size(3);
            h.set_mode(0o755);
            h.set_cksum();
            b.append_data(&mut h, n, &b"bin"[..])?;
        }
        b.into_inner()?.finish()?;
        Ok(())
    }

    #[test]
    fn a_release_archive_must_hold_exactly_one_file() -> std::io::Result<()> {
        let dir = std::env::temp_dir().join(format!("vf-cli-tar-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let one = dir.join("one.tar.gz");
        let two = dir.join("two.tar.gz");
        make_tar(&one, &["codex-x"])?;
        make_tar(&two, &["a", "b"])?;
        let dest = dir.join("codex");
        assert!(extract_single(&one, &dest).is_ok());
        assert_eq!(std::fs::read(&dest)?, b"bin");
        assert!(extract_single(&two, &dir.join("other")).is_err());
        std::fs::remove_dir_all(&dir)
    }

    fn make_zip(path: &Path, names: &[&str]) -> zip::result::ZipResult<()> {
        use std::io::Write as _;
        let mut w = zip::ZipWriter::new(std::fs::File::create(path)?);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for n in names {
            w.start_file(*n, opts)?;
            w.write_all(b"bin")?;
        }
        w.finish()?;
        Ok(())
    }

    #[test]
    fn a_zip_release_must_hold_exactly_one_file() -> zip::result::ZipResult<()> {
        let dir = std::env::temp_dir().join(format!("vf-cli-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let one = dir.join("one.zip");
        let two = dir.join("two.zip");
        make_zip(&one, &["copilot.exe"])?;
        make_zip(&two, &["a", "b"])?;
        let dest = dir.join("copilot");
        assert!(extract_single_zip(&one, &dest).is_ok());
        assert_eq!(std::fs::read(&dest)?, b"bin");
        assert!(extract_single_zip(&two, &dir.join("other")).is_err());
        assert!(extract_single_zip(&dir.join("missing.zip"), &dir.join("x")).is_err());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
