//! Self-updates. A release channel (`<base>/manifest.json` + `manifest.sig`, signed with the release
//! key) lists the binaries by platform (`vibefork-<os>-<arch>[.exe]`) with their SHA-256. A binary
//! is used only when the manifest signature and the file hash match; the new binary must answer
//! `version` before it replaces the old one, and the old one is kept for `update --rollback`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ed25519_dalek::VerifyingKey;

use crate::signing::{
    Manifest, SignatureError, SubkeyCert, check_fresh, verify_file, verify_manifest,
    verify_subkey_cert,
};

/// Errors of an update.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// Network.
    #[error("download failed: {0}")]
    Download(String),
    /// Signature or hash.
    #[error(transparent)]
    Signature(#[from] SignatureError),
    /// No binary for this platform in the release.
    #[error("the release has no build for this platform ({0})")]
    NoBuild(String),
    /// The new binary does not start.
    #[error("the new version does not start: {0}")]
    Broken(String),
    /// Disk.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Release file name of a binary for this platform: `vibefork-windows-x86_64.exe`.
pub fn asset_name(binary: &str) -> String {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    format!(
        "{binary}-{}-{}{ext}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Whether version `a` is newer than `b` (dot-separated numbers; anything after `-` is ignored).
pub fn is_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split('-')
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parse(a), parse(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

/// A verified release manifest.
#[derive(Debug, Clone)]
pub struct Release {
    base_url: String,
    /// The signed manifest.
    pub manifest: Manifest,
}

fn client() -> Result<reqwest::Client, UpdateError> {
    // Release files live on a CDN behind redirects; their hashes are checked against the signed
    // manifest, which itself is fetched without redirects.
    reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| UpdateError::Download(e.to_string()))
}

async fn get(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, UpdateError> {
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| UpdateError::Download(e.to_string()))?;
    if !res.status().is_success() {
        return Err(UpdateError::Download(format!(
            "{url}: HTTP {}",
            res.status()
        )));
    }
    Ok(res
        .bytes()
        .await
        .map_err(|e| UpdateError::Download(e.to_string()))?
        .to_vec())
}

impl Release {
    /// Fetches and verifies the manifest of a channel.
    pub async fn fetch(base_url: &str, key: &VerifyingKey) -> Result<Self, UpdateError> {
        let base = base_url.trim_end_matches('/').to_owned();
        let strict = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| UpdateError::Download(e.to_string()))?;
        let manifest_bytes = get(&strict, &format!("{base}/manifest.json")).await?;
        let sig = get(&strict, &format!("{base}/manifest.sig")).await?;
        let cert_json = get_optional(&strict, &format!("{base}/subkey.cert.json")).await?;
        let cert_sig = get_optional(&strict, &format!("{base}/subkey.cert.sig")).await?;
        let subkey = match (cert_json, cert_sig) {
            (None, None) => None,
            (Some(cert_json), Some(cert_sig)) => {
                let cert_sig =
                    String::from_utf8(cert_sig).map_err(|_| SignatureError::BadSignature)?;
                let subkey = verify_subkey_cert(&cert_json, &cert_sig, key)?;
                let cert: SubkeyCert =
                    serde_json::from_slice(&cert_json).map_err(|_| SignatureError::BadManifest)?;
                if cert.purpose != "release" {
                    return Err(SignatureError::SubkeyCertMismatch.into());
                }
                Some(subkey)
            }
            _ => return Err(SignatureError::SubkeyCertMismatch.into()),
        };
        let manifest_key = subkey.as_ref().unwrap_or(key);
        let manifest = verify_manifest(
            &manifest_bytes,
            &String::from_utf8_lossy(&sig),
            manifest_key,
        )?;
        check_fresh(&manifest)?;
        Ok(Self {
            base_url: base,
            manifest,
        })
    }

    /// The release version.
    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    /// Downloads and verifies the binary `asset`.
    pub async fn download(&self, asset: &str) -> Result<Vec<u8>, UpdateError> {
        if !self.manifest.files.contains_key(asset) {
            return Err(UpdateError::NoBuild(asset.to_owned()));
        }
        let bytes = get(&client()?, &format!("{}/{asset}", self.base_url)).await?;
        verify_file(&self.manifest, asset, &bytes)?;
        Ok(bytes)
    }
}

async fn get_optional(client: &reqwest::Client, url: &str) -> Result<Option<Vec<u8>>, UpdateError> {
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| UpdateError::Download(e.to_string()))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        return Err(UpdateError::Download(format!(
            "{url}: HTTP {}",
            res.status()
        )));
    }
    Ok(Some(
        res.bytes()
            .await
            .map_err(|e| UpdateError::Download(e.to_string()))?
            .to_vec(),
    ))
}

/// The copy of the previous version kept next to a binary.
pub fn previous(binary: &Path) -> PathBuf {
    let name = binary
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = binary
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    binary.with_file_name(format!("{name}.previous{ext}"))
}

/// Replaces `binary` with `bytes` after the new binary answers `version` with `expected`. The old
/// binary is kept as [`previous`]. Works for a running binary (it is renamed, not overwritten).
pub fn install(binary: &Path, bytes: &[u8], expected: &str) -> Result<(), UpdateError> {
    let staged = binary.with_file_name(format!(
        "{}.new{}",
        binary
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        binary
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default()
    ));
    std::fs::write(&staged, bytes)?;
    make_executable(&staged)?;
    if let Err(e) = self_check(&staged, expected) {
        let _ = std::fs::remove_file(&staged);
        return Err(e);
    }
    let old = previous(binary);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(binary, &old)?;
    if let Err(e) = std::fs::rename(&staged, binary) {
        // Put the old version back.
        let _ = std::fs::rename(&old, binary);
        return Err(e.into());
    }
    Ok(())
}

/// Swaps `binary` and its [`previous`] copy.
pub fn rollback(binary: &Path) -> Result<(), UpdateError> {
    let old = previous(binary);
    if !old.exists() {
        return Err(UpdateError::Broken("no previous version is kept".into()));
    }
    let tmp = binary.with_extension("swap");
    std::fs::rename(binary, &tmp)?;
    std::fs::rename(&old, binary)?;
    std::fs::rename(&tmp, &old)?;
    Ok(())
}

/// Runs `<binary> version` and expects `<name> <expected>`.
fn self_check(binary: &Path, expected: &str) -> Result<(), UpdateError> {
    let out = std::process::Command::new(binary)
        .arg("version")
        .output()
        .map_err(|e| UpdateError::Broken(e.to_string()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    if out.status.success() && text.split_whitespace().nth(1) == Some(expected) {
        Ok(())
    } else {
        Err(UpdateError::Broken(format!(
            "`version` printed {:?}",
            text.trim()
        )))
    }
}

#[cfg(unix)]
fn make_executable(p: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("0.1.0", "0.0.9"));
        assert!(is_newer("1.10.0", "1.9.3"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0-beta", "1.0.0"));
        assert!(is_newer("1.0.1", "1.0"));
    }

    #[test]
    fn names() {
        let n = asset_name("vibefork");
        assert!(n.starts_with("vibefork-") && n.contains(std::env::consts::ARCH));
        assert_eq!(
            previous(Path::new("/a/vibefork.exe")),
            Path::new("/a/vibefork.previous.exe")
        );
        assert_eq!(
            previous(Path::new("/a/vibefork")),
            Path::new("/a/vibefork.previous")
        );
    }
}
