//! HTTP request and response bodies of `/v1`. Field names are camelCase on the wire.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::event::Usage;
use crate::model::ModelInfo;

/// `GET /v1/hello` — the only call that needs neither a session nor an allowed origin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    /// Always `vibefork`.
    pub agent: String,
    /// Agent version.
    pub version: String,
    /// `windows`, `macos`, `linux`.
    pub platform: String,
    /// Whether extra web origins are in effect (development builds only). The list itself is not exposed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub extra_origins_active: bool,
}

/// An error body: `{ "error": "…" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorBody {
    /// Human-readable reason.
    pub error: String,
}

/// A browser's public key (JWK, EC P-256). A private part (`d`) is rejected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PublicJwk {
    /// Must be `EC`.
    pub kty: String,
    /// Must be `P-256`.
    pub crv: String,
    /// X coordinate, base64url.
    pub x: String,
    /// Y coordinate, base64url.
    pub y: String,
    /// Private key — must be absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
}

/// `POST /v1/pair/start`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairStartRequest {
    /// The browser's public key.
    pub public_key_jwk: PublicJwk,
    /// Browser name shown in the agent's dialog.
    #[serde(default)]
    pub name: Option<String>,
}

/// Response of `POST /v1/pair/start`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairStartResponse {
    /// Id to poll `GET /v1/pair/status?id=…`.
    pub pair_id: String,
    /// Code shown both on the page and in the agent's dialog (`123-456`).
    pub code: String,
}

/// State of a pairing request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PairState {
    /// The dialog is open.
    Pending,
    /// The user allowed it.
    Approved,
    /// The user blocked it or the dialog timed out.
    Denied,
}

/// Response of `GET /v1/pair/status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairStatusResponse {
    /// State.
    pub status: PairState,
    /// Key id once approved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
}

/// `POST /v1/auth/challenge`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeRequest {
    /// Paired key.
    pub key_id: String,
}

/// Response of `POST /v1/auth/challenge`: a one-time nonce, valid for 60 s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChallengeResponse {
    /// The nonce to sign (its UTF-8 bytes).
    pub nonce: String,
}

/// `POST /v1/auth/session`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequest {
    /// Paired key.
    pub key_id: String,
    /// Nonce from the challenge.
    pub nonce: String,
    /// ECDSA P-256 / SHA-256 signature of the nonce, IEEE P1363 (r‖s), base64url.
    pub signature: String,
}

/// Response of `POST /v1/auth/session`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    /// Bearer token; kept in memory only.
    pub token: String,
    /// Lifetime in seconds.
    pub expires_in: u64,
}

/// Response of `GET /v1/me`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    /// Paired key.
    pub key_id: String,
    /// Origin of the session.
    pub origin: String,
    /// Browser name.
    pub name: String,
}

/// One paired browser in `GET /v1/pairs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairInfo {
    /// Key id.
    pub key_id: String,
    /// Origin the key is bound to.
    pub origin: String,
    /// Browser name.
    pub name: String,
    /// Pairing time, Unix seconds.
    pub created: u64,
    /// Projects this browser was given access to.
    #[serde(default)]
    pub projects: Vec<String>,
}

/// `POST /v1/pairs/revoke`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RevokeRequest {
    /// Key to revoke.
    pub key_id: String,
}

/// Response of `POST /v1/pairs/revoke`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RevokeResponse {
    /// Revoked key id.
    pub revoked: String,
}

/// Response of `POST /v1/github/connect`: the code the user enters on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GithubConnectResponse {
    /// Code to enter.
    pub user_code: String,
    /// Where to enter it (`https://github.com/login/device`).
    pub verification_uri: String,
    /// Seconds the code is valid.
    pub expires_in: u64,
}

/// Response of `GET /v1/github/status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GithubStatusResponse {
    /// The agent has a GitHub sign-in.
    pub connected: bool,
    /// A sign-in code is waiting for the user.
    pub pending: bool,
    /// GitHub login.
    pub login: Option<String>,
    /// Why the last sign-in failed.
    pub error: Option<String>,
}

/// Request of `POST /v1/projects/import`: a project from GitHub, without git on the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImportRequest {
    /// Where the code comes from: `owner/name` or `https://github.com/owner/name` (the original, or a
    /// fork of it).
    pub repo: String,
    /// Branch, tag, commit or `pull/<N>/head` of `repo`; absent — its default branch.
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// The original when `repo` is a fork or the ref is a pull request: `owner/name`; absent —
    /// `repo` is the original.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
}

/// Response of `POST /v1/projects/import`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImportResponse {
    /// Project name (folder).
    pub project: String,
    /// The original: `owner/name`.
    pub upstream: String,
    /// Commit the project was taken from.
    pub commit: String,
    /// Present when the code is not the original's (a fork or a pull request): what it changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign: Option<ForeignChanges>,
}

/// What code from outside the original changes compared with it, so the user sees it before a build.
/// Sorted by file name only; the content is not reviewed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ForeignChanges {
    /// Where the code comes from: `owner/name@ref`.
    pub source: String,
    /// Number of changed files.
    pub files: u32,
    /// Changed dependency manifests and lockfiles.
    pub dependencies: Vec<String>,
    /// Changed build scripts, installers and CI files.
    pub build_scripts: Vec<String>,
    /// Changed binary files.
    pub binaries: Vec<String>,
    /// Whether every changed file was checked (false: GitHub's list was cut or not available).
    pub complete: bool,
    /// The comparison on github.com.
    pub compare_url: String,
}

/// Request of `POST /v1/save`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SaveRequest {
    /// Project.
    pub project: String,
    /// Repository to save into (`owner/name`), when the user picked one; otherwise the agent
    /// finds or proposes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<String>,
}

/// What "Save" did or what the user does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SaveStep {
    /// Saved to the user's copy.
    Saved,
    /// Nothing new since the last save.
    Unchanged,
    /// Connect GitHub first (`POST /v1/github/connect`).
    ConnectGithub,
    /// Create the copy of the original on github.com (`url`; `name` — a free name for it when
    /// the usual one is taken; `next_url` — allowing the app on it, for a private copy).
    CreateCopy,
    /// The project has no original on GitHub: create an empty private repository (`url`, named
    /// `name`), then allow the app on it (`next_url`).
    CreateRepo,
    /// Allow the app on the copy (`url`).
    AllowApp,
    /// The app reaches every repository of the account: leave only the copy (`url`).
    NarrowAccess,
    /// The copy has moved on, and these `files` were changed both there and here: nothing saved.
    Conflict,
}

/// Response of `POST /v1/save`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SaveResponse {
    /// Result or next step.
    pub step: SaveStep,
    /// Page to open (next step) or the saved work on GitHub.
    pub url: Option<String>,
    /// The user's copy: `owner/name`.
    pub copy: Option<String>,
    /// Saved commit.
    pub commit: Option<String>,
    /// The page after `url` (see [`SaveStep`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_url: Option<String>,
    /// Name for the repository the user creates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Files of a conflict.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

/// Response of `GET /v1/projects`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectsResponse {
    /// Folder with the projects.
    pub root: String,
    /// Project names (folder names).
    pub projects: Vec<String>,
}

/// Response of `GET /v1/models`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelsResponse {
    /// Models available on this machine.
    pub models: Vec<ModelInfo>,
}

/// One journal entry: a task or a plan step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    /// RFC 3339 time.
    pub at: String,
    /// Provider (`planner` for plan records).
    pub provider: String,
    /// Model.
    pub model: String,
    /// The task or `[step n/m] title`.
    pub prompt: String,
    /// Provider session id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// Context came from the journal (another model before).
    #[serde(default)]
    pub handoff: bool,
    /// Final message of the model.
    #[serde(default)]
    pub summary: String,
    /// Newest commit of the step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// All commits of the step (the model may commit by itself).
    #[serde(default)]
    pub commits: Vec<String>,
    /// Changed files, e.g. `3 files`.
    #[serde(default)]
    pub files: String,
    /// Duration, milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
    /// Cost reported by the provider, USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Token usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// Request of `POST /v1/rollback`: reset the project to a checkpoint of its journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RollbackRequest {
    /// Project.
    pub project: String,
    /// A commit listed in the project's journal.
    pub commit: String,
}

/// Response of `POST /v1/rollback`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RollbackResponse {
    /// Project.
    pub project: String,
    /// `HEAD` after the rollback.
    pub commit: String,
}

/// Response of `GET /v1/journal?project=…`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JournalResponse {
    /// Entries, oldest first.
    pub entries: Vec<JournalEntry>,
}
