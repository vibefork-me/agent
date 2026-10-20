//! GitHub: the agent's own sign-in, projects taken from GitHub without git, and "Save" into the
//! user's copy.
//!
//! A project taken from GitHub remembers the exact commit of the original it came from. Saves are
//! built as commits on top of that commit in the user's copy (it has the original's history), on the
//! branch [`vf_github::SAVE_BRANCH`] — so a quick start without git keeps "Save to GitHub" with the
//! full fork structure possible.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use vf_core::Secret;
use vf_git::Repo;
use vf_github::{Change, Client, GithubError, Tokens, fork_url, install_url, parse_repo};
use vf_journal::{JournalStore as _, Vcs as _};
use vf_protocol::http::{
    GithubConnectResponse, GithubStatusResponse, ImportRequest, ImportResponse, SaveResponse,
    SaveStep,
};
use vf_protocol::{BackendError, Cancel};

use crate::Engine;
use crate::step::now_rfc3339;

/// Name of the GitHub sign-in in the OS secret store.
const SECRET: &str = "github";

/// What the agent remembers about a project taken from GitHub (`<state>/projects/<name>.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectMeta {
    /// The original: `owner/name`.
    pub(crate) upstream: String,
    /// Commit of the original the project was taken from.
    pub(crate) base_sha: String,
    /// Local commit with the original's files.
    pub(crate) local_base: String,
    /// The user's copy: `owner/name`.
    #[serde(default)]
    pub(crate) copy: Option<String>,
    /// Local commit of the last save.
    #[serde(default)]
    pub(crate) saved_local: Option<String>,
    /// Commit of the last save in the copy.
    #[serde(default)]
    pub(crate) saved_remote: Option<String>,
    /// Time of the last save.
    #[serde(default)]
    pub(crate) saved_at: Option<String>,
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
        let client = self.client().unwrap_or_else(Client::anonymous);
        let info = client.repo(&upstream).map_err(gh)?.ok_or_else(|| {
            BackendError::NotFound(format!("{upstream} is not a public project on GitHub"))
        })?;
        if info.private {
            return Err(BackendError::BadRequest(
                "private projects are not supported yet".into(),
            ));
        }
        let sha = client
            .branch_head(&info.full_name, &info.default_branch)
            .map_err(gh)?;
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
                &sha[..7.min(sha.len())]
            ),
        )
        .map_err(|e| BackendError::Internal(e.0))?;
        let local_base = repo.head().map_err(|e| BackendError::Internal(e.0))?;
        self.store_meta(
            &name,
            &ProjectMeta {
                upstream: info.full_name.clone(),
                base_sha: sha.clone(),
                local_base,
                ..ProjectMeta::default()
            },
        )?;
        Ok(ImportResponse {
            project: name,
            upstream: info.full_name,
            commit: sha,
        })
    }

    pub(crate) fn needs_confirm(&self, project: &str) -> bool {
        self.load_meta(project)
            .is_none_or(|m| m.saved_remote.is_none())
    }

    pub(crate) fn save_project(&self, project: &str) -> Result<SaveResponse, BackendError> {
        let step = |step, url: Option<String>, copy: Option<String>| {
            Ok(SaveResponse {
                step,
                url,
                copy,
                commit: None,
            })
        };
        let dir = self
            .projects
            .resolve(project)
            .map_err(|e| BackendError::NotFound(e.to_string()))?;
        let mut meta = self.load_meta(project).ok_or_else(|| {
            BackendError::BadRequest("this project was not taken from GitHub: saving it to a new repository is not supported yet".into())
        })?;
        let Some(client) = self.client() else {
            return step(SaveStep::ConnectGithub, None, None);
        };
        let user = match client.user() {
            Ok(u) => u,
            Err(GithubError::Auth(_)) => return step(SaveStep::ConnectGithub, None, None),
            Err(e) => return Err(gh(e)),
        };
        let name = meta
            .upstream
            .split('/')
            .nth(1)
            .unwrap_or_default()
            .to_owned();
        let copy = meta
            .copy
            .clone()
            .unwrap_or_else(|| format!("{}/{name}", user.login));
        let Some(info) = client.repo(&copy).map_err(gh)? else {
            return step(SaveStep::CreateCopy, Some(fork_url(&meta.upstream)), None);
        };
        if info.parent.as_deref() != Some(meta.upstream.as_str()) {
            return Err(BackendError::BadRequest(format!(
                "{copy} exists but is not a copy of {}",
                meta.upstream
            )));
        }
        if !client.app_installed_on(&copy).map_err(gh)? {
            return step(
                SaveStep::AllowApp,
                Some(install_url(user.id, info.id)),
                Some(copy),
            );
        }

        let repo = Repo::open(&dir);
        repo.commit_all("vibefork: save", &[])
            .map_err(|e| BackendError::Internal(e.0))?;
        let head = repo.head().map_err(|e| BackendError::Internal(e.0))?;
        let from = meta
            .saved_local
            .clone()
            .unwrap_or_else(|| meta.local_base.clone());
        if head == from {
            return Ok(SaveResponse {
                step: SaveStep::Unchanged,
                url: Some(saved_url(&copy)),
                copy: Some(copy),
                commit: meta.saved_remote.clone(),
            });
        }
        let parent = meta
            .saved_remote
            .clone()
            .unwrap_or_else(|| meta.base_sha.clone());
        if meta.saved_remote.is_none() && !client.has_commit(&copy, &parent).map_err(gh)? {
            return Err(BackendError::BadRequest(format!(
                "your copy does not have the version the project was taken from ({}); update the copy on GitHub",
                &parent[..7.min(parent.len())]
            )));
        }
        let changes: Vec<Change> = repo
            .changes(&from, &head)
            .map_err(|e| BackendError::Internal(e.0))?
            .into_iter()
            .map(|c| match c.content {
                Some(content) => Change::Put(c.path, c.mode, content),
                None => Change::Delete(c.path),
            })
            .collect();
        let message = self.save_message(project, meta.saved_at.as_deref());
        let sha = client
            .save(&copy, &parent, &message, &changes)
            .map_err(gh)?;
        meta.copy = Some(copy.clone());
        meta.saved_local = Some(head);
        meta.saved_remote = Some(sha.clone());
        meta.saved_at = Some(now_rfc3339());
        self.store_meta(project, &meta)?;
        Ok(SaveResponse {
            step: SaveStep::Saved,
            url: Some(saved_url(&copy)),
            copy: Some(copy),
            commit: Some(sha),
        })
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

fn saved_url(copy: &str) -> String {
    format!("https://github.com/{copy}/tree/{}", vf_github::SAVE_BRANCH)
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
