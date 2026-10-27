//! WebSocket messages on `/v1/ws`. The first client message is [`ClientMessage::Auth`] (within 5 s).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canvas::{CanvasEvent, NoteAttachment};
use crate::event::Event;
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
    /// Cancel the running task or build. While it is paused, its changes are undone as well.
    Cancel,
    /// A message to the agent while it works; it is taken into account at the next point that
    /// allows it (answered with `note.taken`).
    Note {
        /// Project name.
        project: String,
        /// The message.
        text: String,
        /// Attached files or long pasted text.
        #[serde(default)]
        attachments: Vec<NoteAttachment>,
    },
    /// Pause the running task or build at once.
    Stop,
    /// Continue a paused task or build.
    Resume,
    /// Start what an offer of the analysis suggests.
    ChooseOffer {
        /// Project name.
        project: String,
        /// Index in the last `offers` event.
        index: u32,
    },
    /// Undo the changes of one part.
    RollbackPart {
        /// Project name.
        project: String,
        /// Node id of the part.
        id: String,
    },
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
    /// The task is paused (`stop`): the step in progress was interrupted; `resume` runs it again,
    /// `cancel` ends the task and undoes its changes.
    Paused,
    /// The paused task goes on: the interrupted step runs again.
    Resumed,
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
    /// A numbered event of the project's session stream (see [`crate::canvas`]).
    Canvas {
        /// Project.
        project: String,
        /// Number within the project, from 1, without gaps.
        seq: u64,
        /// When, Unix time in milliseconds.
        at: u64,
        /// The run it belongs to.
        session: String,
        /// The event.
        ev: CanvasEvent,
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
