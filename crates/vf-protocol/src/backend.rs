//! The port between the API (`vf-server`) and what does the work (`vf-engine`). Calls are blocking:
//! the server runs them on blocking threads.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::canvas::{CanvasResponse, NoteAttachment};
use crate::http::{
    AiResponse, DescribeRequest, EnvironmentResponse, FileDiffResponse, FileResponse,
    GithubConnectResponse, GithubStatusResponse, GraphResponse, ImportProgressResponse,
    ImportRequest, ImportResponse, JournalResponse, ModelsResponse, ProjectFilesResponse,
    ProjectsResponse, RollbackResponse, SaveRequest, SaveResponse,
};
use crate::note::{MAX_PENDING_NOTES, MAX_TASK_NOTES, Note, NoteRejected};
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
    /// Request limit reached (`429`).
    RateLimited {
        /// Reset time in minutes.
        reset_minutes: u64,
    },
    /// Anything else (`500`).
    Internal(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(m) | Self::NotFound(m) | Self::Forbidden(m) | Self::Internal(m) => {
                f.write_str(m)
            }
            Self::RateLimited { reset_minutes } => {
                write!(
                    f,
                    "GitHub's limit of requests is reached; connect GitHub or try again in {reset_minutes} min"
                )
            }
        }
    }
}

/// Why an offer of the analysis cannot start a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferRejected {
    /// The project has no offers kept (it was never described, or the model offered nothing).
    NoOffers,
    /// No offer has this index, or it has another title than the one shown.
    BadIndex,
    /// The backend does not run offers.
    NotSupported,
}

impl OfferRejected {
    /// The error code sent to the browser.
    pub fn code(self) -> &'static str {
        match self {
            Self::NoOffers => "no_offers",
            Self::BadIndex => "bad_index",
            Self::NotSupported => "not_supported",
        }
    }
}

impl std::fmt::Display for OfferRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoOffers => "this project has no offers: look at its map first",
            Self::BadIndex => "there is no such offer",
            Self::NotSupported => "not supported by this version of the agent",
        })
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
    /// Language of the UI/response (`en`, `ru`, …); default is the OS language.
    pub lang: Option<String>,
}

/// What the person asked of a running task.
#[derive(Debug, Default)]
struct Control {
    cancelled: bool,
    paused: bool,
    undo: bool,
    /// Messages of the person waiting to be taken.
    notes: VecDeque<Note>,
    /// Messages queued so far (the number of the last one).
    posted: u32,
}

/// Cancellation, pause and messages of the person to a running task, shared between the requester
/// and the worker.
///
/// Work in progress (a model, a build, a call to an AI) stops when [`Cancel::is_cancelled`] turns
/// true: on cancel, and also on pause. A paused task waits in [`Cancel::wait_resumed`] and runs the
/// interrupted step again once resumed.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<(Mutex<Control>, Condvar)>);

impl Cancel {
    /// A new, not cancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    fn set(&self, f: impl FnOnce(&mut Control)) {
        let (state, cv) = &*self.0;
        if let Ok(mut s) = state.lock() {
            f(&mut s);
        }
        cv.notify_all();
    }

    fn read<T>(&self, f: impl FnOnce(&Control) -> T, poisoned: T) -> T {
        self.0.0.lock().map(|s| f(&s)).unwrap_or(poisoned)
    }

    /// Requests cancellation; what the task changed stays.
    pub fn cancel(&self) {
        self.set(|s| s.cancelled = true);
    }

    /// Requests cancellation and that the task's changes are undone.
    pub fn cancel_and_undo(&self) {
        self.set(|s| {
            s.cancelled = true;
            s.undo = true;
        });
    }

    /// Pauses the task: the work in progress stops at once.
    pub fn pause(&self) {
        self.set(|s| s.paused = true);
    }

    /// Continues a paused task.
    pub fn resume(&self) {
        self.set(|s| s.paused = false);
    }

    /// Whether the work in progress must stop now (cancelled or paused).
    pub fn is_cancelled(&self) -> bool {
        self.read(|s| s.cancelled || s.paused, true)
    }

    /// Whether the task is paused (and not cancelled).
    pub fn is_paused(&self) -> bool {
        self.read(|s| s.paused && !s.cancelled, false)
    }

    /// Whether the task's changes must be undone.
    pub fn wants_undo(&self) -> bool {
        self.read(|s| s.cancelled && s.undo, false)
    }

    /// Queues a message of the person for the task; returns its number within the task. The message
    /// is checked against the bounds of [`crate::note`] first.
    pub fn post_note(
        &self,
        text: String,
        attachments: Vec<NoteAttachment>,
    ) -> Result<u32, NoteRejected> {
        crate::note::check(&text, &attachments)?;
        let Ok(mut s) = self.0.0.lock() else {
            return Err(NoteRejected::NotRunning);
        };
        if s.cancelled {
            return Err(NoteRejected::NotRunning);
        }
        if s.notes.len() >= MAX_PENDING_NOTES {
            return Err(NoteRejected::QueueFull);
        }
        if s.posted >= MAX_TASK_NOTES {
            return Err(NoteRejected::TooManyNotes);
        }
        s.posted = s.posted.saturating_add(1);
        let id = s.posted;
        s.notes.push_back(Note {
            id,
            text,
            attachments,
        });
        Ok(id)
    }

    /// Takes the queued messages, oldest first.
    pub fn take_notes(&self) -> Vec<Note> {
        self.0
            .0
            .lock()
            .map(|mut s| s.notes.drain(..).collect())
            .unwrap_or_default()
    }

    /// Waits up to `timeout`; returns `true` when the work must stop (cancelled or paused).
    pub fn wait(&self, timeout: Duration) -> bool {
        let (state, cv) = &*self.0;
        let Ok(guard) = state.lock() else { return true };
        match cv.wait_timeout_while(guard, timeout, |s| !s.cancelled && !s.paused) {
            Ok((s, _)) => s.cancelled || s.paused,
            Err(_) => true,
        }
    }

    /// Waits while the task is paused; `true` when it was resumed, `false` when cancelled.
    pub fn wait_resumed(&self) -> bool {
        let (state, cv) = &*self.0;
        let Ok(guard) = state.lock() else {
            return false;
        };
        match cv.wait_while(guard, |s| s.paused && !s.cancelled) {
            Ok(s) => !s.cancelled,
            Err(_) => false,
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
    /// Runs the sign-in of a subscription's official CLI to the end (minutes; the user finishes
    /// it in the browser). The caller has confirmed it with the user.
    fn connect_ai(&self, provider: &str) -> Result<(), BackendError>;
    /// Changes the CLI home mode for a provider (`agent` or `user`).
    fn set_cli_home_mode(
        &self,
        provider: &str,
        mode: crate::http::CliHomeMode,
    ) -> Result<(), BackendError> {
        let _ = (provider, mode);
        Err(BackendError::BadRequest(
            "this provider does not support cli home modes".into(),
        ))
    }
    /// Connects a provider using an API key (e.g. OpenRouter).
    fn connect_ai_key(&self, provider: &str, key: &str) -> Result<(), BackendError> {
        let _ = (provider, key);
        Err(BackendError::BadRequest(
            "this provider does not use an API key".into(),
        ))
    }
    /// Checks custom endpoint reachability and returns models if available from `/models`.
    fn verify_ai_custom(
        &self,
        endpoint: &str,
        key: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<String>, BackendError> {
        let _ = (endpoint, key, model);
        Err(BackendError::BadRequest(
            "custom OpenAI-compatible API not supported".into(),
        ))
    }
    /// Connects a custom OpenAI-compatible API endpoint with optional key and optional model.
    fn connect_ai_custom(
        &self,
        endpoint: &str,
        key: Option<&str>,
        model: Option<&str>,
    ) -> Result<(), BackendError> {
        let _ = (endpoint, key, model);
        Err(BackendError::BadRequest(
            "custom OpenAI-compatible API not supported".into(),
        ))
    }
    /// Disconnects a provider (e.g. `custom`, `openrouter`).
    fn disconnect_ai(&self, provider: &str) -> Result<(), BackendError> {
        let _ = provider;
        Err(BackendError::BadRequest(
            "this provider cannot be disconnected".into(),
        ))
    }
    /// Runs a task to the end, reporting progress through `out`; the last message is
    /// [`ServerMessage::Done`], [`ServerMessage::Cancelled`] or [`ServerMessage::Error`].
    fn run_task(&self, task: TaskRequest, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel);
    /// What offer `index` of the project's kept offers (the last `offers` event) asks for: its title
    /// and text, ready to run as a task prompt. With `title`, the offer must have that title.
    fn offer_prompt(
        &self,
        project: &str,
        index: u32,
        title: Option<&str>,
    ) -> Result<String, OfferRejected> {
        let _ = (project, index, title);
        Err(OfferRejected::NotSupported)
    }
    /// The session stream of a project from event `from` on, at most
    /// [`crate::canvas::MAX_CANVAS_PAGE`] events (replay after the editor reloads).
    fn canvas(&self, project: &str, from: u64) -> Result<CanvasResponse, BackendError> {
        let _ = project;
        Ok(CanvasResponse {
            events: Vec::new(),
            next: from,
        })
    }
    /// Builds a project; the last message is [`ServerMessage::BuildDone`] or
    /// [`ServerMessage::Error`]. The caller has confirmed it with the user.
    fn build(&self, project: &str, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel);
    /// The commands [`Backend::build`] would run in `project`, as shown to the person before they
    /// allow it; empty when not known.
    fn build_commands(&self, project: &str) -> Vec<String> {
        let _ = project;
        Vec::new()
    }
    /// The command [`Backend::run_program`] would start in `project`; `None` when not known.
    fn run_command(&self, project: &str) -> Option<String> {
        let _ = project;
        None
    }
    /// Runs the project's program until it exits or `cancel`; its output goes out as
    /// [`ServerMessage::ProgramLog`] bounded by `vf_core::limits`, the last message is
    /// [`ServerMessage::ProgramDone`], [`ServerMessage::Cancelled`] or [`ServerMessage::Error`].
    /// The caller has confirmed it with the user.
    fn run_program(&self, project: &str, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel) {
        let _ = (project, cancel);
        out(ServerMessage::Error {
            error: "not supported by this version of the agent".into(),
            code: Some("not_supported".into()),
        });
    }
    /// Resets a project to a commit of its journal. The caller has confirmed it with the user.
    fn rollback(&self, project: &str, commit: &str) -> Result<RollbackResponse, BackendError>;
    /// Returns the files of one part of the map (node `id`) to how they were before the last task
    /// that changed them, as a checkpoint with a journal entry; other parts stay as they are. Only
    /// while no task runs in the project. The session stream gets a `change` and a `result` event;
    /// a failure is one [`ServerMessage::Error`]. The caller has confirmed it with the user.
    fn rollback_part(&self, project: &str, id: &str, out: &mut dyn FnMut(ServerMessage)) {
        let _ = (project, id);
        out(ServerMessage::Error {
            error: "not supported by this version of the agent".into(),
            code: Some("not_supported".into()),
        });
    }
    /// Starts the agent's own GitHub sign-in (device flow); the token comes to the agent directly.
    fn github_connect(&self) -> Result<GithubConnectResponse, BackendError>;
    /// The GitHub sign-in.
    fn github_status(&self) -> Result<GithubStatusResponse, BackendError>;
    /// Takes a project from GitHub (an archive of one commit; no git on the machine).
    fn import(&self, req: &ImportRequest) -> Result<ImportResponse, BackendError>;
    /// [`Backend::import`] for a browser session `owner`, whose progress
    /// [`Backend::import_progress`] reports to the same `owner` only.
    fn import_for(&self, req: &ImportRequest, owner: &str) -> Result<ImportResponse, BackendError> {
        let _ = owner;
        self.import(req)
    }
    /// Progress of the import of `repo` (`owner/name` or its URL) started by `owner`; `idle` when
    /// there is none.
    fn import_progress(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<ImportProgressResponse, BackendError> {
        let _ = (owner, repo);
        Ok(ImportProgressResponse::default())
    }
    /// The tools `project` needs and the disk space they take, without installing anything; without
    /// a project — every tool the agent can install.
    fn environment(&self, project: Option<&str>) -> Result<EnvironmentResponse, BackendError> {
        let _ = project;
        Err(BackendError::NotFound(
            "not supported by this version of the agent".into(),
        ))
    }
    /// Whether saving this project must be confirmed by the user first (first save).
    fn save_needs_confirm(&self, project: &str) -> bool;
    /// Saves the project's work into the user's copy on GitHub, or tells the next step.
    fn save(&self, req: &SaveRequest) -> Result<SaveResponse, BackendError>;
    /// A project file for the viewer (read-only; no git data, no secret files, at most 2 MiB).
    fn read_file(&self, project: &str, path: &str) -> Result<FileResponse, BackendError>;
    /// The files of a project for the editor's file tree, bounded by `vf_core::limits`.
    fn project_files(&self, project: &str) -> Result<ProjectFilesResponse, BackendError> {
        let _ = project;
        Err(BackendError::NotFound(
            "not supported by this version of the agent".into(),
        ))
    }
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
        assert!(!c.wants_undo());
    }

    #[test]
    fn a_pause_stops_the_work_and_waits_for_resume_or_cancel() {
        let c = Cancel::new();
        c.pause();
        assert!(c.is_cancelled() && c.is_paused());
        assert!(c.wait(Duration::from_millis(1)));
        let c2 = c.clone();
        let t = std::thread::spawn(move || c2.wait_resumed());
        c.resume();
        assert!(t.join().unwrap_or(false));
        assert!(!c.is_cancelled() && !c.is_paused());

        c.pause();
        let c2 = c.clone();
        let t = std::thread::spawn(move || c2.wait_resumed());
        c.cancel_and_undo();
        assert!(!t.join().unwrap_or(true));
        assert!(c.is_cancelled() && !c.is_paused() && c.wants_undo());
    }

    #[test]
    fn notes_are_numbered_bounded_and_taken_once() {
        let c = Cancel::new();
        assert_eq!(c.post_note("one".into(), Vec::new()), Ok(1));
        assert_eq!(c.post_note("two".into(), Vec::new()), Ok(2));
        assert_eq!(
            c.post_note(" ".into(), Vec::new()),
            Err(NoteRejected::Empty)
        );
        let taken = c.take_notes();
        assert_eq!(
            taken
                .iter()
                .map(|n| (n.id, n.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "one"), (2, "two")]
        );
        assert!(c.take_notes().is_empty());

        for _ in 2..MAX_PENDING_NOTES + 2 {
            assert!(c.post_note("x".into(), Vec::new()).is_ok());
        }
        assert_eq!(
            c.post_note("x".into(), Vec::new()),
            Err(NoteRejected::QueueFull)
        );
        c.take_notes();
        while c.post_note("x".into(), Vec::new()).is_ok() {}
        assert_eq!(
            c.post_note("x".into(), Vec::new()),
            Err(NoteRejected::TooManyNotes)
        );

        let c = Cancel::new();
        c.cancel();
        assert_eq!(
            c.post_note("late".into(), Vec::new()),
            Err(NoteRejected::NotRunning)
        );
    }
}
