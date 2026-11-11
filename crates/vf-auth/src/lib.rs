//! 🔒 **Security boundary.** Pairing, browser keys, sessions, confirmations.
//!
//! The scheme:
//! 1. the page asks to pair → the agent shows a **native dialog** with a code; the code goes nowhere
//!    else, so whoever asked to pair does not learn it. The user types the code on the page and clicks
//!    Allow (the page cannot click a native dialog); a pairing needs both, three wrong codes deny it;
//! 2. the browser keeps a **non-extractable** P-256 key; the agent stores only the public key + origin;
//! 3. every visit: one-time challenge → signature → short session token kept **in memory only**,
//!    bound to the origin;
//! 4. dangerous actions → native confirm dialog showing the action (invariant 3);
//! 5. the agent has its own key ([`identity`]): a paired editor checks the agent's signature before
//!    it sends anything, so another program on a loopback port cannot pass for the agent.

mod crypto;
pub mod identity;
mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vf_core::{ConfirmDialog, Secret};
use vf_protocol::http::{PairInfo, PairState, PublicJwk};

pub use crypto::key_id_of;
use identity::AgentIdentity;
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
    /// The identity challenge is not base64url of an accepted length.
    #[error("bad challenge")]
    BadIdentityChallenge,
    /// Too many pairing requests from this origin are waiting for the user.
    #[error("too many pairing requests; answer the open dialog first")]
    TooManyPairings,
}

/// Pairing requests one origin may keep waiting for the user at the same time.
pub const MAX_WAITING_PAIRINGS: usize = 3;
/// Wrong codes after which a pairing request is denied.
pub const MAX_WRONG_CODES: u8 = 3;
/// Longest part of a task shown when work on a project starts, characters (a longer one ends with `…`).
pub const START_TASK_SHOWN: usize = 600;
/// How long a pairing request is kept (for status polling) after it was started.
const PENDING_KEEP: Duration = Duration::from_secs(10 * 60);

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

/// What a page may know about its pairing request (never the code).
#[derive(Debug, Clone, PartialEq)]
pub struct PairingStatus {
    /// State of the request.
    pub status: PairState,
    /// Paired key, once approved.
    pub key_id: Option<String>,
    /// The right code was entered on the page.
    pub code_accepted: bool,
    /// The agent's public key, once approved: the editor keeps it to check the agent later.
    pub agent_key: Option<PublicJwk>,
}

struct Pending {
    origin: String,
    key_id: String,
    /// The record saved once both the dialog and the code say yes.
    record: PairRecord,
    /// Digits of the code shown in the dialog.
    code: String,
    /// The user clicked Allow.
    allowed: bool,
    code_accepted: bool,
    wrong_codes: u8,
    status: PairState,
    created: Instant,
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
    identity: Arc<AgentIdentity>,
    dialog: Arc<dyn ConfirmDialog>,
}

impl Auth {
    /// Loads paired browsers from `state_dir/pairs.json` (only public keys are stored there) and
    /// the agent's own key from `state_dir/agent-key.json` (made on the first start).
    pub fn load(state_dir: PathBuf, dialog: Arc<dyn ConfirmDialog>) -> Result<Self, AuthError> {
        Ok(Self {
            state: Arc::new(Mutex::new(State::default())),
            store: Arc::new(
                PairStore::load(state_dir.join("pairs.json")).map_err(|_| AuthError::Storage)?,
            ),
            identity: Arc::new(
                AgentIdentity::load_or_create(&state_dir).map_err(|_| AuthError::Storage)?,
            ),
            dialog,
        })
    }

    /// The agent's public key.
    pub fn agent_key(&self) -> PublicJwk {
        self.identity.public_jwk()
    }

    /// Proves this agent to an editor at `origin`: signs [`identity::statement`] of the origin, the
    /// `port` the agent listens on and the editor's `challenge`. Returns `(agent key, signature)`.
    pub fn prove_identity(
        &self,
        origin: &str,
        port: u16,
        challenge: &str,
    ) -> Result<(PublicJwk, String), AuthError> {
        if !identity::valid_challenge(challenge) {
            return Err(AuthError::BadIdentityChallenge);
        }
        Ok((
            self.identity.public_jwk(),
            self.identity.sign(origin, port, challenge),
        ))
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Starts pairing: returns the `pair_id` and opens the native dialog with the code on a separate
    /// thread. The code is shown only there; [`Auth::enter_pairing_code`] takes it from the page.
    pub fn start_pairing(
        &self,
        origin: &str,
        jwk: &PublicJwk,
        name: Option<&str>,
    ) -> Result<String, AuthError> {
        crypto::verifying_key(jwk)?; // reject anything that is not a valid public P-256 key
        {
            // Finished requests are kept for status polling, then dropped; one origin may keep only a
            // few dialogs waiting, so a page cannot flood the screen with them.
            let mut state = self.state();
            state
                .pending
                .retain(|_, p| p.created.elapsed() < PENDING_KEEP);
            let waiting = state
                .pending
                .values()
                .filter(|p| p.origin == origin && p.status == PairState::Pending)
                .count();
            if waiting >= MAX_WAITING_PAIRINGS {
                return Err(AuthError::TooManyPairings);
            }
        }
        let key_id = key_id_of(jwk);
        let pair_id = crypto::random_token();
        let code = crypto::pairing_code();
        let name = vf_core::dialog_field(name.unwrap_or("browser"), 80);
        self.state().pending.insert(
            pair_id.clone(),
            Pending {
                origin: origin.to_owned(),
                key_id,
                record: PairRecord {
                    jwk: jwk.clone(),
                    origin: origin.to_owned(),
                    name: name.clone(),
                    created: now_secs(),
                    projects: Vec::new(),
                },
                code: code_digits(&code),
                allowed: false,
                code_accepted: false,
                wrong_codes: 0,
                status: PairState::Pending,
                created: Instant::now(),
            },
        );

        let me = self.clone();
        let id = pair_id.clone();
        let lang = self.dialog.language();
        let title = vf_core::catalog::pair_title(lang);
        let text = vf_core::catalog::pair_text(lang, origin, &name, &code);
        std::thread::spawn(move || {
            let allowed = me.dialog.confirm(&title, &text);
            let mut state = me.state();
            if let Some(p) = state.pending.get_mut(&id)
                && p.status == PairState::Pending
            {
                if allowed {
                    p.allowed = true;
                    me.finish_if_ready(p);
                } else {
                    p.status = PairState::Denied;
                }
                tracing::info!(key_id = %p.key_id, allowed, "pairing dialog answered");
            }
        });
        Ok(pair_id)
    }

    /// The code the user typed on the page for request `pair_id` started from `origin`: returns
    /// whether it is right and the state after it, `None` for an unknown request or another origin.
    /// The request is approved once the code is right and Allow is clicked, in any order; after
    /// [`MAX_WRONG_CODES`] wrong codes it is denied for good.
    pub fn enter_pairing_code(
        &self,
        pair_id: &str,
        origin: &str,
        code: &str,
    ) -> Option<(bool, PairState)> {
        let mut state = self.state();
        let p = state
            .pending
            .get_mut(pair_id)
            .filter(|p| p.origin == origin && p.created.elapsed() < PENDING_KEEP)?;
        if p.status == PairState::Denied {
            return Some((false, PairState::Denied));
        }
        if !same_code(&p.code, &code_digits(code)) {
            p.wrong_codes = p.wrong_codes.saturating_add(1);
            if p.status == PairState::Pending && p.wrong_codes >= MAX_WRONG_CODES {
                p.status = PairState::Denied;
                tracing::warn!(key_id = %p.key_id, "pairing denied: wrong code too many times");
            }
            return Some((false, p.status));
        }
        if p.status == PairState::Pending {
            p.code_accepted = true;
            self.finish_if_ready(p);
        }
        Some((true, p.status))
    }

    /// Saves the pairing once Allow was clicked and the right code entered.
    fn finish_if_ready(&self, p: &mut Pending) {
        if !(p.allowed && p.code_accepted) {
            return;
        }
        let saved = self
            .store
            .insert(p.key_id.clone(), p.record.clone())
            .is_ok();
        p.status = if saved {
            PairState::Approved
        } else {
            PairState::Denied
        };
        tracing::info!(key_id = %p.key_id, saved, "pairing finished");
    }

    /// Status of a pairing request started from `origin`; `None` if unknown.
    pub fn pairing_status(&self, pair_id: &str, origin: &str) -> Option<PairingStatus> {
        let state = self.state();
        let p = state.pending.get(pair_id).filter(|p| p.origin == origin)?;
        Some(PairingStatus {
            status: p.status,
            key_id: (p.status == PairState::Approved).then(|| p.key_id.clone()),
            code_accepted: p.code_accepted,
            agent_key: (p.status == PairState::Approved).then(|| self.identity.public_jwk()),
        })
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

    /// Asks the user in the native dialog to allow an action that `origin` asks for: what will be
    /// done, why, and the commands with links to check them. Blocks.
    pub fn ask_permission(&self, origin: &str, request: &vf_core::Permission) -> bool {
        let origin = vf_core::dialog_field(origin, 100);
        self.dialog
            .ask_permission(Some(&origin), &request.cleaned())
    }

    /// Asks the user for a secret in the native dialog with input masked. Blocks.
    pub fn ask_secret(&self, origin: &str, prompt: &str) -> Option<Secret<String>> {
        let lang = self.dialog.language();
        let prompt = vf_core::dialog_field(prompt, 200);
        let title = vf_core::catalog::ask_secret_title(lang);
        let text = vf_core::catalog::ask_secret_text(lang, origin, &prompt);
        self.dialog.ask_secret(&title, &text)
    }

    /// Asks the user for a text input in the native dialog unmasked. Blocks.
    pub fn ask_text(&self, origin: &str, prompt: &str) -> Option<String> {
        let lang = self.dialog.language();
        let prompt = vf_core::dialog_field(prompt, 200);
        let title = vf_core::catalog::ask_text_title(lang);
        let text = vf_core::catalog::ask_text_text(lang, origin, &prompt);
        self.dialog.ask_text(&title, &text)
    }

    /// Asks the user for a secret in the native dialog with input masked, allowing empty input. Blocks.
    pub fn ask_secret_optional(&self, origin: &str, prompt: &str) -> Option<Secret<String>> {
        let lang = self.dialog.language();
        let prompt = vf_core::dialog_field(prompt, 200);
        let title = vf_core::catalog::ask_secret_title(lang);
        let text = vf_core::catalog::ask_secret_text(lang, origin, &prompt);
        self.dialog.ask_secret_optional(&title, &text)
    }

    /// The native confirm dialog.
    pub fn dialog(&self) -> &Arc<dyn ConfirmDialog> {
        &self.dialog
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
        let lang = self.dialog.language();
        let project_shown = vf_core::dialog_field(project, 80);
        let origin_clean = vf_core::dialog_field(origin, 80);
        let name_clean = vf_core::dialog_field(&pair.name, 80);
        let title = vf_core::catalog::project_access_title(lang);
        let text =
            vf_core::catalog::project_access_text(lang, &origin_clean, &name_clean, &project_shown);
        let allowed = self.dialog.confirm(&title, &text);
        allowed && self.store.add_project(key_id, project).is_ok()
    }

    /// Whether the user starts work on `project` with `task`, the first task an editor gives there:
    /// the native dialog shows who asks, the project, the task as given and what the model may do;
    /// Start / Not now. Blocks.
    pub fn start_work_allowed(&self, origin: &str, project: &str, task: &str) -> bool {
        let lang = self.dialog.language();
        let title = vf_core::catalog::start_work_title(lang);
        let text = vf_core::catalog::start_work_text(
            lang,
            &vf_core::dialog_field(origin, 80),
            &vf_core::dialog_field(project, 80),
            &vf_core::dialog_field(task, START_TASK_SHOWN),
        );
        self.dialog.confirm_with(
            &title,
            &text,
            &vf_core::catalog::btn_start(lang),
            &vf_core::catalog::btn_not_now(lang),
        )
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

/// The digits of a code as typed (`123-456`, `123 456` and `123456` are the same code).
fn code_digits(code: &str) -> String {
    code.chars().filter(char::is_ascii_digit).take(16).collect()
}

/// Compares two codes in time that does not depend on where they differ.
fn same_code(expected: &str, given: &str) -> bool {
    expected.len() == given.len()
        && expected
            .bytes()
            .zip(given.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use vf_core::Language;

    struct RecordingDialog {
        lang: Language,
        last_title: Mutex<Option<String>>,
        last_text: Mutex<Option<String>>,
    }

    impl RecordingDialog {
        fn new(lang: Language) -> Self {
            Self {
                lang,
                last_title: Mutex::new(None),
                last_text: Mutex::new(None),
            }
        }
    }

    impl ConfirmDialog for RecordingDialog {
        fn language(&self) -> Language {
            self.lang
        }

        fn confirm(&self, title: &str, text: &str) -> bool {
            *self.last_title.lock().unwrap() = Some(title.to_owned());
            *self.last_text.lock().unwrap() = Some(text.to_owned());
            true
        }

        fn ask_secret(&self, title: &str, text: &str) -> Option<Secret<String>> {
            *self.last_title.lock().unwrap() = Some(title.to_owned());
            *self.last_text.lock().unwrap() = Some(text.to_owned());
            Some(Secret::new("secret-value".to_owned()))
        }
    }

    #[test]
    fn a_permission_says_who_asks_what_why_and_how_to_check_it_in_russian() {
        let dialog = Arc::new(RecordingDialog::new(Language::Ru));
        let temp_dir =
            std::env::temp_dir().join(format!("vf_auth_test_{}", crypto::random_token()));
        let auth = Auth::load(temp_dir, dialog.clone()).unwrap();

        let (what, why) = vf_core::catalog::permission_build(Language::Ru, "notes-app");
        let request =
            vf_core::Permission::new(what, why).with_commands(["npm run build".to_owned()]);
        assert!(auth.ask_permission("http://localhost:5173", &request));
        let title = dialog.last_title.lock().unwrap().take().unwrap();
        let text = dialog.last_text.lock().unwrap().take().unwrap();

        assert_eq!(title, "Vibefork: действие на вашем компьютере");
        assert!(
            text.starts_with("Запрос от http://localhost:5173."),
            "{text}"
        );
        assert!(text.contains("Что: Собрать проект notes-app\n"), "{text}");
        assert!(text.contains("Зачем: "), "{text}");
        assert!(
            text.contains(
                "Команды:\nnpm run build\n  Проверить: https://www.google.com/search?hl=ru&q="
            ),
            "{text}"
        );
    }

    #[test]
    fn a_permission_without_commands_promises_no_check_in_english() {
        let dialog = Arc::new(RecordingDialog::new(Language::En));
        let temp_dir =
            std::env::temp_dir().join(format!("vf_auth_test_{}", crypto::random_token()));
        let auth = Auth::load(temp_dir, dialog.clone()).unwrap();

        let (what, why) =
            vf_core::catalog::permission_rollback_part(Language::En, "notes-app", "Search\nbox");
        auth.ask_permission(
            "http://localhost:5173\nTarget: forged",
            &vf_core::Permission::new(what, why),
        );
        let title = dialog.last_title.lock().unwrap().take().unwrap();
        let text = dialog.last_text.lock().unwrap().take().unwrap();

        assert_eq!(title, "Vibefork: action on your computer");
        assert!(
            text.starts_with("Request from http://localhost:5173 Target: forged."),
            "{text}"
        );
        assert!(text.contains("What: Put the part \u{201c}Search box\u{201d} of the project notes-app back as it was\n"), "{text}");
        assert!(!text.contains("check each command"), "{text}");
        assert!(!text.contains("google"), "{text}");
    }

    #[test]
    fn ask_secret_localized_russian() {
        let dialog = Arc::new(RecordingDialog::new(Language::Ru));
        let temp_dir =
            std::env::temp_dir().join(format!("vf_auth_test_{}", crypto::random_token()));
        let auth = Auth::load(temp_dir, dialog.clone()).unwrap();

        let res = auth.ask_secret("http://localhost:5173", "введите ключ");
        assert!(res.is_some());
        let title = dialog.last_title.lock().unwrap().take().unwrap();
        let text = dialog.last_text.lock().unwrap().take().unwrap();

        assert_eq!(title, "Vibefork: ввод ключа");
        assert!(text.contains("запрашивает:\n\nвведите ключ"));
    }
}
