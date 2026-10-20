//! 🔒 **Security boundary.** GitHub for "Save": sign-in of the agent itself (device flow of the
//! Vibefork.me GitHub App — the token comes from GitHub straight to the agent, never through a
//! server or a web page), the REST calls that save work into the user's copy of a project, and the
//! download of a project without git.
//!
//! The app has only `contents: write` and `metadata: read`, on the repositories the user installs it
//! on (one copy at a time). It cannot create, delete or administer repositories: the user creates
//! the copy on github.com ([`fork_url`]) and allows the app on it ([`install_url`]).

mod archive;
mod device;

use std::future::Future;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vf_core::Secret;

pub use archive::extract;
pub use device::{DeviceCode, poll_token, refresh, start_device_flow};

/// Client ID of the Vibefork.me GitHub App (public).
pub const CLIENT_ID: &str = "Iv23ligLOsa7qIfUzXP5";
/// URL name of the app.
pub const APP_SLUG: &str = "vibefork-me";
/// Branch of the user's copy that receives saves.
pub const SAVE_BRANCH: &str = "vibefork";
const API: &str = "https://api.github.com";

/// Errors of GitHub calls.
#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    /// Network or GitHub failure.
    #[error("GitHub: {0}")]
    Http(String),
    /// No such repository (or no access to it).
    #[error("not found")]
    NotFound,
    /// The user has not approved the sign-in (yet).
    #[error("{0}")]
    Auth(String),
    /// Bad data from GitHub or in an archive.
    #[error("{0}")]
    Data(String),
}

/// Tokens of the signed-in user, stored in the OS secret store as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    /// Access token.
    pub access: String,
    /// Refresh token (device-flow tokens refresh without the app's secret).
    pub refresh: Option<String>,
    /// Expiry of the access token, Unix seconds.
    pub expires_at: Option<u64>,
}

impl Tokens {
    /// Whether the access token ends within `margin`.
    pub fn expiring(&self, margin: Duration) -> bool {
        self.expires_at
            .is_some_and(|at| at <= now() + margin.as_secs())
    }
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `owner/name` from `owner/name`, `https://github.com/owner/name` or `….git`.
pub fn parse_repo(input: &str) -> Option<String> {
    let s = input.trim().trim_end_matches('/').trim_end_matches(".git");
    let s = s
        .strip_prefix("https://github.com/")
        .or_else(|| s.strip_prefix("github.com/"))
        .unwrap_or(s);
    let mut parts = s.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    let ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 100
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && p != "."
            && p != ".."
    };
    (parts.next().is_none() && ok(owner) && ok(name)).then(|| format!("{owner}/{name}"))
}

/// The page where the user creates their copy (one button on github.com).
pub fn fork_url(upstream: &str) -> String {
    format!("https://github.com/{upstream}/fork")
}

/// The page that installs the app on exactly one repository (preselected).
pub fn install_url(user_id: u64, repo_id: u64) -> String {
    format!(
        "https://github.com/apps/{APP_SLUG}/installations/new/permissions?suggested_target_id={user_id}&repository_ids[]={repo_id}"
    )
}

/// A repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    /// Id.
    pub id: u64,
    /// `owner/name`.
    pub full_name: String,
    /// Default branch.
    pub default_branch: String,
    /// The repository it was forked from, if it is a fork.
    pub parent: Option<String>,
    /// Private.
    pub private: bool,
}

/// The signed-in user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// Login.
    pub login: String,
    /// Id.
    pub id: u64,
}

/// One change of a save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// New or changed file: path, git mode (`100644` / `100755`), content.
    Put(String, String, Vec<u8>),
    /// Deleted file: path.
    Delete(String),
}

/// Runs a future from a blocking thread (inside the agent's runtime, or a private one).
fn block_on<F: Future>(f: F) -> Result<F::Output, GithubError> {
    match tokio::runtime::Handle::try_current() {
        Ok(h) => Ok(h.block_on(f)),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map(|rt| rt.block_on(f))
            .map_err(|e| GithubError::Http(e.to_string())),
    }
}

fn http() -> Result<reqwest::Client, GithubError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("vibefork-agent")
        .build()
        .map_err(|e| GithubError::Http(e.to_string()))
}

/// REST client; anonymous for public data or with the user's token.
pub struct Client {
    token: Option<Secret<String>>,
}

impl Client {
    /// Anonymous (public repositories only).
    pub fn anonymous() -> Self {
        Self { token: None }
    }

    /// With the user's access token.
    pub fn with_token(token: Secret<String>) -> Self {
        Self { token: Some(token) }
    }

    fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, GithubError> {
        let mut req = http()?
            .request(method, format!("{API}{path}"))
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28");
        if let Some(t) = &self.token {
            req = req.bearer_auth(t.expose());
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        block_on(async {
            let res = req
                .send()
                .await
                .map_err(|e| GithubError::Http(e.to_string()))?;
            let status = res.status();
            let v: Value = res.json().await.unwrap_or(Value::Null);
            match status.as_u16() {
                200..=299 => Ok(v),
                404 => Err(GithubError::NotFound),
                401 => Err(GithubError::Auth("GitHub sign-in expired".into())),
                _ => Err(GithubError::Http(format!(
                    "HTTP {status}: {}",
                    v["message"].as_str().unwrap_or_default()
                ))),
            }
        })?
    }

    fn get(&self, path: &str) -> Result<Value, GithubError> {
        self.call(reqwest::Method::GET, path, None)
    }

    fn post(&self, path: &str, body: Value) -> Result<Value, GithubError> {
        self.call(reqwest::Method::POST, path, Some(body))
    }

    /// The signed-in user.
    pub fn user(&self) -> Result<User, GithubError> {
        let v = self.get("/user")?;
        Ok(User {
            login: v["login"]
                .as_str()
                .ok_or_else(|| GithubError::Data("no login".into()))?
                .to_owned(),
            id: v["id"].as_u64().unwrap_or(0),
        })
    }

    /// A repository; `None` when it does not exist (or is not visible).
    pub fn repo(&self, full_name: &str) -> Result<Option<RepoInfo>, GithubError> {
        match self.get(&format!("/repos/{full_name}")) {
            Ok(v) => Ok(Some(RepoInfo {
                id: v["id"].as_u64().unwrap_or(0),
                full_name: v["full_name"].as_str().unwrap_or(full_name).to_owned(),
                default_branch: v["default_branch"].as_str().unwrap_or("main").to_owned(),
                parent: v["parent"]["full_name"].as_str().map(str::to_owned),
                private: v["private"].as_bool().unwrap_or(false),
            })),
            Err(GithubError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The commit a branch points to.
    pub fn branch_head(&self, full_name: &str, branch: &str) -> Result<String, GithubError> {
        let v = self.get(&format!("/repos/{full_name}/commits/{branch}"))?;
        v["sha"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| GithubError::Data("no commit".into()))
    }

    /// The `.tar.gz` of a repository at a commit.
    pub fn tarball(&self, full_name: &str, sha: &str) -> Result<Vec<u8>, GithubError> {
        let mut req = http()?.get(format!("{API}/repos/{full_name}/tarball/{sha}"));
        if let Some(t) = &self.token {
            req = req.bearer_auth(t.expose());
        }
        block_on(async {
            let res = req
                .send()
                .await
                .map_err(|e| GithubError::Http(e.to_string()))?;
            if !res.status().is_success() {
                return Err(if res.status().as_u16() == 404 {
                    GithubError::NotFound
                } else {
                    GithubError::Http(format!("HTTP {}", res.status()))
                });
            }
            Ok(res
                .bytes()
                .await
                .map_err(|e| GithubError::Http(e.to_string()))?
                .to_vec())
        })?
    }

    /// Whether the app is installed on `full_name` for this user.
    pub fn app_installed_on(&self, full_name: &str) -> Result<bool, GithubError> {
        let installations = self.get("/user/installations")?;
        for inst in installations["installations"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if inst["app_slug"] != APP_SLUG {
                continue;
            }
            let id = inst["id"].as_u64().unwrap_or(0);
            let repos = self.get(&format!(
                "/user/installations/{id}/repositories?per_page=100"
            ))?;
            if repos["repositories"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|r| r["full_name"] == full_name)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether a commit exists in a repository.
    pub fn has_commit(&self, full_name: &str, sha: &str) -> Result<bool, GithubError> {
        match self.get(&format!("/repos/{full_name}/git/commits/{sha}")) {
            Ok(_) => Ok(true),
            Err(GithubError::NotFound) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Writes `changes` on top of commit `parent` as one commit and points [`SAVE_BRANCH`] at it.
    /// Returns the new commit.
    pub fn save(
        &self,
        full_name: &str,
        parent: &str,
        message: &str,
        changes: &[Change],
    ) -> Result<String, GithubError> {
        let base = self.get(&format!("/repos/{full_name}/git/commits/{parent}"))?;
        let base_tree = base["tree"]["sha"]
            .as_str()
            .ok_or_else(|| GithubError::Data("no tree".into()))?
            .to_owned();
        let mut entries = Vec::new();
        for c in changes {
            match c {
                Change::Put(path, mode, content) => {
                    use base64::Engine as _;
                    let blob = self.post(
                        &format!("/repos/{full_name}/git/blobs"),
                        json!({ "content": base64::engine::general_purpose::STANDARD.encode(content), "encoding": "base64" }),
                    )?;
                    entries.push(
                        json!({ "path": path, "mode": mode, "type": "blob", "sha": blob["sha"] }),
                    );
                }
                Change::Delete(path) => entries.push(
                    json!({ "path": path, "mode": "100644", "type": "blob", "sha": Value::Null }),
                ),
            }
        }
        let tree = self.post(
            &format!("/repos/{full_name}/git/trees"),
            json!({ "base_tree": base_tree, "tree": entries }),
        )?;
        let commit = self.post(
            &format!("/repos/{full_name}/git/commits"),
            json!({ "message": message, "tree": tree["sha"], "parents": [parent] }),
        )?;
        let sha = commit["sha"]
            .as_str()
            .ok_or_else(|| GithubError::Data("no commit".into()))?
            .to_owned();
        let path = format!("/repos/{full_name}/git/refs/heads/{SAVE_BRANCH}");
        match self.call(
            reqwest::Method::PATCH,
            &path,
            Some(json!({ "sha": sha, "force": false })),
        ) {
            Ok(_) => {}
            // The branch does not exist yet (GitHub answers 422 for a missing ref).
            Err(GithubError::NotFound | GithubError::Http(_)) => {
                self.post(
                    &format!("/repos/{full_name}/git/refs"),
                    json!({ "ref": format!("refs/heads/{SAVE_BRANCH}"), "sha": sha }),
                )?;
            }
            Err(e) => return Err(e),
        }
        Ok(sha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names() {
        assert_eq!(
            parse_repo("octocat/Spoon-Knife").as_deref(),
            Some("octocat/Spoon-Knife")
        );
        assert_eq!(
            parse_repo("https://github.com/octocat/Spoon-Knife.git").as_deref(),
            Some("octocat/Spoon-Knife")
        );
        assert_eq!(parse_repo("github.com/a/b/").as_deref(), Some("a/b"));
        for bad in [
            "a",
            "a/b/c",
            "../x",
            "a/..",
            "https://gitlab.com/a/b",
            "a b/c",
        ] {
            assert_eq!(parse_repo(bad), None, "{bad}");
        }
    }

    #[test]
    fn links() {
        assert_eq!(fork_url("o/r"), "https://github.com/o/r/fork");
        assert_eq!(
            install_url(1, 2),
            "https://github.com/apps/vibefork-me/installations/new/permissions?suggested_target_id=1&repository_ids[]=2"
        );
    }
}
