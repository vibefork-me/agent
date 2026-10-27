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
    /// Unix timestamp (seconds) after which the manifest is rejected. `None` = no expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

/// A subkey certified by the root key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubkeyCert {
    /// Base64 Ed25519 public key of the subkey.
    pub public_key: String,
    /// Unix timestamp after which the cert is rejected.
    pub expires_at: i64,
    /// `"bundle"` or `"release"`.
    pub purpose: String,
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
    /// The manifest's expiry has passed.
    #[error("manifest expired")]
    Expired,
    /// The subkey certificate's expiry has passed.
    #[error("subkey certificate expired")]
    SubkeyCertExpired,
    /// A subkey certificate is required, but missing. Direct signing with root key is rejected.
    #[error("subkey certificate missing")]
    SubkeyCertMissing,
    /// The subkey certificate has an unsupported or unexpected purpose.
    #[error("subkey certificate purpose mismatch")]
    SubkeyCertMismatch,
    /// The artifact version is older than the currently cached version.
    #[error("artifact version rollback rejected")]
    Rollback,
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
/// paths (`[A-Za-z0-9._/-]`, no `..`, no leading `/`). Expiry is checked separately by
/// [`check_fresh`] for network downloads; cached files remain usable offline.
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

/// Rejects a manifest whose download freshness period has expired.
pub fn check_fresh(manifest: &Manifest) -> Result<(), SignatureError> {
    if let Some(exp) = manifest.expires_at {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if now > exp {
            return Err(SignatureError::Expired);
        }
    }
    Ok(())
}

const SUBKEY_CERT_DOMAIN: &[u8] = b"vibefork-subkey-cert-v1\n";

fn subkey_cert_message(cert_json: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(SUBKEY_CERT_DOMAIN.len() + cert_json.len());
    message.extend_from_slice(SUBKEY_CERT_DOMAIN);
    message.extend_from_slice(cert_json);
    message
}

/// Signs the domain-separated bytes of a subkey certificate.
pub fn sign_subkey_cert(cert_json: &[u8], root_key: &SigningKey) -> String {
    STANDARD.encode(root_key.sign(&subkey_cert_message(cert_json)).to_bytes())
}

/// Verifies a root-signed subkey certificate and returns its public key.
pub fn verify_subkey_cert(
    cert_json: &[u8],
    cert_sig_base64: &str,
    root_key: &VerifyingKey,
) -> Result<VerifyingKey, SignatureError> {
    let sig = STANDARD
        .decode(cert_sig_base64.trim())
        .map_err(|_| SignatureError::BadSignature)?;
    let sig = Signature::from_slice(&sig).map_err(|_| SignatureError::BadSignature)?;
    root_key
        .verify(&subkey_cert_message(cert_json), &sig)
        .map_err(|_| SignatureError::BadSignature)?;
    let cert: SubkeyCert =
        serde_json::from_slice(cert_json).map_err(|_| SignatureError::BadManifest)?;
    if cert.purpose != "bundle" && cert.purpose != "release" {
        return Err(SignatureError::SubkeyCertMismatch);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if now > cert.expires_at {
        return Err(SignatureError::SubkeyCertExpired);
    }
    public_key(&cert.public_key)
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
            expires_at: None,
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
            expires_at: None,
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
            expires_at: None,
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

    #[test]
    fn expired_manifest_is_rejected() {
        let key = SigningKey::generate(&mut OsRng);
        let manifest = Manifest {
            version: "1".into(),
            files: BTreeMap::new(),
            expires_at: Some(0),
        };
        let bytes = serde_json::to_vec(&manifest).unwrap_or_default();
        let sig = sign(&bytes, &key);
        let verified = verify_manifest(&bytes, &sig, &key.verifying_key());
        assert!(verified.is_ok());
        assert_eq!(
            verified.and_then(|manifest| check_fresh(&manifest)),
            Err(SignatureError::Expired)
        );
    }

    #[test]
    fn subkey_certificate_verifies_and_rejects_tampering() {
        let root = SigningKey::generate(&mut OsRng);
        let subkey = SigningKey::generate(&mut OsRng);
        let cert = SubkeyCert {
            public_key: STANDARD.encode(subkey.verifying_key().to_bytes()),
            expires_at: i64::MAX,
            purpose: "bundle".into(),
        };
        let bytes = serde_json::to_vec(&cert).unwrap_or_default();
        let sig = sign_subkey_cert(&bytes, &root);
        assert_eq!(
            verify_subkey_cert(&bytes, &sig, &root.verifying_key()),
            Ok(subkey.verifying_key())
        );
        let mut tampered = bytes;
        tampered.push(b' ');
        assert_eq!(
            verify_subkey_cert(&tampered, &sig, &root.verifying_key()),
            Err(SignatureError::BadSignature)
        );
    }

    #[test]
    fn manifest_and_certificate_signatures_are_domain_separated() {
        let root = SigningKey::generate(&mut OsRng);
        let subkey = SigningKey::generate(&mut OsRng);
        let manifest = Manifest {
            version: "1".into(),
            files: BTreeMap::new(),
            expires_at: None,
        };
        let manifest_json = serde_json::to_vec(&manifest).unwrap_or_default();
        let manifest_sig = sign(&manifest_json, &root);
        assert_eq!(
            verify_subkey_cert(&manifest_json, &manifest_sig, &root.verifying_key()),
            Err(SignatureError::BadSignature)
        );

        let cert = SubkeyCert {
            public_key: STANDARD.encode(subkey.verifying_key().to_bytes()),
            expires_at: i64::MAX,
            purpose: "bundle".into(),
        };
        let cert_json = serde_json::to_vec(&cert).unwrap_or_default();
        let cert_sig = sign_subkey_cert(&cert_json, &root);
        assert_eq!(
            verify_manifest(&cert_json, &cert_sig, &root.verifying_key()),
            Err(SignatureError::BadSignature)
        );
    }

    #[test]
    fn manifest_with_unknown_field_verifies_for_forward_compatibility() {
        let key = SigningKey::generate(&mut OsRng);
        let bytes = br#"{"version":"1","files":{},"future_field":{"value":true}}"#;
        let sig = sign(bytes, &key);
        let manifest = verify_manifest(bytes, &sig, &key.verifying_key());
        assert_eq!(
            manifest,
            Ok(Manifest {
                version: "1".into(),
                files: BTreeMap::new(),
                expires_at: None,
            })
        );
    }
}
