//! Runs the work behind the API: implements [`vf_protocol::Backend`] on top of the adapters
//! (projects folder, journal files, git, providers, toolchains).
//!
//! A task is one or more **steps**. A step runs one model in the project folder: context from the
//! journal (native resume with the same provider and model, otherwise a handoff preamble), a cleaned
//! environment, then a checkpoint and a journal entry (`vf-journal`).

mod build;
mod canvas;
mod changes;
mod explain;
mod files;
mod github;
mod graph;
mod map;
mod notes;
mod orchestrate;
mod program;
mod rollback;
mod step;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use vf_core::Secret;
use vf_git::Repo;
use vf_journal::{CanvasLog as _, JournalStore as _};
use vf_platform::{CanvasFiles, JournalFiles, ProjectError, Projects, Settings};
use vf_protocol::canvas::{CanvasEvent, CanvasResponse, MAX_CANVAS_PAGE, Stage};
use vf_protocol::http::{
    AiConnected, AiKind, AiProvider, AiResponse, DescribeRequest, EnvironmentResponse,
    FileDiffResponse, FileResponse, GithubConnectResponse, GithubStatusResponse, GraphResponse,
    ImportProgressResponse, ImportRequest, ImportResponse, JournalResponse, ModelsResponse,
    ProjectFilesResponse, ProjectInfo, ProjectsResponse, RollbackResponse, SaveRequest,
    SaveResponse,
};
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, BackendError, Cancel, ModelInfo, TaskRequest};
use vf_providers::Provider;

use orchestrate::Task;
pub use step::{StepRequest, StepResult};

/// How long a subscription sign-in may take (the user finishes it in the browser).
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

/// Longest accepted prompt, characters.
pub const MAX_PROMPT: usize = 8000;
/// Longest diff sent to the browser, bytes.
pub const MAX_DIFF: usize = 200_000;

/// The engine of one agent process.
pub struct Engine {
    projects: Projects,
    journal: JournalFiles,
    canvas: CanvasFiles,
    providers: Vec<Arc<dyn Provider>>,
    busy: Mutex<HashSet<String>>,
    simulate_clean_machine: bool,
    tools: PathBuf,
    caches: PathBuf,
    artifacts: PathBuf,
    sandbox_helper: Option<PathBuf>,
    state_dir: PathBuf,
    sign_in: github::SignInState,
    /// The last import of each browser session, for its progress.
    imports: github::Imports,
    /// CLIs being installed now.
    installing: Mutex<HashSet<String>>,
    /// Asks the person before tools are installed; without it (tests) tools are installed as
    /// needed.
    dialog: Option<Arc<dyn vf_core::ConfirmDialog>>,
}

impl Engine {
    /// An engine over the folders of `settings` with the given providers.
    pub fn new(settings: &Settings, providers: Vec<Arc<dyn Provider>>) -> Self {
        step::clean_stale_tmp(
            &settings.storage.caches.join("tmp"),
            std::time::Duration::from_secs(86_400),
        );
        for p in &providers {
            let id = p.id();
            if matches!(id, "claude" | "codex" | "copilot") {
                p.set_cli_home_mode(settings.cli_home_mode(id));
            }
        }
        Self {
            projects: Projects::new(&settings.storage.projects),
            journal: JournalFiles::new(settings.journal_dir()),
            canvas: CanvasFiles::new(settings.canvas_dir()),
            providers,
            busy: Mutex::new(HashSet::new()),
            simulate_clean_machine: settings.simulate_clean_machine,
            tools: settings.storage.tools.clone(),
            caches: settings.storage.caches.clone(),
            artifacts: settings.storage.artifacts.clone(),
            sandbox_helper: None,
            state_dir: settings.state_dir.clone(),
            sign_in: Default::default(),
            imports: Default::default(),
            installing: Mutex::new(HashSet::new()),
            dialog: None,
        }
    }

    /// Asks the person through `dialog` before installing tools a project needs: what, why, and a
    /// link to check each tool. A tool version allowed once is not asked about again.
    pub fn with_dialog(mut self, dialog: Arc<dyn vf_core::ConfirmDialog>) -> Self {
        self.dialog = Some(dialog);
        self
    }

    /// Hands a provider the CLI the agent installed earlier when it found none of its own.
    fn attach_installed_cli(&self, id: &str) {
        if let Some(p) = self.provider(id)
            && !p.status().installed
            && let Some(bin) = vf_toolchains::cli::installed(&self.tools, id)
        {
            p.use_cli(bin);
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
        let error = |out: &mut dyn FnMut(ServerMessage), e: &str, code: &str| {
            out(ServerMessage::Error {
                error: e.to_owned(),
                code: Some(code.to_owned()),
            })
        };
        let dir = match self.projects.resolve(&task.project) {
            Ok(d) => d,
            Err(_) => return error(out, "unknown project", "not_found"),
        };
        if task.prompt.trim().is_empty() || task.prompt.chars().count() > MAX_PROMPT {
            return error(
                out,
                &format!("prompt required (≤ {MAX_PROMPT} chars)"),
                "bad_prompt",
            );
        }
        let repo = Repo::open(&dir);
        if !repo.is_ready()
            && let Err(e) = Repo::init(&dir)
        {
            return error(out, &format!("cannot prepare git: {e}"), "git_failed");
        }
        let provider = task.provider.clone().unwrap_or_else(|| "auto".into());
        let model = task.model.clone().unwrap_or_default();
        let inventory = self.inventory();
        let lang = task
            .lang
            .as_deref()
            .and_then(vf_core::catalog::lang_code)
            .map(str::to_owned)
            .unwrap_or_else(|| vf_platform::system_language().as_str().to_owned());
        // Where the task started: cancelling it while paused goes back here.
        let start = vf_journal::Vcs::head(&repo).ok();
        let inbox = notes::Inbox::new(&self.caches, &task.project, &lang);
        let t = Task {
            project: &task.project,
            dir: &dir,
            repo: &repo,
            prompt: &task.prompt,
            continue_: task.continue_,
            lang: &lang,
            inbox: &inbox,
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
                    return error(
                        out,
                        &format!("model not available: {provider} {shown}"),
                        "model_unavailable",
                    );
                }
                self.single(&t, &provider, &model, out, cancel);
            }
        }
        if cancel.wants_undo()
            && let Some(start) = start
            && let Err(e) = repo.reset_hard(&start)
        {
            tracing::warn!(project = %task.project, error = %e, "cancelled task not undone");
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
                code: Some("model_unavailable".into()),
            });
        };
        orchestrate::think(out, orchestrate::Thought::Task, t.lang);
        let system_instruction = vf_core::catalog::system_instruction(t.lang);
        let req = StepRequest {
            project: t.project,
            dir: t.dir,
            prompt: t.prompt,
            prefix: "",
            notes: "",
            model,
            continue_: t.continue_,
            step: None,
            system: Some(&system_instruction),
        };
        let r = self.step_noted(t.repo, p.as_ref(), &req, Some(t.inbox), out, cancel);
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

/// A project held by one run (task, build, analysis); released when dropped.
pub(crate) struct Hold<'a> {
    engine: &'a Engine,
    project: String,
}

impl Drop for Hold<'_> {
    fn drop(&mut self) {
        self.engine
            .busy
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.project);
    }
}

impl Engine {
    /// A build; while the person has paused it, waits, then builds again from the start.
    fn pausable_build(&self, project: &str, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel) {
        loop {
            // The interrupted build's "cancelled" is not the end of a paused build.
            self.run_build(
                project,
                &mut |m| {
                    if !(matches!(m, ServerMessage::Cancelled) && cancel.is_paused()) {
                        out(m);
                    }
                },
                cancel,
            );
            if !cancel.is_paused() {
                return;
            }
            out(ServerMessage::Paused);
            if !cancel.wait_resumed() {
                return out(ServerMessage::Cancelled);
            }
            out(ServerMessage::Resumed);
        }
    }

    /// Holds a project for one run; `None` while another run holds it.
    pub(crate) fn hold(&self, project: &str) -> Option<Hold<'_>> {
        let mut busy = self.busy.lock().unwrap_or_else(|p| p.into_inner());
        busy.insert(project.to_owned()).then(|| Hold {
            engine: self,
            project: project.to_owned(),
        })
    }

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
                    code: Some("task_already_running".into()),
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

    fn ai(&self) -> Result<AiResponse, BackendError> {
        let order = [
            "claude",
            "codex",
            "copilot",
            "custom",
            "gemini",
            "openrouter",
        ];
        let mut providers: Vec<AiProvider> = Vec::new();
        for id in order {
            let Some(p) = self.provider(id) else {
                continue;
            };
            self.attach_installed_cli(id);
            let s = p.status();
            let installing = self
                .installing
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .contains(id);
            let host = if id == "custom" {
                vf_platform::settings::read_config(&self.state_dir)
                    .custom_openai
                    .map(|c| vf_core::endpoint_host_label(&c.endpoint))
            } else {
                None
            };
            providers.push(AiProvider {
                installing,
                id: id.into(),
                kind: match id {
                    "openrouter" | "gemini" => AiKind::Free,
                    "custom" => AiKind::Custom,
                    _ => AiKind::Subscription,
                },
                installed: s.installed,
                connected: match (s.installed, s.signed_in) {
                    (false, _) | (_, Some(false)) => AiConnected::No,
                    (true, Some(true)) => AiConnected::Yes,
                    (true, None) => AiConnected::Unknown,
                },
                user_signed_in: if matches!(id, "claude" | "codex" | "copilot") {
                    s.user_signed_in
                } else {
                    None
                },
                mode: if matches!(id, "claude" | "codex" | "copilot") {
                    s.cli_home_mode.or(Some(vf_protocol::CliHomeMode::Agent))
                } else {
                    None
                },
                host,
            });
        }
        let ready = providers.iter().any(|p| p.connected != AiConnected::No);
        Ok(AiResponse { ready, providers })
    }

    fn connect_ai(&self, provider: &str) -> Result<(), BackendError> {
        if !matches!(provider, "claude" | "codex" | "copilot") {
            return Err(BackendError::BadRequest("unknown subscription".into()));
        }
        let p = self
            .provider(provider)
            .ok_or_else(|| BackendError::NotFound("no such provider".into()))?;
        self.attach_installed_cli(provider);
        if !p.status().installed {
            self.installing
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(provider.to_owned());
            let installed = vf_toolchains::cli::install(&self.tools, provider, &Cancel::new());
            self.installing
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(provider);
            p.use_cli(installed.map_err(BackendError::Internal)?);
        }
        p.connect(CONNECT_TIMEOUT).map_err(BackendError::BadRequest)
    }

    fn set_cli_home_mode(
        &self,
        provider: &str,
        mode: vf_protocol::CliHomeMode,
    ) -> Result<(), BackendError> {
        if !matches!(provider, "claude" | "codex" | "copilot") {
            return Err(BackendError::BadRequest(
                "this provider does not support cli home modes".into(),
            ));
        }
        let p = self
            .provider(provider)
            .ok_or_else(|| BackendError::NotFound("no such provider".into()))?;
        p.set_cli_home_mode(mode);
        vf_platform::Settings::save_cli_home_mode_at(&self.state_dir, provider, mode)
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        Ok(())
    }

    fn connect_ai_key(&self, provider: &str, key: &str) -> Result<(), BackendError> {
        if provider != "openrouter" && provider != "gemini" {
            return Err(BackendError::BadRequest("unknown provider for key".into()));
        }
        let p = self
            .provider(provider)
            .ok_or_else(|| BackendError::NotFound("no such provider".into()))?;
        let secret = Secret::new(key.to_owned());
        p.connect_key(&secret).map_err(BackendError::BadRequest)?;
        vf_platform::secrets::set(provider, key)
            .map_err(|e| BackendError::Internal(format!("cannot save secret: {e}")))?;
        p.reload();
        Ok(())
    }

    fn verify_ai_custom(
        &self,
        endpoint: &str,
        key: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<String>, BackendError> {
        let clean_endpoint = vf_core::validate_custom_openai_endpoint(endpoint)
            .map_err(|e| BackendError::BadRequest(e.to_string()))?;

        vf_providers::openai::OpenAiCompatible::verify_custom_endpoint(&clean_endpoint, key, model)
            .map_err(BackendError::BadRequest)
    }

    fn connect_ai_custom(
        &self,
        endpoint: &str,
        key: Option<&str>,
        model: Option<&str>,
    ) -> Result<(), BackendError> {
        let clean_endpoint = vf_core::validate_custom_openai_endpoint(endpoint)
            .map_err(|e| BackendError::BadRequest(e.to_string()))?;

        vf_providers::openai::OpenAiCompatible::verify_custom_endpoint(&clean_endpoint, key, model)
            .map_err(BackendError::BadRequest)?;

        if let Some(k) = key.filter(|k| !k.trim().is_empty()) {
            vf_platform::secrets::set("custom_openai", k)
                .map_err(|e| BackendError::Internal(format!("cannot save secret: {e}")))?;
        } else {
            let _ = vf_platform::secrets::remove("custom_openai");
        }

        let cfg = vf_platform::settings::CustomOpenAiConfig {
            endpoint: clean_endpoint,
            model: model.filter(|m| !m.trim().is_empty()).map(str::to_owned),
        };
        vf_platform::settings::Settings::save_custom_openai_config_at(&self.state_dir, Some(&cfg))
            .map_err(|e| BackendError::Internal(format!("cannot save config: {e}")))?;

        if let Some(p) = self.provider("custom") {
            p.reload();
        }
        Ok(())
    }

    fn disconnect_ai(&self, provider: &str) -> Result<(), BackendError> {
        if provider == "custom" {
            vf_platform::settings::Settings::save_custom_openai_config_at(&self.state_dir, None)
                .map_err(|e| BackendError::Internal(format!("cannot update config: {e}")))?;
            let _ = vf_platform::secrets::remove("custom_openai");
            if let Some(p) = self.provider("custom") {
                p.reload();
            }
            return Ok(());
        }
        if provider == "openrouter" || provider == "gemini" {
            let _ = vf_platform::secrets::remove(provider);
            if let Some(p) = self.provider(provider) {
                p.reload();
            }
            return Ok(());
        }
        if matches!(provider, "claude" | "codex" | "copilot") {
            let p = self
                .provider(provider)
                .ok_or_else(|| BackendError::NotFound("no such provider".into()))?;
            // With the user's own sign-in the agent only stops using it; it never signs the user
            // out of their own CLI. With its own sign-in it signs out.
            if p.cli_home_mode() == Some(vf_protocol::CliHomeMode::User) {
                return self.set_cli_home_mode(provider, vf_protocol::CliHomeMode::Agent);
            }
            return p.logout().map_err(BackendError::BadRequest);
        }
        Err(BackendError::BadRequest(
            "this provider cannot be disconnected".into(),
        ))
    }

    fn models(&self) -> Result<ModelsResponse, BackendError> {
        Ok(ModelsResponse {
            models: self.inventory(),
        })
    }

    fn run_task(&self, task: TaskRequest, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel) {
        let project = task.project.clone();
        self.exclusive(&project, out, |out| {
            if self.projects.resolve(&project).is_err() {
                return self.task(task, out, cancel);
            }
            let timings = self.timings(&project);
            let mut stream = canvas::Stream::new(&self.canvas, &project, out);
            stream.estimate(timings);
            stream.emit(CanvasEvent::Task {
                text: task.prompt.clone(),
                nodes: Vec::new(),
            });
            stream.emit(CanvasEvent::Stage {
                id: Stage::Understand,
            });
            // Steps journaled before this task are not its own.
            let before = self.journal.read(&project).map(|e| e.len()).unwrap_or(0);
            self.task(
                task,
                &mut |m| {
                    let step_ended = matches!(
                        m,
                        ServerMessage::StepDone { .. } | ServerMessage::Done { plan: false, .. }
                    );
                    let task_ended = matches!(m, ServerMessage::Done { .. });
                    if step_ended || task_ended {
                        let entries = self.journal.read(&project).unwrap_or_default();
                        if step_ended
                            && let Some(last) = entries.get(before..).and_then(<[_]>::last)
                        {
                            for ev in changes::change_events(&last.artifacts) {
                                stream.emit(ev);
                            }
                        }
                        if task_ended {
                            let own = entries
                                .into_iter()
                                .skip(before)
                                .flat_map(|e| e.artifacts)
                                .collect();
                            stream.result_artifacts(changes::merge(own));
                        }
                    }
                    stream.forward(m);
                },
                cancel,
            );
        });
    }

    fn offer_prompt(
        &self,
        project: &str,
        index: u32,
        title: Option<&str>,
    ) -> Result<String, vf_protocol::OfferRejected> {
        if self.projects.resolve(project).is_err() {
            return Err(vf_protocol::OfferRejected::NoOffers);
        }
        self.kept_offer_prompt(project, index, title)
    }

    fn canvas(&self, project: &str, from: u64) -> Result<CanvasResponse, BackendError> {
        self.projects
            .resolve(project)
            .map_err(|_| BackendError::NotFound("unknown project".into()))?;
        let from = from.max(1);
        let events = self
            .canvas
            .read_from(project, from, MAX_CANVAS_PAGE)
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        let next = events.last().map_or(from, |r| r.seq.saturating_add(1));
        Ok(CanvasResponse { events, next })
    }

    fn build(&self, project: &str, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel) {
        self.exclusive(project, out, |out| {
            if self.projects.resolve(project).is_err() {
                return self.pausable_build(project, out, cancel);
            }
            let mut stream = canvas::Stream::new(&self.canvas, project, out);
            self.pausable_build(project, &mut |m| stream.forward(m), cancel);
        });
    }

    fn run_program(&self, project: &str, out: &mut dyn FnMut(ServerMessage), cancel: &Cancel) {
        self.exclusive(project, out, |out| {
            self.run_project_program(project, out, cancel);
        });
    }

    fn build_commands(&self, project: &str) -> Vec<String> {
        self.projects
            .resolve(project)
            .ok()
            .and_then(|dir| vf_builds::detect_recipe(&dir))
            .map(|r| r.steps.iter().map(build::command_line).collect())
            .unwrap_or_default()
    }

    fn run_command(&self, project: &str) -> Option<String> {
        let dir = self.projects.resolve(project).ok()?;
        vf_builds::detect_run(&dir).map(|step| build::command_line(&step))
    }

    fn github_connect(&self) -> Result<GithubConnectResponse, BackendError> {
        self.connect()
    }

    fn github_status(&self) -> Result<GithubStatusResponse, BackendError> {
        Ok(self.status())
    }

    fn import(&self, req: &ImportRequest) -> Result<ImportResponse, BackendError> {
        self.import_project(req, "")
    }

    fn import_for(&self, req: &ImportRequest, owner: &str) -> Result<ImportResponse, BackendError> {
        self.import_project(req, owner)
    }

    fn import_progress(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<ImportProgressResponse, BackendError> {
        Ok(self.import_progress_of(owner, repo))
    }

    fn environment(&self, project: Option<&str>) -> Result<EnvironmentResponse, BackendError> {
        Ok(match project {
            Some(project) => {
                let dir = self.projects.resolve(project).map_err(project_error)?;
                vf_toolchains::estimates::estimate_project(&dir, &self.tools)
            }
            None => {
                vf_toolchains::estimates::estimate(&self.tools, vf_toolchains::estimates::catalog())
            }
        })
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

    fn read_file(&self, project: &str, path: &str) -> Result<FileResponse, BackendError> {
        self.read_project_file(project, path)
    }

    fn project_files(&self, project: &str) -> Result<ProjectFilesResponse, BackendError> {
        self.list_project_files(project)
    }

    fn read_diff(&self, project: &str, path: &str) -> Result<FileDiffResponse, BackendError> {
        self.read_project_diff(project, path)
    }

    fn graph(&self, project: &str, folders: bool) -> Result<GraphResponse, BackendError> {
        self.project_graph(project, folders)
    }

    fn describe_graph(&self, req: &DescribeRequest) -> Result<GraphResponse, BackendError> {
        self.describe_project_graph(req)
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

    fn explain(
        &self,
        req: vf_protocol::ExplainRequest,
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) {
        self.explain_question(req, out, cancel);
    }

    fn rollback_part(&self, project: &str, id: &str, out: &mut dyn FnMut(ServerMessage)) {
        self.exclusive(project, out, |out| match self.roll_back_part(project, id) {
            Ok(done) => {
                let mut stream = canvas::Stream::new(&self.canvas, project, out);
                stream.emit(CanvasEvent::Change {
                    id: id.to_owned(),
                    files: done.files,
                });
                stream.emit(CanvasEvent::Result {
                    text: "Part rolled back".into(),
                    detail: done.summary,
                    artifacts: done.artifacts,
                    kind: Some(vf_protocol::canvas::ResultKind::PartRolledBack),
                    exit: None,
                    id: Some(id.to_owned()),
                });
            }
            Err(e) => out(ServerMessage::Error {
                error: e.to_string(),
                code: Some(
                    match e {
                        BackendError::NotFound(_) => "not_found",
                        BackendError::BadRequest(_) => "nothing_to_roll_back",
                        _ => "internal",
                    }
                    .into(),
                ),
            }),
        });
    }
}
