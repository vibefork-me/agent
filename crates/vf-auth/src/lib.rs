//! 🔒 **Security boundary.** Pairing, browser keys, sessions, confirmations.
//!
//! The scheme:
//! 1. the page asks to pair → the agent shows a **native dialog** with a code; the user compares the
//!    code with the page and clicks Allow (the page cannot click a native dialog);
//! 2. the browser keeps a **non-extractable** P-256 key; the agent stores only the public key + origin;
//! 3. every visit: one-time challenge → signature → short session token kept **in memory only**,
//!    bound to the origin;
//! 4. dangerous actions → native confirm dialog showing the action (invariant 3).

mod crypto;
mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vf_core::{ConfirmDialog, Secret};
use vf_protocol::http::{PairInfo, PairState, PublicJwk};

pub use crypto::key_id_of;
use store::{PairRecord, PairStore};

/// A session token. Never printed (invariant 6).
pub type SessionToken = Secret<String>;

/// Session lifetime.
pub const SESSION_TTL: Duration = Duration::from_secs(15 * 60);
/// Challenge lifetime.
pub const CHALLENGE_TTL: Duration = Duration::from_secs(60);

/// Why an auth operation was refused. Messages are safe to return to the client.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    /// The public key is not an EC P-256 public JWK.
    #[error("P-256 public JWK required")]
    BadKey,
    /// The key is not paired for this origin.
    #[error("not paired")]
    NotPaired,
    /// Unknown, expired or already used challenge.
    #[error("bad or expired challenge")]
    BadChallenge,
    /// The signature does not verify.
    #[error("bad signature")]
    BadSignature,
    /// Pairings could not be saved.
    #[error("storage error")]
    Storage,
}

/// A valid session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// Paired key.
    pub key_id: String,
    /// Origin the session is bound to.
    pub origin: String,
    /// Browser name.
    pub name: String,
}

struct Pending {
    origin: String,
    key_id: String,
    status: PairState,
}

struct Session {
    key_id: String,
    origin: String,
    expires: Instant,
}

#[derive(Default)]
struct State {
    pending: HashMap<String, Pending>,
    nonces: HashMap<String, (String, Instant)>,
    sessions: HashMap<String, Session>,
}

/// Pairing and session state of the agent. Cheap to clone (shared inside).
#[derive(Clone)]
pub struct Auth {
    state: Arc<Mutex<State>>,
    store: Arc<PairStore>,
    dialog: Arc<dyn ConfirmDialog>,
}

impl Auth {
    /// Loads paired browsers from `state_dir/pairs.json` (only public keys are stored there).
    pub fn load(state_dir: PathBuf, dialog: Arc<dyn ConfirmDialog>) -> Result<Self, AuthError> {
        Ok(Self {
            state: Arc::new(Mutex::new(State::default())),
            store: Arc::new(
                PairStore::load(state_dir.join("pairs.json")).map_err(|_| AuthError::Storage)?,
            ),
            dialog,
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Starts pairing: returns `(pair_id, code)` and opens the native dialog on a separate thread.
    pub fn start_pairing(
        &self,
        origin: &str,
        jwk: &PublicJwk,
        name: Option<&str>,
    ) -> Result<(String, String), AuthError> {
        crypto::verifying_key(jwk)?; // reject anything that is not a valid public P-256 key
        let key_id = key_id_of(jwk);
        let pair_id = crypto::random_token();
        let code = crypto::pairing_code();
        let name: String = name.unwrap_or("browser").chars().take(80).collect();
        self.state().pending.insert(
            pair_id.clone(),
            Pending {
                origin: origin.to_owned(),
                key_id: key_id.clone(),
                status: PairState::Pending,
            },
        );

        let me = self.clone();
        let (origin, jwk, id) = (origin.to_owned(), jwk.clone(), pair_id.clone());
        let text = format!(
            "Site: {origin}\nBrowser: {name}\nCode: {code}\n\nAllow only if the same code is shown on the page."
        );
        std::thread::spawn(move || {
            let allowed = me.dialog.confirm("vibefork: connect a browser?", &text);
            let saved = allowed
                && me
                    .store
                    .insert(
                        key_id.clone(),
                        PairRecord {
                            jwk,
                            origin,
                            name,
                            created: now_secs(),
                            projects: Vec::new(),
                        },
                    )
                    .is_ok();
            if let Some(p) = me.state().pending.get_mut(&id) {
                p.status = if saved {
                    PairState::Approved
                } else {
                    PairState::Denied
                };
            }
            tracing::info!(key_id = %key_id, allowed = saved, "pairing finished");
        });
        Ok((pair_id, code))
    }

    /// Status of a pairing request started from `origin`; `None` if unknown.
    pub fn pairing_status(
        &self,
        pair_id: &str,
        origin: &str,
    ) -> Option<(PairState, Option<String>)> {
        let state = self.state();
        let p = state.pending.get(pair_id).filter(|p| p.origin == origin)?;
        Some((
            p.status,
            (p.status == PairState::Approved).then(|| p.key_id.clone()),
        ))
    }

    /// Issues a one-time challenge for a key paired with `origin`.
    pub fn challenge(&self, key_id: &str, origin: &str) -> Result<String, AuthError> {
        if !self.store.is_paired(key_id, origin) {
            return Err(AuthError::NotPaired);
        }
        let nonce = crypto::random_token();
        self.state().nonces.insert(
            nonce.clone(),
            (key_id.to_owned(), Instant::now() + CHALLENGE_TTL),
        );
        Ok(nonce)
    }

    /// Verifies the signed challenge and opens a session bound to `origin`.
    pub fn open_session(
        &self,
        key_id: &str,
        nonce: &str,
        signature_b64url: &str,
        origin: &str,
    ) -> Result<SessionToken, AuthError> {
        let issued = self.state().nonces.remove(nonce); // one-time, even if verification fails
        match issued {
            Some((k, expires)) if k == key_id && expires > Instant::now() => {}
            _ => return Err(AuthError::BadChallenge),
        }
        let record = self
            .store
            .get(key_id)
            .filter(|r| r.origin == origin)
            .ok_or(AuthError::NotPaired)?;
        crypto::verify(&record.jwk, nonce.as_bytes(), signature_b64url)?;
        let token = crypto::random_token();
        self.state().sessions.insert(
            token.clone(),
            Session {
                key_id: key_id.to_owned(),
                origin: origin.to_owned(),
                expires: Instant::now() + SESSION_TTL,
            },
        );
        Ok(Secret::new(token))
    }

    /// The session for a bearer token used from `origin`, if valid.
    pub fn session(&self, token: &str, origin: &str) -> Option<SessionInfo> {
        let (key_id, session_origin) = {
            let state = self.state();
            let s = state.sessions.get(token)?;
            if s.expires <= Instant::now() || s.origin != origin {
                return None;
            }
            (s.key_id.clone(), s.origin.clone())
        };
        let record = self.store.get(&key_id)?; // revoked → no session
        Some(SessionInfo {
            key_id,
            origin: session_origin,
            name: record.name,
        })
    }

    /// Asks the user to confirm an action in the native dialog. Blocks.
    pub fn confirm_action(&self, origin: &str, action: &str, target: Option<&str>) -> bool {
        let action: String = action.chars().take(100).collect();
        let target = target
            .map(|t| format!("\nTarget: {}", t.chars().take(200).collect::<String>()))
            .unwrap_or_default();
        self.dialog.confirm(
            "vibefork: confirm an action",
            &format!("{origin} asks to:\n\n{action}{target}"),
        )
    }

    /// Whether a paired browser may work in `project`. The first time, the user is asked in the
    /// native dialog; a yes is remembered for that browser. Blocks.
    pub fn project_allowed(&self, key_id: &str, origin: &str, project: &str) -> bool {
        if self.store.has_project(key_id, project) {
            return true;
        }
        let Some(pair) = self.store.get(key_id).filter(|p| p.origin == origin) else {
            return false;
        };
        let project_shown: String = project.chars().take(80).collect();
        let allowed = self.dialog.confirm(
            "vibefork: give access to a project?",
            &format!(
                "{origin} ({}) asks to work in the project:\n\n{project_shown}\n\nAllow only if you are doing this in your browser right now.",
                pair.name
            ),
        );
        allowed && self.store.add_project(key_id, project).is_ok()
    }

    /// Paired browsers.
    pub fn pairs(&self) -> Vec<PairInfo> {
        self.store
            .list()
            .into_iter()
            .map(|(key_id, r)| PairInfo {
                key_id,
                origin: r.origin,
                name: r.name,
                created: r.created,
                projects: r.projects,
            })
            .collect()
    }

    /// Revokes a key and closes its sessions.
    pub fn revoke(&self, key_id: &str) -> Result<(), AuthError> {
        self.store.remove(key_id).map_err(|_| AuthError::Storage)?;
        self.state().sessions.retain(|_, s| s.key_id != key_id);
        Ok(())
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
