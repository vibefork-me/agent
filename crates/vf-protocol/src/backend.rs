//! The port between the API (`vf-server`) and what does the work (`vf-engine`). Calls are blocking:
//! the server runs them on blocking threads.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::http::{
    AiResponse, DescribeRequest, FileDiffResponse, FileResponse, GithubConnectResponse,
    GithubStatusResponse, GraphResponse, ImportRequest, ImportResponse, JournalResponse,
    ModelsResponse, ProjectsResponse, RollbackResponse, SaveRequest, SaveResponse,
};
use crate::ws::ServerMessage;

/// Why a call failed; maps to an HTTP status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// Bad input (`400`).
    BadRequest(String),
    /// No such thing (`404`).
    NotFound(String),
    /// Not allowed (`403`).
    Forbidden(String),
    /// Anything else (`500`).
    Internal(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(m) | Self::NotFound(m) | Self::Forbidden(m) | Self::Internal(m) => {
                f.write_str(m)
            }
        }
    }
}

/// A task to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRequest {
    /// Project name.
    pub project: String,
    /// What to do.
    pub prompt: String,
    /// `auto`, `plan`, or a provider; `None` = `auto`.
    pub provider: Option<String>,
    /// Model; `None` or empty = the provider's default.
    pub model: Option<String>,
    /// Continue with the history of the project.
    pub continue_: bool,
}

/// Cancellation of a running task, shared between the requester and the worker.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<(Mutex<bool>, Condvar)>);

impl Cancel {
    /// A new, not cancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        let (flag, cv) = &*self.0;
        if let Ok(mut f) = flag.lock() {
            *f = true;
        }
        cv.notify_all();
    }

    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.0.lock().map(|f| *f).unwrap_or(true)
    }

    /// Waits up to `timeout`; returns `true` when cancelled.
    pub fn wait(&self, timeout: Duration) -> bool {
        let (flag, cv) = &*self.0;
        let Ok(guard) = flag.lock() else { return true };
        match cv.wait_timeout_while(guard, timeout, |f| !*f) {
            Ok((f, _)) => *f,
            Err(_) => true,
        }
    }
}

/// What the API needs from the engine.
pub trait Backend: Send + Sync {
    /// Projects in the projects folder.
    fn projects(&self) -> Result<ProjectsResponse, BackendError>;
    /// Journal of a project.
    fn journal(&self, project: &str) -> Result<JournalResponse, BackendError>;
    /// Models available now (installed and logged-in CLIs, configured API keys).
    fn models(&self) -> Result<ModelsResponse, BackendError>;
    /// Whether AI is connected, per way to connect it.
    fn ai(&self) -> Result<AiResponse, BackendError>;
    /// Runs a task to the end, reporting progress through `out`; the last message is
    /// [`ServerMessage::Done`], [`ServerMessage::Cancelled`] or [`ServerMessage::Error`].
    fn run_task(&self, task: TaskRequest, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel);
    /// Builds a project; the last message is [`ServerMessage::BuildDone`] or
    /// [`ServerMessage::Error`]. The caller has confirmed it with the user.
    fn build(&self, project: &str, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel);
    /// Resets a project to a commit of its journal. The caller has confirmed it with the user.
    fn rollback(&self, project: &str, commit: &str) -> Result<RollbackResponse, BackendError>;
    /// Starts the agent's own GitHub sign-in (device flow); the token comes to the agent directly.
    fn github_connect(&self) -> Result<GithubConnectResponse, BackendError>;
    /// The GitHub sign-in.
    fn github_status(&self) -> Result<GithubStatusResponse, BackendError>;
    /// Takes a project from GitHub (an archive of one commit; no git on the machine).
    fn import(&self, req: &ImportRequest) -> Result<ImportResponse, BackendError>;
    /// Whether saving this project must be confirmed by the user first (first save).
    fn save_needs_confirm(&self, project: &str) -> bool;
    /// Saves the project's work into the user's copy on GitHub, or tells the next step.
    fn save(&self, req: &SaveRequest) -> Result<SaveResponse, BackendError>;
    /// A project file for the viewer (read-only; no git data, no secret files, at most 2 MiB).
    fn read_file(&self, project: &str, path: &str) -> Result<FileResponse, BackendError>;
    /// A project file as it was taken and as it is now, for the viewer's diff.
    fn read_diff(&self, project: &str, path: &str) -> Result<FileDiffResponse, BackendError>;
    /// The map of a project: its parts by folder and what changed since it was taken.
    /// `folders` — the folder map even when a model drew one.
    fn graph(&self, project: &str, folders: bool) -> Result<GraphResponse, BackendError>;
    /// Has a model available now draw the project's map in plain words; slow (up to minutes).
    fn describe_graph(&self, req: &DescribeRequest) -> Result<GraphResponse, BackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_wakes_waiters() {
        let c = Cancel::new();
        assert!(!c.wait(Duration::from_millis(1)));
        let c2 = c.clone();
        let t = std::thread::spawn(move || c2.wait(Duration::from_secs(10)));
        c.cancel();
        assert!(t.join().unwrap_or(false));
        assert!(c.is_cancelled());
    }
}
