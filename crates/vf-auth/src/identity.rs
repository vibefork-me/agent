//! The agent's own key: proves to a paired editor that it talks to this agent, not to another
//! program on the same loopback port.
//!
//! The private key lives in `agent-key.json` in the agent's state folder (owner-only) and never
//! leaves the agent. The editor learns the public key when it pairs and from then on sends nothing
//! before the agent signs the editor's random challenge together with the editor's origin and the
//! port the agent really listens on — so a program on another port that relays requests to the agent
//! cannot pass the check.

use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{Signature, SigningKey};
use rand::RngCore as _;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use vf_protocol::http::PublicJwk;

/// Prefix of every signed identity statement; a signature over anything else is never made.
pub const IDENTITY_CONTEXT: &str = "vibefork-agent-identity-v1";
/// Shortest and longest challenge accepted, characters of base64url.
pub const CHALLENGE_LEN: std::ops::RangeInclusive<usize> = 22..=128;

/// The file with the agent's private key.
const FILE: &str = "agent-key.json";

#[derive(Serialize, Deserialize)]
struct KeyFile {
    /// Private scalar, base64url.
    d: String,
}

/// The agent's signing key.
pub(crate) struct AgentIdentity {
    key: SigningKey,
}

impl AgentIdentity {
    /// Loads the key from `state_dir`, or creates and saves a new one when there is none or the
    /// file cannot be read as a key (paired editors then pair again).
    pub(crate) fn load_or_create(state_dir: &Path) -> io::Result<Self> {
        let path = state_dir.join(FILE);
        match std::fs::read(&path) {
            Ok(bytes) => {
                if let Some(key) = parse(&bytes) {
                    return Ok(Self { key });
                }
                tracing::warn!(file = %path.display(), "the agent's key is damaged; a new one is made, editors pair again");
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let key = new_key();
        save(&path, &key)?;
        Ok(Self { key })
    }

    /// The public key, as the editor stores it.
    pub(crate) fn public_jwk(&self) -> PublicJwk {
        let point = self.key.verifying_key().to_sec1_point(false);
        PublicJwk {
            kty: "EC".into(),
            crv: "P-256".into(),
            x: point
                .x()
                .map(|x| URL_SAFE_NO_PAD.encode(x))
                .unwrap_or_default(),
            y: point
                .y()
                .map(|y| URL_SAFE_NO_PAD.encode(y))
                .unwrap_or_default(),
            d: None,
        }
    }

    /// ECDSA P-256 / SHA-256 signature (IEEE P1363, base64url) of [`statement`].
    pub(crate) fn sign(&self, origin: &str, port: u16, challenge: &str) -> String {
        let signature: Signature = self.key.sign(statement(origin, port, challenge).as_bytes());
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    }
}

/// What the agent signs: the context, the editor's origin, the port it listens on, the challenge.
pub fn statement(origin: &str, port: u16, challenge: &str) -> String {
    format!("{IDENTITY_CONTEXT}\n{origin}\n{port}\n{challenge}")
}

/// Whether `challenge` is base64url of an accepted length.
pub(crate) fn valid_challenge(challenge: &str) -> bool {
    CHALLENGE_LEN.contains(&challenge.len())
        && challenge
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn parse(bytes: &[u8]) -> Option<SigningKey> {
    let file: KeyFile = serde_json::from_slice(bytes).ok()?;
    let d = URL_SAFE_NO_PAD.decode(file.d).ok()?;
    SigningKey::from_slice(&d).ok()
}

fn new_key() -> SigningKey {
    loop {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        // Fails only for zero or a value not below the group order: practically never; draw again.
        if let Ok(key) = SigningKey::from_slice(&bytes) {
            return key;
        }
    }
}

fn save(path: &PathBuf, key: &SigningKey) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = KeyFile {
        d: URL_SAFE_NO_PAD.encode(key.to_bytes()),
    };
    let tmp = path.with_extension("json.tmp");
    crate::store::write_owner_only(&tmp, &serde_json::to_vec(&file).map_err(io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::VerifyingKey;
    use p256::ecdsa::signature::Verifier as _;

    fn temp_dir() -> PathBuf {
        let mut bytes = [0u8; 8];
        OsRng.fill_bytes(&mut bytes);
        std::env::temp_dir().join(format!("vf-identity-{}", u64::from_le_bytes(bytes)))
    }

    fn verifying_key(jwk: &PublicJwk) -> VerifyingKey {
        let mut sec1 = vec![4u8];
        sec1.extend(URL_SAFE_NO_PAD.decode(&jwk.x).unwrap());
        sec1.extend(URL_SAFE_NO_PAD.decode(&jwk.y).unwrap());
        VerifyingKey::from_sec1_bytes(&sec1).unwrap()
    }

    #[test]
    fn the_key_is_kept_between_starts_and_signs_the_statement() {
        let dir = temp_dir();
        let first = AgentIdentity::load_or_create(&dir).unwrap();
        let again = AgentIdentity::load_or_create(&dir).unwrap();
        assert_eq!(first.public_jwk(), again.public_jwk());
        assert!(first.public_jwk().d.is_none());

        let sig = first.sign("https://ui.test", 47802, "challenge-challenge-123");
        let sig = Signature::from_slice(&URL_SAFE_NO_PAD.decode(sig).unwrap()).unwrap();
        let key = verifying_key(&first.public_jwk());
        let signed = statement("https://ui.test", 47802, "challenge-challenge-123");
        assert_eq!(
            signed,
            "vibefork-agent-identity-v1\nhttps://ui.test\n47802\nchallenge-challenge-123"
        );
        assert!(key.verify(signed.as_bytes(), &sig).is_ok());
        let other_port = statement("https://ui.test", 47801, "challenge-challenge-123");
        assert!(key.verify(other_port.as_bytes(), &sig).is_err());
    }

    #[test]
    fn a_damaged_key_file_is_replaced() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), b"not json").unwrap();
        let identity = AgentIdentity::load_or_create(&dir).unwrap();
        let reloaded = AgentIdentity::load_or_create(&dir).unwrap();
        assert_eq!(identity.public_jwk(), reloaded.public_jwk());
    }

    #[cfg(unix)]
    #[test]
    fn the_key_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = temp_dir();
        AgentIdentity::load_or_create(&dir).unwrap();
        let mode = std::fs::metadata(dir.join(FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn challenges_are_base64url_of_a_sane_length() {
        assert!(valid_challenge("AAAAAAAAAAAAAAAAAAAAAA"));
        assert!(!valid_challenge("short"));
        assert!(!valid_challenge(&"A".repeat(129)));
        assert!(!valid_challenge("AAAAAAAAAAAAAAAAAAAAA\n"));
        assert!(!valid_challenge("AAAAAAAAAAAAAAAAAAAAA/"));
    }
}
