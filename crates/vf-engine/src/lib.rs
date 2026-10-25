//! Runs the work behind the API: implements [`vf_protocol::Backend`] on top of the adapters
//! (projects folder, journal files, git, providers, toolchains).
//!
//! A task is one or more **steps**. A step runs one model in the project folder: context from the
//! journal (native resume with the same provider and model, otherwise a handoff preamble), a cleaned
//! environment, then a checkpoint and a journal entry (`vf-journal`).

mod build;
mod github;
mod orchestrate;
mod step;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use vf_git::Repo;
use vf_journal::JournalStore as _;
use vf_platform::{JournalFiles, ProjectError, Projects, Settings};
use vf_protocol::http::{
    GithubConnectResponse, GithubStatusResponse, ImportRequest, ImportResponse, JournalResponse,
    ModelsResponse, ProjectInfo, ProjectsResponse, RollbackResponse, SaveRequest, SaveResponse,
};
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, BackendError, Cancel, ModelInfo, TaskRequest};
use vf_providers::Provider;

use orchestrate::Task;
pub use step::{StepRequest, StepResult};

/// Longest accepted prompt, characters.
pub const MAX_PROMPT: usize = 8000;
/// Longest diff sent to the browser, bytes.
pub const MAX_DIFF: usize = 200_000;

/// The engine of one agent process.
pub struct Engine {
    projects: Projects,
    journal: JournalFiles,
    providers: Vec<Arc<dyn Provider>>,
    busy: Mutex<HashSet<String>>,
    simulate_clean_machine: bool,
    tools: PathBuf,
    caches: PathBuf,
    artifacts: PathBuf,
    sandbox_helper: Option<PathBuf>,
    state_dir: PathBuf,
    sign_in: github::SignInState,
}

impl Engine {
    /// An engine over the folders of `settings` with the given providers.
    pub fn new(settings: &Settings, providers: Vec<Arc<dyn Provider>>) -> Self {
        Self {
            projects: Projects::new(&settings.storage.projects),
            journal: JournalFiles::new(settings.journal_dir()),
            providers,
            busy: Mutex::new(HashSet::new()),
            simulate_clean_machine: settings.simulate_clean_machine,
            tools: settings.storage.tools.clone(),
            caches: settings.storage.caches.clone(),
            artifacts: settings.storage.artifacts.clone(),
            sandbox_helper: None,
            state_dir: settings.state_dir.clone(),
            sign_in: Default::default(),
        }
    }

    /// Runs the model's processes and builds in the sandbox, launched through `helper` (the agent
    /// binary, which has `sandbox-exec`). Without it nothing is isolated.
    pub fn with_sandbox(mut self, helper: PathBuf) -> Self {
        self.sandbox_helper = Some(helper);
        self
    }

    /// Every model available now.
    pub fn inventory(&self) -> Vec<ModelInfo> {
        self.providers.iter().flat_map(|p| p.models()).collect()
    }

    fn provider(&self, id: &str) -> Option<&Arc<dyn Provider>> {
        self.providers.iter().find(|p| p.id() == id)
    }

    /// Whether `provider` / `model` can run now; an empty model is the CLI's default.
    fn available(&self, inventory: &[ModelInfo], provider: &str, model: &str) -> bool {
        inventory
            .iter()
            .any(|m| m.provider == provider && m.model == model)
            || (model.is_empty()
                && provider != "openrouter"
                && inventory.iter().any(|m| m.provider == provider))
    }

    fn task(&self, task: TaskRequest, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel) {
        let error = |out: &mut dyn FnMut(ServerMessage), e: &str| {
            out(ServerMessage::Error {
                error: e.to_owned(),
            })
        };
        let dir = match self.projects.resolve(&task.project) {
            Ok(d) => d,
            Err(_) => return error(out, "unknown project"),
        };
        if task.prompt.trim().is_empty() || task.prompt.chars().count() > MAX_PROMPT {
            return error(out, &format!("prompt required (≤ {MAX_PROMPT} chars)"));
        }
        let repo = Repo::open(&dir);
        if !repo.is_ready()
            && let Err(e) = Repo::init(&dir)
        {
            return error(out, &format!("cannot prepare git: {e}"));
        }
        let provider = task.provider.clone().unwrap_or_else(|| "auto".into());
        let model = task.model.clone().unwrap_or_default();
        let inventory = self.inventory();
        let t = Task {
            project: &task.project,
            dir: &dir,
            repo: &repo,
            prompt: &task.prompt,
            continue_: task.continue_,
        };
        match provider.as_str() {
            "auto" => self.auto(&t, &inventory, out, cancel),
            "plan" => self.plan(&t, &inventory, None, out, cancel),
            _ => {
                if !self.available(&inventory, &provider, &model) {
                    let shown = if model.is_empty() {
                        "(default)"
                    } else {
                        &model
                    };
                    return error(out, &format!("model not available: {provider} {shown}"));
                }
                self.single(&t, &provider, &model, out, cancel);
            }
        }
    }

    /// One model for the whole task.
    pub(crate) fn single(
        &self,
        t: &Task<'_>,
        provider: &str,
        model: &str,
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) {
        let Some(p) = self.provider(provider).cloned() else {
            return out(ServerMessage::Error {
                error: format!("unknown provider {provider}"),
            });
        };
        let req = StepRequest {
            project: t.project,
            dir: t.dir,
            prompt: t.prompt,
            prefix: "",
            model,
            continue_: t.continue_,
            step: None,
        };
        let r = self.step(t.repo, p.as_ref(), &req, out, cancel);
        if r.cancelled {
            return out(ServerMessage::Cancelled);
        }
        out(ServerMessage::Done {
            code: r.code,
            provider: Some(provider.to_owned()),
            model: Some(model.to_owned()),
            session_id: r.session,
            commit: r.commit,
            diff: r.diff,
            status: r.status,
            plan: false,
        });
    }
}

fn project_error(e: ProjectError) -> BackendError {
    match e {
        ProjectError::InvalidName | ProjectError::NotFound => BackendError::NotFound(e.to_string()),
        ProjectError::Escapes => BackendError::BadRequest(e.to_string()),
        ProjectError::Io(m) => BackendError::Internal(m),
    }
}

impl Engine {
    /// One task, build or rollback per project at a time.
    fn exclusive(
        &self,
        project: &str,
        out: &mut dyn FnMut(ServerMessage),
        f: impl FnOnce(&mut dyn FnMut(ServerMessage)),
    ) {
        {
            let mut busy = self.busy.lock().unwrap_or_else(|p| p.into_inner());
            if !busy.insert(project.to_owned()) {
                return out(ServerMessage::Error {
                    error: "a task is already running in this project".into(),
                });
            }
        }
        f(out);
        self.busy
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(project);
    }
}

impl Backend for Engine {
    fn projects(&self) -> Result<ProjectsResponse, BackendError> {
        let projects = self.projects.list();
        let details = projects
            .iter()
            .map(|name| {
                let meta = self.load_meta(name).unwrap_or_default();
                ProjectInfo {
                    name: name.clone(),
                    source: meta.upstream,
                    original: meta.original,
                    commit: (!meta.base_sha.is_empty()).then_some(meta.base_sha),
                }
            })
            .collect();
        Ok(ProjectsResponse {
            root: self.projects.root().display().to_string(),
            projects,
            details,
        })
    }

    fn journal(&self, project: &str) -> Result<JournalResponse, BackendError> {
        self.projects.resolve(project).map_err(project_error)?;
        let entries = self
            .journal
            .read(project)
            .map_err(|e| BackendError::Internal(e.0))?;
        Ok(JournalResponse { entries })
    }

    fn models(&self) -> Result<ModelsResponse, BackendError> {
        Ok(ModelsResponse {
            models: self.inventory(),
        })
    }

    fn run_task(&self, task: TaskRequest, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel) {
        let project = task.project.clone();
        self.exclusive(&project, out, |out| self.task(task, out, cancel));
    }

    fn build(&self, project: &str, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel) {
        self.exclusive(project, out, |out| self.run_build(project, out, cancel));
    }

    fn github_connect(&self) -> Result<GithubConnectResponse, BackendError> {
        self.connect()
    }

    fn github_status(&self) -> Result<GithubStatusResponse, BackendError> {
        Ok(self.status())
    }

    fn import(&self, req: &ImportRequest) -> Result<ImportResponse, BackendError> {
        self.import_project(req)
    }

    fn save_needs_confirm(&self, project: &str) -> bool {
        self.needs_confirm(project)
    }

    fn save(&self, req: &SaveRequest) -> Result<SaveResponse, BackendError> {
        let mut out = |_| {};
        let mut result = Err(BackendError::BadRequest(
            "a task is running in this project".into(),
        ));
        self.exclusive(&req.project, &mut out, |_| result = self.save_project(req));
        result
    }

    fn rollback(&self, project: &str, commit: &str) -> Result<RollbackResponse, BackendError> {
        let mut out = |_| {};
        let mut result = Err(BackendError::BadRequest(
            "a task is running in this project".into(),
        ));
        self.exclusive(project, &mut out, |_| {
            result = self.reset_to(project, commit)
        });
        result
    }
}
