//! Signed manifest: `manifest.json` + `manifest.sig`.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// List of files of a signed bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Bundle version.
    pub version: String,
    /// Relative path → SHA-256 (lowercase hex).
    pub files: BTreeMap<String, String>,
}

/// Why a signed artifact was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SignatureError {
    /// The built-in public key is malformed.
    #[error("bad public key")]
    BadPublicKey,
    /// The signature is malformed or does not verify.
    #[error("bad signature")]
    BadSignature,
    /// The manifest is not valid JSON or has unsafe paths.
    #[error("bad manifest")]
    BadManifest,
    /// A file is not in the manifest or its hash differs.
    #[error("file does not match the manifest: {0}")]
    FileMismatch(String),
}

/// Parses a base64 Ed25519 public key.
pub fn public_key(base64: &str) -> Result<VerifyingKey, SignatureError> {
    let bytes = STANDARD
        .decode(base64.trim())
        .map_err(|_| SignatureError::BadPublicKey)?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| SignatureError::BadPublicKey)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| SignatureError::BadPublicKey)
}

/// Verifies `manifest_bytes` against a base64 signature and parses it. Paths must be safe relative
/// paths (`[A-Za-z0-9._/-]`, no `..`, no leading `/`).
pub fn verify_manifest(
    manifest_bytes: &[u8],
    signature_base64: &str,
    key: &VerifyingKey,
) -> Result<Manifest, SignatureError> {
    let sig = STANDARD
        .decode(signature_base64.trim())
        .map_err(|_| SignatureError::BadSignature)?;
    let sig = Signature::from_slice(&sig).map_err(|_| SignatureError::BadSignature)?;
    key.verify(manifest_bytes, &sig)
        .map_err(|_| SignatureError::BadSignature)?;
    let manifest: Manifest =
        serde_json::from_slice(manifest_bytes).map_err(|_| SignatureError::BadManifest)?;
    if manifest.files.keys().all(|p| is_safe_path(p)) {
        Ok(manifest)
    } else {
        Err(SignatureError::BadManifest)
    }
}

/// Checks a file against the manifest.
pub fn verify_file(manifest: &Manifest, path: &str, bytes: &[u8]) -> Result<(), SignatureError> {
    match manifest.files.get(path) {
        Some(expected) if *expected == sha256_hex(bytes) => Ok(()),
        _ => Err(SignatureError::FileMismatch(path.to_owned())),
    }
}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Signs manifest bytes; returns the base64 signature (for the signing tool).
pub fn sign(manifest_bytes: &[u8], key: &SigningKey) -> String {
    STANDARD.encode(key.sign(manifest_bytes).to_bytes())
}

pub(crate) fn is_safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    fn signed(files: &[(&str, &[u8])]) -> (SigningKey, Vec<u8>, String) {
        let key = SigningKey::generate(&mut OsRng);
        let manifest = Manifest {
            version: "1".into(),
            files: files
                .iter()
                .map(|(p, b)| ((*p).to_owned(), sha256_hex(b)))
                .collect(),
        };
        let bytes = serde_json::to_vec(&manifest).unwrap_or_default();
        let sig = sign(&bytes, &key);
        (key, bytes, sig)
    }

    #[test]
    fn valid_manifest_and_files_pass() {
        let (key, bytes, sig) = signed(&[("index.html", b"<h1>hi</h1>")]);
        let m = verify_manifest(&bytes, &sig, &key.verifying_key());
        assert!(m.is_ok());
        let m = m.unwrap_or_else(|_| Manifest {
            version: String::new(),
            files: BTreeMap::new(),
        });
        assert_eq!(verify_file(&m, "index.html", b"<h1>hi</h1>"), Ok(()));
    }

    #[test]
    fn tampered_manifest_file_or_wrong_key_fail() {
        let (key, mut bytes, sig) = signed(&[("index.html", b"<h1>hi</h1>")]);
        let other = SigningKey::generate(&mut OsRng);
        assert_eq!(
            verify_manifest(&bytes, &sig, &other.verifying_key()),
            Err(SignatureError::BadSignature)
        );
        let m = verify_manifest(&bytes, &sig, &key.verifying_key()).unwrap_or_else(|_| Manifest {
            version: String::new(),
            files: BTreeMap::new(),
        });
        assert!(verify_file(&m, "index.html", b"<h1>evil</h1>").is_err());
        assert!(verify_file(&m, "other.js", b"x").is_err());
        if let Some(b) = bytes.last_mut() {
            *b ^= 1;
        }
        assert_eq!(
            verify_manifest(&bytes, &sig, &key.verifying_key()),
            Err(SignatureError::BadSignature)
        );
    }

    #[test]
    fn unsafe_paths_are_rejected() {
        for p in ["../x", "/abs", "a/../b", "a//b", "a\\b", "", "./x"] {
            assert!(!is_safe_path(p), "{p}");
        }
        assert!(is_safe_path("assets/app-1.js"));
    }
}
