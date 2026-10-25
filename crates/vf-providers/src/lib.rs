//! Coding models the agent can drive, behind one [`Provider`] trait.
//!
//! Adapters:
//! - [`claude`] — the user's Claude Code (subscription), headless `-p --output-format stream-json`,
//!   without the user's personal environment (invariant 7);
//! - [`codex`] — the user's Codex CLI (ChatGPT subscription), `exec --json`, `--ignore-user-config`;
//! - [`openai`] — the agent's own coding loop over an OpenAI-compatible API (OpenRouter) — works
//!   with nothing installed but a key.
//!
//! Every adapter turns its raw output into [`vf_protocol::Event`]s. The task prompt goes to CLIs
//! through stdin, never through the command line.

pub mod claude;
mod cli;
pub mod codex;
pub mod env;
pub mod openai;
pub mod process;
mod tools;

use std::path::Path;

use vf_protocol::{Cancel, Event, ModelInfo, Usage};

pub use env::ChildEnv;

/// Output of a running provider.
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    /// A normalized event.
    Event(Event),
    /// A line of a CLI's stderr.
    Stderr(String),
}

/// One run of a model in a project folder.
#[derive(Debug, Clone, Copy)]
pub struct RunRequest<'a> {
    /// Project folder (working directory).
    pub dir: &'a Path,
    /// Full prompt (with handoff or plan preamble).
    pub prompt: &'a str,
    /// Model; empty = the provider's default.
    pub model: &'a str,
    /// Provider session to resume.
    pub resume: Option<&'a str>,
    /// Environment of child processes.
    pub env: &'a ChildEnv,
}

/// How a run ended.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunResult {
    /// Exit code (0 = success).
    pub code: i32,
    /// The model reported success.
    pub ok: bool,
    /// Stopped by cancel.
    pub cancelled: bool,
    /// Provider session id (for a later resume).
    pub session: Option<String>,
    /// Final message.
    pub summary: String,
    /// Duration, milliseconds.
    pub ms: Option<u64>,
    /// Cost, USD.
    pub cost_usd: Option<f64>,
    /// Tokens.
    pub usage: Option<Usage>,
}

/// A one-shot, read-only question (planning, rating a task).
#[derive(Debug, Clone, Copy)]
pub struct AskRequest<'a> {
    /// Project folder the model may read.
    pub dir: &'a Path,
    /// System instruction, if any.
    pub system: Option<&'a str>,
    /// The question.
    pub prompt: &'a str,
    /// Model; empty = the provider's default.
    pub model: &'a str,
    /// Environment of child processes.
    pub env: &'a ChildEnv,
    /// Longest answer, tokens (API providers).
    pub max_tokens: Option<u32>,
    /// How long to wait.
    pub timeout: std::time::Duration,
}

/// The answer to an [`AskRequest`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Answer {
    /// Final text.
    pub text: String,
    /// Cost, USD.
    pub cost_usd: Option<f64>,
}

/// A source of coding models (a CLI or an API).
pub trait Provider: Send + Sync {
    /// Provider id: `claude`, `codex`, `openrouter`.
    fn id(&self) -> &'static str;
    /// Models available now (CLI found and logged in, key present…).
    fn models(&self) -> Vec<ModelInfo>;
    /// Whether a native session can be resumed.
    fn can_resume(&self) -> bool;
    /// Runs the model to the end.
    fn run(&self, req: &RunRequest<'_>, out: &mut dyn FnMut(Output), cancel: &Cancel) -> RunResult;
    /// Asks a read-only question; the model may read the project but not change it.
    fn ask(&self, req: &AskRequest<'_>, cancel: &Cancel) -> Result<Answer, String>;
    /// Runs the provider's own sign-in (its official CLI opens the browser); blocks until the
    /// user finishes it or `timeout` passes.
    fn connect(&self, timeout: std::time::Duration) -> Result<(), String> {
        let _ = timeout;
        Err("this provider is connected another way".into())
    }
    /// Whether the provider is set up: its CLI is installed (or it needs none) and the user is
    /// signed in (`None` — the agent cannot tell).
    fn status(&self) -> Status {
        let ok = !self.models().is_empty();
        Status {
            installed: ok,
            signed_in: Some(ok),
        }
    }
}

/// Whether a provider is set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Status {
    /// The CLI is installed, or the provider needs none.
    pub installed: bool,
    /// The user is signed in or a key is present; `None` — unknown.
    pub signed_in: Option<bool>,
}

/// Collects what a run reports through its events.
#[derive(Debug, Default)]
pub struct Tracker {
    result: RunResult,
    got_result: bool,
}

impl Tracker {
    /// Starts with the session being resumed, if any.
    pub fn new(resume: Option<&str>) -> Self {
        Self {
            result: RunResult {
                session: resume.map(str::to_owned),
                ..RunResult::default()
            },
            got_result: false,
        }
    }

    /// Looks at one event.
    pub fn observe(&mut self, ev: &Event) {
        match ev {
            Event::Init {
                session: Some(s), ..
            } => self.result.session = Some(s.clone()),
            Event::Text { text } => self.result.summary = text.clone(),
            Event::Result {
                ok,
                text,
                ms,
                cost_usd,
                usage,
                ..
            } => {
                self.got_result = true;
                self.result.ok = *ok;
                if !text.is_empty() {
                    self.result.summary = text.clone();
                }
                self.result.ms = ms.or(self.result.ms);
                self.result.cost_usd = cost_usd.or(self.result.cost_usd);
                self.result.usage = usage.clone().or(self.result.usage.take());
            }
            _ => {}
        }
    }

    /// The result for an exit code.
    pub fn finish(mut self, code: i32, cancelled: bool) -> RunResult {
        self.result.code = code;
        self.result.cancelled = cancelled;
        self.result.ok = self.got_result && self.result.ok && code == 0 && !cancelled;
        self.result
    }
}

/// Truncates text to `max` characters.
pub(crate) fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}
