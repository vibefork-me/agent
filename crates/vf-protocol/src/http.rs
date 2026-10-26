//! HTTP request and response bodies of `/v1`. Field names are camelCase on the wire.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::event::Usage;
use crate::model::ModelInfo;

/// `GET /v1/hello` — answers without an Origin header (or from allowed origins); does not require a session.
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
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    /// Human-readable reason.
    pub error: String,
    /// Machine-readable code (e.g. `rate_limit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Reset time in minutes (when rate limited).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "reset_minutes"
    )]
    pub reset_minutes: Option<u64>,
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
    /// The same projects with where their code comes from.
    #[serde(default)]
    pub details: Vec<ProjectInfo>,
}

/// A project and where its code comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectInfo {
    /// Project name (folder).
    pub name: String,
    /// GitHub repository the code was taken from: `owner/name`; absent — not taken from GitHub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The original when the code came from a fork or a pull request: `owner/name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
    /// Commit the code was taken at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
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
    /// Prompt version used, if applicable (e.g. `plan:1`, `loop:1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_version: Option<String>,
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

/// `GET /v1/file?project=…&path=…`: a file for the code viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileResponse {
    /// Project-relative path, `/`-separated.
    pub path: String,
    /// UTF-8 text; absent for a binary or too large file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Size in bytes.
    pub size: u64,
    /// Larger than 2 MiB: described, not sent.
    pub too_large: bool,
    /// Not UTF-8 text.
    pub binary: bool,
}

/// `GET /v1/diff?project=…&path=…`: a file as the project was taken and as it is now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffResponse {
    /// Project-relative path, `/`-separated.
    pub path: String,
    /// Commit the original side comes from (the project's start); absent — no start commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Text at the start; absent — the file did not exist, or is binary / too large.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
    /// Text now; absent — the file was deleted, or is binary / too large.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// A side is larger than 2 MiB.
    pub too_large: bool,
    /// A side is not UTF-8 text.
    pub binary: bool,
}

/// `GET /v1/graph?project=…[&view=folders]`: the map of a project — its parts by folder, and which of them
/// changed since the project was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GraphResponse {
    /// Who drew the map.
    pub source: GraphSource,
    /// Newest commit of the project; the map changes only with it. Absent — no commits yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Parts of the project.
    pub nodes: Vec<GraphNode>,
    /// A folder contains a part.
    pub edges: Vec<GraphEdge>,
}

/// A part of a project: a folder (or the files at the root).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    /// Folder path, `/`-separated; `.` — files at the root.
    pub id: String,
    /// Name to show; empty for the files no part of a model's map covers (id `other`).
    pub label: String,
    /// What the part does, one sentence (model's map only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Number of files in it (including deeper folders shown as their own parts).
    pub files: u32,
    /// Files in it added, changed or deleted since the project was taken.
    pub changed: u32,
}

/// `from` contains `to` (folders) or uses `to` (model's map).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    /// Containing part.
    pub from: String,
    /// Contained part.
    pub to: String,
}

/// Who drew a project map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GraphSource {
    /// A model, in plain words: parts by what they do.
    Model,
    /// The agent, by folders: no model has described the project yet.
    Folders,
}

/// Request of `POST /v1/graph/describe`: have a model draw the project's map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DescribeRequest {
    /// Project.
    pub project: String,
    /// Language of names and summaries (`en`, `ru`, …); default `en`.
    #[serde(default)]
    pub lang: Option<String>,
}

/// `GET /v1/ai`: whether AI is connected — the editor is not usable without it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiResponse {
    /// At least one way is connected (or cannot be told apart from connected).
    pub ready: bool,
    /// Ways to connect AI, in the order to offer them.
    pub providers: Vec<AiProvider>,
}

/// One way to connect AI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiProvider {
    /// `claude`, `codex`, `openrouter`.
    pub id: String,
    /// Paid subscription or free.
    pub kind: AiKind,
    /// Its CLI is installed (always true for a provider that needs none).
    pub installed: bool,
    /// The agent is installing its official CLI now.
    pub installing: bool,
    /// Signed in or a key is set.
    pub connected: AiConnected,
    /// Whether the user has an existing personal login in their personal home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_signed_in: Option<bool>,
    /// Current CLI home mode (agent or user).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<CliHomeMode>,
    /// Host label for custom endpoints (e.g. "localhost:11434").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

/// Where a CLI keeps its sign-in, settings, and sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CliHomeMode {
    /// Isolated home inside Vibefork (`state/tools/cli-home/<provider>`).
    #[default]
    Agent,
    /// User's personal home (`~/.claude` or `~/.codex`).
    User,
}

/// What a way to connect AI costs the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiKind {
    /// The user's own subscription (Claude, ChatGPT).
    Subscription,
    /// Free models.
    Free,
    /// Custom OpenAI-compatible API.
    Custom,
}

/// Whether a way to connect AI is set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiConnected {
    /// Ready to use.
    Yes,
    /// Not signed in / no key.
    No,
    /// Installed; the agent cannot tell whether the user is signed in.
    Unknown,
}

/// Request of `POST /v1/ai/connect`: sign in to a subscription through its official CLI (the
/// agent installs a pinned official release first when the CLI is missing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiConnectRequest {
    /// `claude` or `codex`.
    pub provider: String,
    /// Desired CLI home mode (`agent` or `user`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<CliHomeMode>,
}

/// Response of `POST /v1/ai/connect`: the sign-in opened in the browser; `GET /v1/ai` tells
/// when it is done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AiConnectResponse {
    /// The sign-in was started (false: one is already running).
    pub started: bool,
}

/// Request of `POST /v1/ai/disconnect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiDisconnectRequest {
    /// Provider to disconnect (`custom`, `openrouter`).
    pub provider: String,
}

/// Response of `POST /v1/ai/disconnect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AiDisconnectResponse {
    /// Disconnect succeeded.
    pub ok: bool,
}
