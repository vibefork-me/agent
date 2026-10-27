//! WebSocket messages on `/v1/ws`. The first client message is [`ClientMessage::Auth`] (within 5 s).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::event::Event;
use crate::http::ImportStatus;
use crate::model::ModelInfo;

/// Messages from the browser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Authenticate the connection with a session token.
    Auth {
        /// Bearer token from `POST /v1/auth/session`.
        token: String,
    },
    /// Run a task in a project.
    Task {
        /// Project name.
        project: String,
        /// What to do.
        prompt: String,
        /// `auto`, `plan`, or a provider: `claude`, `codex`, `openrouter`.
        #[serde(default)]
        provider: Option<String>,
        /// Model of the provider; empty = its default.
        #[serde(default)]
        model: Option<String>,
        /// Continue with the history of the project.
        #[serde(default, rename = "continue")]
        continue_: bool,
        /// Language of the UI/response (`en`, `ru`, …); default is the OS language.
        #[serde(default)]
        lang: Option<String>,
    },
    /// Build the project (confirmed in the native dialog).
    Build {
        /// Project name.
        project: String,
    },
    /// Cancel the running task or build.
    Cancel,
}

/// Rating of a task by the Auto mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rating {
    /// `simple`, `medium`, `hard`.
    pub complexity: String,
    /// The task can be split into steps.
    pub decompose: bool,
    /// One short sentence.
    pub reason: String,
    /// Model that rated the task, or `heuristic`.
    pub classifier: String,
}

/// A plan step as shown to the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanStepInfo {
    /// Step title.
    pub title: String,
    /// `simple`, `medium`, `hard`.
    pub complexity: String,
    /// Model label chosen for the step.
    pub label: String,
}

/// Position of a step in a plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StepInfo {
    /// 1-based number.
    pub n: u32,
    /// Number of steps.
    pub of: u32,
    /// Title.
    pub title: String,
}

/// Toolchain preparation progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolchainStatus {
    /// Installing.
    Installing,
    /// Downloading installer or archive.
    Downloading,
    /// Extracting archive files.
    Extracting,
    /// Ready.
    Ready,
    /// Failed.
    Failed,
}

/// Messages from the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// The connection is authenticated.
    Authed {
        /// Paired key.
        #[serde(rename = "keyId")]
        key_id: String,
    },
    /// Echo of a text message (connectivity check).
    Echo {
        /// The message.
        data: String,
    },
    /// Auto mode rated the task and chose a model.
    Routing {
        /// Rating.
        rating: Rating,
        /// Chosen model.
        choice: ModelInfo,
    },
    /// A plan is being made.
    Planning {
        /// Rating, when planning was chosen by Auto mode.
        rating: Option<Rating>,
    },
    /// The plan.
    Plan {
        /// Model that planned.
        planner: String,
        /// Cost of planning, USD (API prices).
        #[serde(rename = "costUsd")]
        cost_usd: Option<f64>,
        /// Steps.
        steps: Vec<PlanStepInfo>,
    },
    /// Toolchain preparation.
    Toolchain {
        /// Status.
        status: ToolchainStatus,
        /// Tool, e.g. `java@temurin-21`.
        tool: String,
        /// Downloaded bytes so far (if known).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        downloaded: Option<u64>,
        /// Total bytes to download (if known).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
        /// Estimated seconds remaining.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        eta: Option<f64>,
        /// Install path when ready.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        /// Seconds spent, when ready.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seconds: Option<String>,
        /// Error, when failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Project import progress.
    ImportProgress {
        /// Status.
        status: ImportStatus,
        /// Repo being imported.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repo: Option<String>,
        /// Downloaded bytes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        downloaded: Option<u64>,
        /// Total bytes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
        /// Speed in bytes per second.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speed: Option<f64>,
        /// ETA in seconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        eta: Option<f64>,
        /// Error message if failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// A step (or a single task) started.
    Started {
        /// Project.
        project: String,
        /// Provider.
        provider: String,
        /// Model.
        model: String,
        /// Native resume of the provider's session.
        resume: bool,
        /// Context handed over from the journal.
        handoff: bool,
        /// Toolchain folders put on PATH.
        toolchain: Vec<String>,
        /// Plan position, for plan steps.
        step: Option<StepInfo>,
        /// Isolation of the model's processes: `none`, `weak`, `os`, `micro_vm`.
        #[serde(default)]
        isolation: String,
    },
    /// A normalized provider event.
    Evt {
        /// Provider.
        provider: String,
        /// The event.
        ev: Event,
    },
    /// Raw stderr of a provider CLI.
    Stderr {
        /// Text.
        text: String,
    },
    /// A plan step finished.
    #[serde(rename = "step-done")]
    StepDone {
        /// Step number.
        n: u32,
        /// Number of steps.
        of: u32,
        /// Success.
        ok: bool,
        /// Newest commit of the step.
        commit: Option<String>,
    },
    /// The task was cancelled.
    Cancelled,
    /// The task finished.
    Done {
        /// Exit code (0 = success).
        code: i32,
        /// Provider of the last step.
        provider: Option<String>,
        /// Model of the last step.
        model: Option<String>,
        /// Provider session id.
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
        /// Newest commit.
        commit: Option<String>,
        /// Diff of the whole task.
        diff: String,
        /// Changed files summary.
        status: String,
        /// The task ran as a plan.
        #[serde(default)]
        plan: bool,
    },
    /// Whether the machine will manage the build.
    Estimate {
        /// `ok`, `tight`, `not_enough`.
        verdict: String,
        /// Explanation.
        text: String,
    },
    /// A line of build output.
    BuildLog {
        /// Text.
        text: String,
    },
    /// The build finished.
    BuildDone {
        /// Exit code (0 = success).
        code: i32,
        /// Folder with the kept outputs.
        artifacts: Option<String>,
        /// Output files, relative to `artifacts`.
        files: Vec<String>,
        /// Seconds spent.
        seconds: f64,
    },
    /// An error; for auth errors the connection is closed after it.
    Error {
        /// Reason.
        error: String,
        /// Machine-readable error code.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
}

/// Close code for failed or missing authentication.
pub const CLOSE_AUTH_FAILED: u16 = 4401;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names() -> Result<(), serde_json::Error> {
        let m: ClientMessage =
            serde_json::from_str(r#"{"type":"task","project":"p","prompt":"x","continue":true}"#)?;
        assert!(matches!(
            m,
            ClientMessage::Task {
                continue_: true,
                ..
            }
        ));
        let done = ServerMessage::StepDone {
            n: 1,
            of: 2,
            ok: true,
            commit: None,
        };
        assert!(serde_json::to_string(&done)?.starts_with(r#"{"type":"step-done""#));
        Ok(())
    }
}
