//! GitHub: the agent's own sign-in, projects taken from GitHub without git, and "Save" into the
//! user's copy.
//!
//! A project taken from GitHub remembers the exact commit of the original it came from. Saves are
//! built as commits on top of that commit in the user's fork (it has the original's history), on the
//! branch [`vf_github::SAVE_BRANCH`] (`vibefork-<project>` for a second project of the same
//! original) — so a quick start without git keeps "Save to GitHub" with the full fork structure
//! possible. When the fork does not have that commit (the original rewrote its history), the
//! changes go on top of the fork's current version unless a changed file was changed there too.
//!
//! A project without an original on GitHub is saved into an empty private repository of the user,
//! on its default branch; its first save holds every file.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use vf_core::Secret;
use vf_git::Repo;
use vf_github::{
    Change, Client, GitRef, GithubError, RepoInfo, Tokens, User, fork_url, install_url,
    install_url_any, installation_url, new_repo_url, parse_repo,
};
use vf_journal::{JournalStore as _, Vcs as _};
use vf_protocol::canvas::{CanvasEvent, Stage};
use vf_protocol::http::{
    ForeignChanges, GithubConnectResponse, GithubStatusResponse, ImportRequest, ImportResponse,
    SaveRequest, SaveResponse, SaveStep,
};
use vf_protocol::ws::ServerMessage;
use vf_protocol::{BackendError, Cancel};

use crate::Engine;
use crate::step::now_rfc3339;

/// Name of the GitHub sign-in in the OS secret store.
const SECRET: &str = "github";

/// What the agent remembers about a project taken from GitHub (`<state>/projects/<name>.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectMeta {
    /// The original: `owner/name`; `None` — the project has no original on GitHub.
    #[serde(default)]
    pub(crate) upstream: Option<String>,
    /// The original is private.
    #[serde(default)]
    pub(crate) private: bool,
    /// Commit of the original the project was taken from.
    #[serde(default)]
    pub(crate) base_sha: String,
    /// Local commit with the original's files.
    #[serde(default)]
    pub(crate) local_base: String,
    /// The user's copy: `owner/name`.
    #[serde(default)]
    pub(crate) copy: Option<String>,
    /// Branch of the copy that receives saves.
    #[serde(default)]
    pub(crate) branch: Option<String>,
    /// Local commit of the last save.
    #[serde(default)]
    pub(crate) saved_local: Option<String>,
    /// Commit of the last save in the copy.
    #[serde(default)]
    pub(crate) saved_remote: Option<String>,
    /// Time of the last save.
    #[serde(default)]
    pub(crate) saved_at: Option<String>,
    /// The original when the code was taken from a fork or a pull request (`upstream` is then where
    /// the code came from).
    #[serde(default)]
    pub(crate) original: Option<String>,
}

/// What code taken from `source` at `git_ref` changes compared with `original`. GitHub failing to
/// list the files is not an error: the summary says it is incomplete, and the UI shows that.
fn foreign_changes(
    client: &Client,
    original: &str,
    source: &RepoInfo,
    git_ref: Option<&GitRef>,
) -> ForeignChanges {
    let ref_name = git_ref.map_or_else(|| source.default_branch.clone(), GitRef::as_str);
    let label = format!("{}@{ref_name}", source.full_name);
    let (listed, compare_url) = match git_ref {
        Some(GitRef::Pull(n)) => (
            client.changed_files(&source.full_name, "", Ok(*n)),
            format!("https://github.com/{}/pull/{n}/files", source.full_name),
        ),
        _ => {
            let base = client
                .repo(original)
                .ok()
                .flatten()
                .map_or_else(|| "HEAD".to_owned(), |r| r.default_branch);
            let owner = source.full_name.split('/').next().unwrap_or_default();
            let head = format!("{owner}:{ref_name}");
            (
                client.changed_files(original, &base, Err(&head)),
                format!("https://github.com/{original}/compare/{base}...{head}"),
            )
        }
    };
    match listed {
        Ok(c) => ForeignChanges {
            source: label,
            files: u32::try_from(c.all.len()).unwrap_or(u32::MAX),
            dependencies: c
                .all
                .iter()
                .filter(|p| vf_github::is_dependency_file(p))
                .cloned()
                .collect(),
            build_scripts: c
                .all
                .iter()
                .filter(|p| vf_github::is_build_script(p))
                .cloned()
                .collect(),
            binaries: c
                .all
                .iter()
                .filter(|p| vf_github::is_binary(p))
                .cloned()
                .collect(),
            complete: c.complete,
            compare_url,
        },
        Err(e) => {
            tracing::warn!(source = %label, error = %e, "changed files of foreign code not listed");
            ForeignChanges {
                source: label,
                files: 0,
                dependencies: Vec::new(),
                build_scripts: Vec::new(),
                binaries: Vec::new(),
                complete: false,
                compare_url,
            }
        }
    }
}

/// The sign-in in progress.
#[derive(Default)]
pub(crate) struct SignIn {
    pending: Option<(GithubConnectResponse, Cancel)>,
    error: Option<String>,
    login: Option<String>,
}

fn gh(e: GithubError) -> BackendError {
    match e {
        GithubError::NotFound => BackendError::NotFound("not found on GitHub".into()),
        GithubError::Data(m) => BackendError::BadRequest(m),
        GithubError::RateLimit { reset_minutes } => BackendError::RateLimited { reset_minutes },
        e => BackendError::Internal(e.to_string()),
    }
}

impl Engine {
    fn meta_path(&self, project: &str) -> PathBuf {
        self.state_dir
            .join("projects")
            .join(format!("{project}.json"))
    }

    pub(crate) fn load_meta(&self, project: &str) -> Option<ProjectMeta> {
        serde_json::from_slice(&std::fs::read(self.meta_path(project)).ok()?).ok()
    }

    fn store_meta(&self, project: &str, meta: &ProjectMeta) -> Result<(), BackendError> {
        let path = self.meta_path(project);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| BackendError::Internal(e.to_string()))?;
        }
        let bytes =
            serde_json::to_vec_pretty(meta).map_err(|e| BackendError::Internal(e.to_string()))?;
        std::fs::write(path, bytes).map_err(|e| BackendError::Internal(e.to_string()))
    }

    fn tokens(&self) -> Option<Tokens> {
        vf_platform::secrets::get(SECRET).and_then(|s| serde_json::from_str(s.expose()).ok())
    }

    fn store_tokens(t: &Tokens) {
        if let Ok(json) = serde_json::to_string(t)
            && let Err(e) = vf_platform::secrets::set(SECRET, &json)
        {
            tracing::warn!(error = %e, "GitHub sign-in not stored");
        }
    }

    /// A client with a fresh token; `None` when GitHub is not connected (or the sign-in ended).
    fn client(&self) -> Option<Client> {
        let mut t = self.tokens()?;
        if t.expiring(Duration::from_secs(300)) {
            match vf_github::refresh(&t) {
                Ok(new) => {
                    Self::store_tokens(&new);
                    t = new;
                }
                Err(e) => {
                    tracing::info!(error = %e, "GitHub sign-in ended");
                    let _ = vf_platform::secrets::remove(SECRET);
                    return None;
                }
            }
        }
        Some(Client::with_token(Secret::new(t.access)))
    }

    pub(crate) fn connect(&self) -> Result<GithubConnectResponse, BackendError> {
        let mut s = self.sign_in.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((code, _)) = &s.pending {
            return Ok(code.clone());
        }
        let device = vf_github::start_device_flow().map_err(gh)?;
        let shown = GithubConnectResponse {
            user_code: device.user_code.clone(),
            verification_uri: device.verification_uri.clone(),
            expires_in: device.expires_in,
        };
        let cancel = Cancel::new();
        s.pending = Some((shown.clone(), cancel.clone()));
        s.error = None;
        let state = Arc::clone(&self.sign_in);
        std::thread::spawn(move || {
            let result = vf_github::poll_token(&device, &cancel);
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.pending = None;
            match result {
                Ok(t) => {
                    Self::store_tokens(&t);
                    s.login = Client::with_token(Secret::new(t.access))
                        .user()
                        .ok()
                        .map(|u| u.login);
                }
                Err(e) => s.error = Some(e.to_string()),
            }
        });
        Ok(shown)
    }

    pub(crate) fn status(&self) -> GithubStatusResponse {
        let connected = self.tokens().is_some();
        let mut s = self.sign_in.lock().unwrap_or_else(|p| p.into_inner());
        if connected && s.login.is_none() {
            s.login = self.client().and_then(|c| c.user().ok()).map(|u| u.login);
        }
        GithubStatusResponse {
            connected,
            pending: s.pending.is_some(),
            login: if connected { s.login.clone() } else { None },
            error: s.error.clone(),
        }
    }

    pub(crate) fn import_project(
        &self,
        req: &ImportRequest,
    ) -> Result<ImportResponse, BackendError> {
        let upstream = parse_repo(&req.repo)
            .ok_or_else(|| BackendError::BadRequest("not a GitHub project (owner/name)".into()))?;
        let git_ref = match req
            .git_ref
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
        {
            Some(r) => Some(GitRef::parse(r).ok_or_else(|| {
                BackendError::BadRequest("not a branch, tag, commit or pull/<N>/head".into())
            })?),
            None => None,
        };
        let original = match req
            .upstream
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
        {
            Some(u) => Some(parse_repo(u).ok_or_else(|| {
                BackendError::BadRequest("the original is not a GitHub project (owner/name)".into())
            })?),
            None => None,
        };
        let signed_in = self.client();
        let connected = signed_in.is_some();
        let client = signed_in.unwrap_or_else(Client::anonymous);
        // A private project is visible once the app is allowed on it.
        let info = client.repo(&upstream).map_err(gh)?.ok_or_else(|| {
            BackendError::NotFound(if connected {
                format!("{upstream} is not on GitHub, or it is private and Vibefork is not allowed on it")
            } else {
                format!("{upstream} is not a public project on GitHub (connect GitHub for a private one)")
            })
        })?;
        let sha = match &git_ref {
            Some(r) => client.resolve_ref(&info.full_name, r).map_err(gh)?,
            None => client
                .branch_head(&info.full_name, &info.default_branch)
                .map_err(gh)?,
        };
        let original = original.unwrap_or_else(|| info.full_name.clone());
        let foreign = (matches!(git_ref, Some(GitRef::Pull(_)))
            || !original.eq_ignore_ascii_case(&info.full_name))
        .then(|| foreign_changes(&client, &original, &info, git_ref.as_ref()));
        let archive = client.tarball(&info.full_name, &sha).map_err(gh)?;
        let base = info
            .full_name
            .split('/')
            .nth(1)
            .unwrap_or("project")
            .to_owned();
        let name = (1..100)
            .map(|i| {
                if i == 1 {
                    base.clone()
                } else {
                    format!("{base}-{i}")
                }
            })
            .find(|n| {
                vf_platform::projects::valid_name(n) && !self.projects.root().join(n).exists()
            })
            .ok_or_else(|| BackendError::BadRequest("no free project name".into()))?;
        // Unpacked next to the projects first, moved into place when complete.
        let tmp = self
            .projects
            .root()
            .join(format!(".import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(self.projects.root())
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        vf_github::extract(&archive, &tmp).map_err(gh)?;
        let dest = self.projects.root().join(&name);
        std::fs::rename(&tmp, &dest).map_err(|e| BackendError::Internal(e.to_string()))?;
        let repo = Repo::init_with(
            &dest,
            &format!(
                "vibefork: start from {}@{}",
                info.full_name,
                sha.get(..7).unwrap_or(&sha)
            ),
        )
        .map_err(|e| BackendError::Internal(e.0))?;
        let local_base = repo.head().map_err(|e| BackendError::Internal(e.0))?;
        self.store_meta(
            &name,
            &ProjectMeta {
                upstream: Some(info.full_name.clone()),
                private: info.private,
                base_sha: sha.clone(),
                local_base,
                original: foreign.is_some().then(|| original.clone()),
                ..ProjectMeta::default()
            },
        )?;
        // The project's session stream starts with its download.
        if let Some(_hold) = self.hold(&name) {
            let size = u64::try_from(archive.len()).unwrap_or(u64::MAX);
            let mut sink = |_: ServerMessage| {};
            let mut stream = crate::canvas::Stream::new(&self.canvas, &name, &mut sink);
            stream.emit(CanvasEvent::Stage {
                id: Stage::Download,
            });
            stream.emit(CanvasEvent::Progress {
                done: size,
                total: size,
                unit: "bytes".into(),
                eta: 0,
                text: None,
                id: None,
            });
        }
        Ok(ImportResponse {
            project: name,
            upstream: original,
            commit: sha,
            foreign,
        })
    }

    pub(crate) fn needs_confirm(&self, project: &str) -> bool {
        self.load_meta(project)
            .is_none_or(|m| m.saved_remote.is_none())
    }

    pub(crate) fn save_project(&self, req: &SaveRequest) -> Result<SaveResponse, BackendError> {
        let project = req.project.as_str();
        let dir = self
            .projects
            .resolve(project)
            .map_err(|e| BackendError::NotFound(e.to_string()))?;
        // A project without an original on GitHub has no metadata yet.
        let mut meta = self.load_meta(project).unwrap_or_default();
        if let Some(c) = &req.copy {
            let c = parse_repo(c)
                .ok_or_else(|| BackendError::BadRequest("not a repository (owner/name)".into()))?;
            if meta.copy.as_deref() != Some(c.as_str()) {
                meta.copy = Some(c);
                meta.branch = None;
                meta.saved_local = None;
                meta.saved_remote = None;
            }
        }
        let Some(client) = self.client() else {
            return Ok(next(SaveStep::ConnectGithub, None));
        };
        let user = match client.user() {
            Ok(u) => u,
            Err(GithubError::Auth(_)) => return Ok(next(SaveStep::ConnectGithub, None)),
            Err(e) => return Err(gh(e)),
        };
        let info = match self.find_copy(&client, &user, &meta, project)? {
            Found::Repo(info) => info,
            Found::Step(r) => return Ok(r),
        };
        let copy = info.full_name.clone();
        let with_copy = |r: SaveResponse| SaveResponse {
            copy: Some(copy.clone()),
            ..r
        };
        match client.installation_on(&copy).map_err(gh)? {
            None => {
                return Ok(with_copy(next(
                    SaveStep::AllowApp,
                    Some(install_url(user.id, info.id)),
                )));
            }
            Some(i) if i.all => {
                return Ok(with_copy(next(
                    SaveStep::NarrowAccess,
                    Some(installation_url(i.id)),
                )));
            }
            Some(_) => {}
        }
        let branch = meta.branch.clone().unwrap_or_else(|| match &meta.upstream {
            Some(up) => save_branch(project, up),
            None => info.default_branch.clone(),
        });

        let repo = Repo::open(&dir);
        if !repo.is_ready() {
            Repo::init(&dir).map_err(|e| BackendError::Internal(e.0))?;
        }
        repo.commit_all("vibefork: save", &[])
            .map_err(|e| BackendError::Internal(e.0))?;
        let head = repo.head().map_err(|e| BackendError::Internal(e.0))?;
        // A save the copy no longer has (the copy was created anew, the branch rewritten): the
        // next save starts over from the project's start.
        if let Some(p) = &meta.saved_remote
            && !client.has_commit(&copy, p).map_err(gh)?
        {
            meta.saved_remote = None;
            meta.saved_local = None;
        }
        let from = meta
            .saved_local
            .clone()
            .unwrap_or_else(|| meta.local_base.clone());
        if head == from {
            return Ok(SaveResponse {
                url: Some(saved_url(&copy, &branch)),
                commit: meta.saved_remote.clone(),
                ..with_copy(next(SaveStep::Unchanged, None))
            });
        }
        let local = |base: &str| -> Result<Vec<Change>, BackendError> {
            Ok(repo
                .changes(base, &head)
                .map_err(|e| BackendError::Internal(e.0))?
                .into_iter()
                .map(|c| match c.content {
                    Some(content) => Change::Put(c.path, c.mode, content),
                    None => Change::Delete(c.path),
                })
                .collect())
        };
        let (parent, changes) = if let Some(p) = meta.saved_remote.clone() {
            (Some(p), local(&from)?)
        } else if client.is_empty(&copy).map_err(gh)? {
            // First commit of an empty repository: every file.
            (None, local("")?)
        } else if !meta.base_sha.is_empty()
            && client.has_commit(&copy, &meta.base_sha).map_err(gh)?
        {
            (Some(meta.base_sha.clone()), local(&from)?)
        } else {
            // The copy does not have the version the project started from (the original rewrote
            // its history, or the repository holds other work): the changes go on top of its
            // current version when no changed file was changed there too.
            let current = match client.ref_head(&copy, &branch).map_err(gh)? {
                Some(h) => h,
                None => client
                    .branch_head(&copy, &info.default_branch)
                    .map_err(gh)?,
            };
            let changes = local(&from)?;
            let paths: Vec<String> = changes
                .iter()
                .map(|c| match c {
                    Change::Put(p, ..) | Change::Delete(p) => p.clone(),
                })
                .collect();
            let theirs = client.blob_ids(&copy, &current, &paths).map_err(gh)?;
            let start = repo
                .blob_ids(&from, &paths)
                .map_err(|e| BackendError::Internal(e.0))?;
            let ours = repo
                .blob_ids(&head, &paths)
                .map_err(|e| BackendError::Internal(e.0))?;
            let files: Vec<String> = paths
                .into_iter()
                .filter(|p| theirs.get(p) != start.get(p) && theirs.get(p) != ours.get(p))
                .collect();
            if !files.is_empty() {
                return Ok(SaveResponse {
                    files,
                    ..with_copy(next(SaveStep::Conflict, Some(saved_url(&copy, &branch))))
                });
            }
            (Some(current), changes)
        };
        let message = self.save_message(project, meta.saved_at.as_deref());
        let sha = client
            .save(&copy, parent.as_deref(), &branch, &message, &changes)
            .map_err(gh)?;
        meta.copy = Some(copy.clone());
        meta.branch = Some(branch.clone());
        meta.saved_local = Some(head);
        meta.saved_remote = Some(sha.clone());
        meta.saved_at = Some(now_rfc3339());
        self.store_meta(project, &meta)?;
        Ok(SaveResponse {
            url: Some(saved_url(&copy, &branch)),
            commit: Some(sha),
            ..with_copy(next(SaveStep::Saved, None))
        })
    }

    /// The repository saves go to, or what the user does on github.com to have it.
    fn find_copy(
        &self,
        client: &Client,
        user: &User,
        meta: &ProjectMeta,
        project: &str,
    ) -> Result<Found, BackendError> {
        if let Some(c) = &meta.copy
            && let Some(info) = client.repo(c).map_err(gh)?
        {
            if let Some(up) = &meta.upstream
                && info.parent.as_deref() != Some(up.as_str())
                && !info.full_name.eq_ignore_ascii_case(up)
            {
                return Err(BackendError::BadRequest(format!(
                    "{c} is not a copy of {up}"
                )));
            }
            return Ok(Found::Repo(info));
        }
        match &meta.upstream {
            Some(up) => self.find_fork_copy(client, user, meta, up),
            None => self.find_own_repo(client, user, meta, project),
        }
    }

    /// A project taken from GitHub: the user's fork of the original (any name), or the original
    /// itself when it is the user's.
    fn find_fork_copy(
        &self,
        client: &Client,
        user: &User,
        meta: &ProjectMeta,
        up: &str,
    ) -> Result<Found, BackendError> {
        let (owner, name) = up.split_once('/').unwrap_or((up, up));
        if owner.eq_ignore_ascii_case(&user.login) {
            return Ok(match client.repo(up).map_err(gh)? {
                Some(info) => Found::Repo(info),
                None => Found::Step(next(SaveStep::AllowApp, Some(install_url_any(user.id)))),
            });
        }
        let usual = client.repo(&format!("{}/{name}", user.login)).map_err(gh)?;
        if let Some(info) = &usual
            && info.parent.as_deref() == Some(up)
        {
            return Ok(Found::Repo(info.clone()));
        }
        if let Some(info) = client.find_fork(up, &user.login).map_err(gh)? {
            return Ok(Found::Repo(info));
        }
        Ok(Found::Step(SaveResponse {
            // A private original has private forks: the app sees the fork once allowed on it.
            next_url: meta.private.then(|| install_url_any(user.id)),
            name: usual.is_some().then(|| format!("{name}-vibefork")),
            ..next(SaveStep::CreateCopy, Some(fork_url(up)))
        }))
    }

    /// A project without an original on GitHub: an empty repository of the user (the project's
    /// name, or the first free `name-N`).
    fn find_own_repo(
        &self,
        client: &Client,
        user: &User,
        meta: &ProjectMeta,
        project: &str,
    ) -> Result<Found, BackendError> {
        let base = meta
            .copy
            .as_deref()
            .and_then(|c| c.split('/').nth(1))
            .unwrap_or(project);
        for i in 1..50 {
            let name = if i == 1 {
                base.to_owned()
            } else {
                format!("{base}-{i}")
            };
            let full = format!("{}/{name}", user.login);
            match client.repo(&full).map_err(gh)? {
                Some(info) if client.is_empty(&full).map_err(gh)? => return Ok(Found::Repo(info)),
                Some(_) => continue,
                None => {
                    // Created under another name and allowed: the empty repository the app reaches.
                    for inst in client.installations().map_err(gh)? {
                        if inst.all {
                            continue;
                        }
                        for r in &inst.repos {
                            if client.is_empty(r).map_err(gh)?
                                && let Some(info) = client.repo(r).map_err(gh)?
                            {
                                return Ok(Found::Repo(info));
                            }
                        }
                    }
                    return Ok(Found::Step(SaveResponse {
                        copy: Some(full),
                        next_url: Some(install_url_any(user.id)),
                        name: Some(name.clone()),
                        ..next(SaveStep::CreateRepo, Some(new_repo_url(&name)))
                    }));
                }
            }
        }
        Err(BackendError::BadRequest("no free repository name".into()))
    }

    /// A Conventional Commits message from the tasks since the last save.
    fn save_message(&self, project: &str, since: Option<&str>) -> String {
        let tasks: Vec<String> = self
            .journal
            .read(project)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| {
                since.is_none_or(|s| e.at.as_str() > s)
                    && !matches!(e.provider.as_str(), "planner" | "rollback")
            })
            .filter(|e| !e.commits.is_empty())
            .map(|e| e.prompt.lines().next().unwrap_or_default().to_owned())
            .collect();
        let subject: String = tasks
            .last()
            .map(String::as_str)
            .unwrap_or("customization")
            .chars()
            .take(60)
            .collect();
        let mut msg = format!(
            "feat: {}",
            subject.trim_end_matches('.').to_lowercase_first()
        );
        if tasks.len() > 1 {
            msg.push_str("\n\n");
            for t in &tasks {
                msg.push_str(&format!("- {t}\n"));
            }
        }
        msg.push_str("\n\nSaved with Vibefork");
        msg
    }
}

/// Result of looking for the copy.
enum Found {
    Repo(RepoInfo),
    Step(SaveResponse),
}

/// A response with only a step and its page.
fn next(step: SaveStep, url: Option<String>) -> SaveResponse {
    SaveResponse {
        step,
        url,
        copy: None,
        commit: None,
        next_url: None,
        name: None,
        files: Vec::new(),
    }
}

/// Branch of the fork for a project: [`vf_github::SAVE_BRANCH`], or `vibefork-<project>` when the
/// project's name differs from the original's (a second project of the same original).
fn save_branch(project: &str, upstream: &str) -> String {
    if upstream
        .split('/')
        .nth(1)
        .is_some_and(|n| n.eq_ignore_ascii_case(project))
    {
        vf_github::SAVE_BRANCH.to_owned()
    } else {
        format!("{}-{project}", vf_github::SAVE_BRANCH)
    }
}

fn saved_url(copy: &str, branch: &str) -> String {
    format!("https://github.com/{copy}/tree/{branch}")
}

trait LowercaseFirst {
    fn to_lowercase_first(&self) -> String;
}

impl LowercaseFirst for str {
    fn to_lowercase_first(&self) -> String {
        let mut c = self.chars();
        match c.next() {
            Some(f) => f.to_lowercase().chain(c).collect(),
            None => String::new(),
        }
    }
}

pub(crate) type SignInState = Arc<Mutex<SignIn>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_per_project_of_one_original() {
        assert_eq!(
            save_branch("Spoon-Knife", "octocat/Spoon-Knife"),
            "vibefork"
        );
        assert_eq!(
            save_branch("spoon-knife", "octocat/Spoon-Knife"),
            "vibefork"
        );
        assert_eq!(
            save_branch("Spoon-Knife-2", "octocat/Spoon-Knife"),
            "vibefork-Spoon-Knife-2"
        );
    }

    #[test]
    fn metadata_of_earlier_versions_reads() {
        let m: ProjectMeta = serde_json::from_str(
            r#"{"upstream":"o/r","baseSha":"abc","localBase":"def","copy":"u/r"}"#,
        )
        .unwrap();
        assert_eq!(m.upstream.as_deref(), Some("o/r"));
        assert_eq!(m.branch, None);
        let local: ProjectMeta = serde_json::from_str("{}").unwrap();
        assert!(local.upstream.is_none() && local.local_base.is_empty());
    }
}
