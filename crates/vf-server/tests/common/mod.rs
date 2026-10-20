//! A backend double for router tests.

#![allow(dead_code)]

use std::sync::Arc;

use vf_protocol::http::{
    GithubConnectResponse, GithubStatusResponse, ImportRequest, ImportResponse, JournalResponse,
    ModelsResponse, ProjectsResponse, RollbackResponse, SaveResponse, SaveStep,
};
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, BackendError, Cancel, TaskRequest};

/// One project `demo` with an empty journal; tasks end at once with `Done`.
pub(crate) struct FakeBackend;

impl Backend for FakeBackend {
    fn projects(&self) -> Result<ProjectsResponse, BackendError> {
        Ok(ProjectsResponse {
            root: "/projects".into(),
            projects: vec!["demo".into()],
        })
    }
    fn journal(&self, project: &str) -> Result<JournalResponse, BackendError> {
        if project == "demo" {
            Ok(JournalResponse { entries: vec![] })
        } else {
            Err(BackendError::NotFound("project not found".into()))
        }
    }
    fn models(&self) -> Result<ModelsResponse, BackendError> {
        Ok(ModelsResponse { models: vec![] })
    }
    fn build(&self, _: &str, out: &mut dyn FnMut(ServerMessage), _: &Cancel) {
        out(ServerMessage::BuildDone {
            code: 0,
            artifacts: None,
            files: vec![],
            seconds: 0.0,
        });
    }
    fn rollback(&self, project: &str, commit: &str) -> Result<RollbackResponse, BackendError> {
        Ok(RollbackResponse {
            project: project.into(),
            commit: commit.into(),
        })
    }
    fn github_connect(&self) -> Result<GithubConnectResponse, BackendError> {
        Err(BackendError::Internal("offline".into()))
    }
    fn github_status(&self) -> Result<GithubStatusResponse, BackendError> {
        Ok(GithubStatusResponse {
            connected: false,
            pending: false,
            login: None,
            error: None,
        })
    }
    fn import(&self, req: &ImportRequest) -> Result<ImportResponse, BackendError> {
        Err(BackendError::BadRequest(req.repo.clone()))
    }
    fn save_needs_confirm(&self, _: &str) -> bool {
        true
    }
    fn save(&self, _: &str) -> Result<SaveResponse, BackendError> {
        Ok(SaveResponse {
            step: SaveStep::Saved,
            url: None,
            copy: None,
            commit: None,
        })
    }
    fn run_task(&self, _: TaskRequest, out: &mut dyn FnMut(ServerMessage), _: &Cancel) {
        out(ServerMessage::Done {
            code: 0,
            provider: None,
            model: None,
            session_id: None,
            commit: None,
            diff: String::new(),
            status: String::new(),
            plan: false,
        });
    }
}

pub(crate) fn backend() -> Arc<dyn Backend> {
    Arc::new(FakeBackend)
}
