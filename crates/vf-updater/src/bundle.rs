//! The UI bundle served by the agent at `/ui/`: downloaded from `<base>/manifest.json`,
//! `<base>/manifest.sig` and `<base>/<file>`, verified, cached on disk, kept in memory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ed25519_dalek::VerifyingKey;

use crate::signing::{
    Manifest, SignatureError, SubkeyCert, check_fresh, verify_file, verify_manifest,
    verify_subkey_cert,
};

/// A verified UI bundle in memory.
#[derive(Debug, Clone)]
pub struct UiBundle {
    /// Bundle version.
    pub version: String,
    files: HashMap<String, Vec<u8>>,
}

/// Why a bundle could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    /// Network error or a non-200 answer (redirects are not followed).
    #[error("download failed: {0}")]
    Download(String),
    /// Signature or hash check failed.
    #[error(transparent)]
    Signature(#[from] SignatureError),
    /// Disk error.
    #[error("cache: {0}")]
    Cache(#[from] std::io::Error),
}

impl UiBundle {
    /// A file of the bundle (`""` = `index.html`).
    pub fn file(&self, path: &str) -> Option<&[u8]> {
        let path = if path.is_empty() || path.ends_with('/') {
            format!("{path}index.html")
        } else {
            path.to_owned()
        };
        self.files.get(&path).map(Vec::as_slice)
    }

    /// Downloads and verifies the bundle at `base_url`, then stores it in `cache_dir/<version>/`.
    pub async fn download(
        base_url: &str,
        key: &VerifyingKey,
        cache_dir: &Path,
    ) -> Result<Self, BundleError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| BundleError::Download(e.to_string()))?;
        let base = base_url.trim_end_matches('/');
        let manifest_bytes = fetch(&client, &format!("{base}/manifest.json")).await?;
        let signature = String::from_utf8(fetch(&client, &format!("{base}/manifest.sig")).await?)
            .map_err(|_| SignatureError::BadSignature)?;
        let cert_json = fetch_optional(&client, &format!("{base}/subkey.cert.json")).await?;
        let cert_sig = fetch_optional(&client, &format!("{base}/subkey.cert.sig")).await?;
        let cert = match (cert_json, cert_sig) {
            (None, None) => None,
            (Some(cert_json), Some(cert_sig)) => {
                let cert_sig =
                    String::from_utf8(cert_sig).map_err(|_| SignatureError::BadSignature)?;
                let subkey = verify_subkey_cert(&cert_json, &cert_sig, key)?;
                let cert: SubkeyCert =
                    serde_json::from_slice(&cert_json).map_err(|_| SignatureError::BadManifest)?;
                if cert.purpose != "bundle" {
                    return Err(SignatureError::SubkeyCertMismatch.into());
                }
                Some((cert_json, cert_sig, subkey))
            }
            _ => return Err(SignatureError::SubkeyCertMismatch.into()),
        };
        let manifest_key = cert.as_ref().map(|(_, _, subkey)| subkey).unwrap_or(key);
        let manifest = verify_manifest(&manifest_bytes, &signature, manifest_key)?;
        check_fresh(&manifest)?;

        if let Ok(current) = std::fs::read_to_string(cache_dir.join("current")) {
            let current = current.trim();
            if !crate::release::is_newer(&manifest.version, current) {
                if manifest.version == current {
                    if let Some(cached) = Self::load_cached(cache_dir, key)
                        && cached.version == manifest.version
                    {
                        return Ok(cached);
                    }
                } else {
                    return Err(SignatureError::Rollback.into());
                }
            }
        }
        let mut files = HashMap::new();
        for path in manifest.files.keys() {
            let bytes = fetch(&client, &format!("{base}/{path}")).await?;
            verify_file(&manifest, path, &bytes)?;
            files.insert(path.clone(), bytes);
        }
        let bundle = Self {
            version: manifest.version.clone(),
            files,
        };
        let cert = cert.map(|(json, sig, _)| (json, sig));
        bundle.save(cache_dir, &manifest_bytes, &signature, cert)?;
        Ok(bundle)
    }

    /// Loads the newest cached bundle, verifying it again.
    pub fn load_cached(cache_dir: &Path, key: &VerifyingKey) -> Option<Self> {
        let current = std::fs::read_to_string(cache_dir.join("current")).ok()?;
        let dir = cache_dir.join(current.trim());
        let manifest_bytes = std::fs::read(dir.join("manifest.json")).ok()?;
        let signature = std::fs::read_to_string(dir.join("manifest.sig")).ok()?;
        let cert_json = std::fs::read(dir.join("subkey.cert.json")).ok();
        let cert_sig = std::fs::read_to_string(dir.join("subkey.cert.sig")).ok();
        let subkey = match (cert_json, cert_sig) {
            (None, None) => None,
            (Some(cert_json), Some(cert_sig)) => {
                let subkey = verify_subkey_cert(&cert_json, &cert_sig, key).ok()?;
                let cert: SubkeyCert = serde_json::from_slice(&cert_json).ok()?;
                if cert.purpose != "bundle" {
                    return None;
                }
                Some(subkey)
            }
            _ => return None,
        };
        let manifest_key = subkey.as_ref().unwrap_or(key);
        let manifest: Manifest = verify_manifest(&manifest_bytes, &signature, manifest_key).ok()?;
        let mut files = HashMap::new();
        for path in manifest.files.keys() {
            let bytes = std::fs::read(dir.join(path)).ok()?;
            verify_file(&manifest, path, &bytes).ok()?;
            files.insert(path.clone(), bytes);
        }
        Some(Self {
            version: manifest.version,
            files,
        })
    }

    fn save(
        &self,
        cache_dir: &Path,
        manifest_bytes: &[u8],
        signature: &str,
        cert: Option<(Vec<u8>, String)>,
    ) -> std::io::Result<()> {
        let version: String = self
            .version
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            .collect();
        let dir: PathBuf = cache_dir.join(if version.is_empty() {
            "unversioned".into()
        } else {
            version.clone()
        });
        for (path, bytes) in &self.files {
            let target = dir.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, bytes)?;
        }
        std::fs::write(dir.join("manifest.json"), manifest_bytes)?;
        std::fs::write(dir.join("manifest.sig"), signature)?;
        match cert {
            Some((cert_json, cert_sig)) => {
                std::fs::write(dir.join("subkey.cert.json"), cert_json)?;
                std::fs::write(dir.join("subkey.cert.sig"), cert_sig)?;
            }
            None => {
                for path in ["subkey.cert.json", "subkey.cert.sig"] {
                    match std::fs::remove_file(dir.join(path)) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        std::fs::write(cache_dir.join("current"), version)
    }
}

async fn fetch(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, BundleError> {
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| BundleError::Download(e.to_string()))?;
    if !res.status().is_success() {
        return Err(BundleError::Download(format!(
            "{url}: HTTP {}",
            res.status()
        )));
    }
    Ok(res
        .bytes()
        .await
        .map_err(|e| BundleError::Download(e.to_string()))?
        .to_vec())
}

async fn fetch_optional(
    client: &reqwest::Client,
    url: &str,
) -> Result<Option<Vec<u8>>, BundleError> {
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| BundleError::Download(e.to_string()))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        return Err(BundleError::Download(format!(
            "{url}: HTTP {}",
            res.status()
        )));
    }
    Ok(Some(
        res.bytes()
            .await
            .map_err(|e| BundleError::Download(e.to_string()))?
            .to_vec(),
    ))
}

/// Content type by file extension.
pub fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}
