//! Key parsing, signature verification, random tokens.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::ecdsa::signature::Verifier as _;
use p256::ecdsa::{Signature, VerifyingKey};
use rand::rngs::OsRng;
use rand::{Rng as _, RngCore as _};
use sha2::{Digest as _, Sha256};
use vf_protocol::http::PublicJwk;

use crate::AuthError;

/// Parses an EC P-256 **public** JWK. Private keys (`d`) and other curves are rejected.
pub(crate) fn verifying_key(jwk: &PublicJwk) -> Result<VerifyingKey, AuthError> {
    if jwk.kty != "EC" || jwk.crv != "P-256" || jwk.d.is_some() {
        return Err(AuthError::BadKey);
    }
    let x = decode_coordinate(&jwk.x)?;
    let y = decode_coordinate(&jwk.y)?;
    let mut sec1 = [0u8; 65];
    sec1[0] = 0x04;
    sec1[1..33].copy_from_slice(&x);
    sec1[33..65].copy_from_slice(&y);
    VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| AuthError::BadKey)
}

fn decode_coordinate(b64: &str) -> Result<[u8; 32], AuthError> {
    URL_SAFE_NO_PAD
        .decode(b64)
        .ok()
        .and_then(|v| <[u8; 32]>::try_from(v).ok())
        .ok_or(AuthError::BadKey)
}

/// Verifies an ECDSA P-256 / SHA-256 signature in IEEE P1363 form (r‖s, 64 bytes, base64url) — the
/// format WebCrypto produces.
pub(crate) fn verify(
    jwk: &PublicJwk,
    message: &[u8],
    signature_b64url: &str,
) -> Result<(), AuthError> {
    let key = verifying_key(jwk)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(signature_b64url)
        .map_err(|_| AuthError::BadSignature)?;
    let signature = Signature::from_slice(&bytes).map_err(|_| AuthError::BadSignature)?;
    key.verify(message, &signature)
        .map_err(|_| AuthError::BadSignature)
}

/// Stable key id: first 16 chars of base64url(SHA-256("P-256:x:y")).
pub fn key_id_of(jwk: &PublicJwk) -> String {
    let digest = Sha256::digest(format!("{}:{}:{}", jwk.crv, jwk.x, jwk.y));
    URL_SAFE_NO_PAD.encode(digest).chars().take(16).collect()
}

/// 32 random bytes from the OS, base64url.
pub(crate) fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Six random digits shown as `123-456`.
pub(crate) fn pairing_code() -> String {
    let n: u32 = OsRng.gen_range(0..1_000_000);
    format!("{:03}-{:03}", n / 1000, n % 1000)
}
