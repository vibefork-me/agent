//! Per-project journal, checkpoints and customizations — the provider-independent history that makes
//! "continue in another model" and fork sync possible. **Layer 1: no IO** — storage and git are
//! the [`JournalStore`] and [`Vcs`] traits, implemented in adapter crates.
//!
//! A step (a task or one plan step):
//! 1. [`StepStart::begin`] remembers `HEAD`;
//! 2. the model works (and may commit by itself);
//! 3. [`StepStart::finish`] commits what is left as a checkpoint with trailers, collects **all commits
//!    since the start**, the diff from the start, and appends a journal entry.

pub mod customizations;
pub mod handoff;
pub mod timing;

use vf_protocol::Usage;
use vf_protocol::canvas::CanvasRecord;
use vf_protocol::http::JournalEntry;

pub use handoff::handoff_preamble;

/// Commit trailer linking commits to a customization (`CUSTOMIZATIONS.md`, fork sync).
pub const CUSTOMIZATION_TRAILER: &str = "Vibefork-Customization";

/// Errors of git or storage, as text (adapters decide the details).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct JournalError(pub String);

/// Version control of a project folder (implemented by `vf-git`).
pub trait Vcs {
    /// Current `HEAD` commit (full hash).
    fn head(&self) -> Result<String, JournalError>;
    /// Diff of the working tree (including new files) against `base`.
    fn diff_from(&self, base: &str) -> Result<String, JournalError>;
    /// Commits everything; `None` when there was nothing to commit. Returns the short hash.
    fn commit_all(
        &self,
        message: &str,
        trailers: &[(&str, &str)],
    ) -> Result<Option<String>, JournalError>;
    /// Short hashes of commits in `base..head`, newest first.
    fn commits_between(&self, base: &str, head: &str) -> Result<Vec<String>, JournalError>;
    /// Number of files changed in `base..head`.
    fn files_changed(&self, base: &str, head: &str) -> Result<usize, JournalError>;
}

/// Where journal entries are kept (implemented in `vf-platform`: JSONL per project).
pub trait JournalStore {
    /// All entries of a project, oldest first.
    fn read(&self, project: &str) -> Result<Vec<JournalEntry>, JournalError>;
    /// Appends an entry.
    fn append(&self, project: &str, entry: &JournalEntry) -> Result<(), JournalError>;
}

/// Where the session stream of a project is kept (implemented in `vf-platform`: JSONL per project).
/// Records are appended in `seq` order; `seq` starts at 1 and has no gaps.
pub trait CanvasLog {
    /// The `seq` of the newest record, 0 when there is none.
    fn last_seq(&self, project: &str) -> Result<u64, JournalError>;
    /// Appends a record.
    fn append(&self, project: &str, record: &CanvasRecord) -> Result<(), JournalError>;
    /// Records with `seq ≥ from`, oldest first, at most `limit`.
    fn read_from(
        &self,
        project: &str,
        from: u64,
        limit: usize,
    ) -> Result<Vec<CanvasRecord>, JournalError>;
}

/// What a finished step reports.
#[derive(Debug, Clone, PartialEq)]
pub struct StepOutcome {
    /// The appended entry.
    pub entry: JournalEntry,
    /// Diff of the step (from its start to the checkpoint).
    pub diff: String,
}

/// Facts about a step, known when it finishes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepReport {
    /// Provider (`claude`, `codex`, `openrouter`, `planner`).
    pub provider: String,
    /// Model.
    pub model: String,
    /// Prompt or `[step n/m] title`.
    pub prompt: String,
    /// Label of the checkpoint commit.
    pub label: String,
    /// Customization the commits belong to.
    pub customization: Option<String>,
    /// Provider session id.
    pub session: Option<String>,
    /// Context came from the journal.
    pub handoff: bool,
    /// Final message of the model.
    pub summary: String,
    /// Duration, milliseconds.
    pub ms: Option<u64>,
    /// Cost reported by the provider, USD.
    pub cost_usd: Option<f64>,
    /// Token usage.
    pub usage: Option<Usage>,
    /// Prompt version used, if applicable (e.g. `loop:1`).
    pub prompt_version: Option<String>,
    /// What the step produced (kept with its journal entry).
    pub artifacts: Vec<vf_protocol::canvas::Artifact>,
}

/// A started step: remembers where it began.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepStart {
    head: String,
}

impl StepStart {
    /// Remembers `HEAD` before the model starts.
    pub fn begin(vcs: &dyn Vcs) -> Result<Self, JournalError> {
        Ok(Self { head: vcs.head()? })
    }

    /// The commit the step started from.
    pub fn base(&self) -> &str {
        &self.head
    }

    /// Checkpoints what is left, collects all commits since the start and appends the journal entry.
    pub fn finish(
        self,
        vcs: &dyn Vcs,
        store: &dyn JournalStore,
        project: &str,
        at: String,
        report: StepReport,
    ) -> Result<StepOutcome, JournalError> {
        let diff = vcs.diff_from(&self.head)?;
        let mut message = format!(
            "vibefork: {}",
            report.label.chars().take(60).collect::<String>()
        );
        message.push_str(&format!(
            " [{}{}]",
            report.provider,
            if report.model.is_empty() {
                String::new()
            } else {
                format!("/{}", report.model)
            }
        ));
        let trailers: Vec<(&str, &str)> = report
            .customization
            .as_deref()
            .map(|id| vec![(CUSTOMIZATION_TRAILER, id)])
            .unwrap_or_default();
        vcs.commit_all(&message, &trailers)?;
        let head = vcs.head()?;
        let commits = if head == self.head {
            Vec::new()
        } else {
            vcs.commits_between(&self.head, &head)?
        };
        let files = if commits.is_empty() {
            0
        } else {
            vcs.files_changed(&self.head, &head)?
        };
        let entry = JournalEntry {
            at,
            provider: report.provider,
            model: report.model,
            prompt: report.prompt,
            session: report.session,
            handoff: report.handoff,
            summary: report.summary.chars().take(2000).collect(),
            commit: commits.first().cloned(),
            commits,
            files: if files == 0 {
                String::new()
            } else {
                format!("{files} files")
            },
            ms: report.ms,
            cost_usd: report.cost_usd,
            usage: report.usage,
            prompt_version: report.prompt_version,
            artifacts: report.artifacts,
        };
        store.append(project, &entry)?;
        Ok(StepOutcome { entry, diff })
    }
}
