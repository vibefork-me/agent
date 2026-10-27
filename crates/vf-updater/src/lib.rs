//! 🔒 **Security boundary.** Signed downloads: the web UI bundle served by the agent and the
//! agent's own updates. Nothing downloaded is used before its Ed25519 signature is verified with a
//! public key built into the agent.
//!
//! - [`signing`] — the signed manifest format: `manifest.json` (version + SHA-256 of every file) and
//!   `manifest.sig` (base64 Ed25519 signature of the exact `manifest.json` bytes);
//! - [`bundle`] — the UI bundle: download without redirects, verify, cache, serve from memory;
//! - [`release`] — self-updates: signed release channel, verified binary, self-check, previous
//!   version kept for rollback.

pub mod bundle;
pub mod release;
pub mod signing;

pub use bundle::UiBundle;
pub use signing::{
    Manifest, SignatureError, SubkeyCert, check_fresh, verify_manifest, verify_subkey_cert,
};

/// Reads a downloaded file of at most [`vf_core::limits::RELEASE_FILE`] bytes.
pub(crate) async fn read_file(mut res: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut body = vf_core::LimitedBody::new(vf_core::limits::RELEASE_FILE, res.content_length())
        .map_err(|e| e.to_string())?;
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        body.push(&chunk).map_err(|e| e.to_string())?;
    }
    Ok(body.into_vec())
}

/// Public key of the UI bundle signer (base64, 32 bytes). The private key is kept offline.
pub const UI_BUNDLE_PUBLIC_KEY: &str = include_str!("../../../keys/ui-bundle.pub");
/// Public key of the release signer (base64, 32 bytes) — a separate key: a signed UI bundle can never
/// pass as a release. The private key is kept offline.
pub const RELEASE_PUBLIC_KEY: &str = include_str!("../../../keys/release.pub");
