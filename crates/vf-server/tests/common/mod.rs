//! A backend double for router tests.

#![allow(dead_code)]

use std::sync::Arc;

use vf_protocol::http::{
    AiResponse, DescribeRequest, FileDiffResponse, FileResponse, GithubConnectResponse,
    GithubStatusResponse, GraphResponse, GraphSource, ImportRequest, ImportResponse,
    JournalResponse, ModelsResponse, ProjectsResponse, RollbackResponse, SaveRequest, SaveResponse,
    SaveStep,
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
            details: vec![],
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
        if req.repo == "rate/limit" {
            return Err(BackendError::RateLimited { reset_minutes: 42 });
        }
        Err(BackendError::BadRequest(req.repo.clone()))
    }
    fn save_needs_confirm(&self, _: &str) -> bool {
        true
    }
    fn read_file(&self, _: &str, path: &str) -> Result<FileResponse, BackendError> {
        if path == ".env" {
            return Err(BackendError::Forbidden("this file is not shown".into()));
        }
        Ok(FileResponse {
            path: path.into(),
            content: Some("text".into()),
            size: 4,
            too_large: false,
            binary: false,
        })
    }
    fn read_diff(&self, _: &str, path: &str) -> Result<FileDiffResponse, BackendError> {
        Ok(FileDiffResponse {
            path: path.into(),
            base: None,
            original: None,
            modified: Some("text".into()),
            too_large: false,
            binary: false,
        })
    }
    fn graph(&self, _: &str, _: bool) -> Result<GraphResponse, BackendError> {
        Ok(GraphResponse {
            source: GraphSource::Folders,
            revision: None,
            nodes: Vec::new(),
            edges: Vec::new(),
        })
    }
    fn describe_graph(&self, _: &DescribeRequest) -> Result<GraphResponse, BackendError> {
        Err(BackendError::BadRequest("no model available".into()))
    }
    fn ai(&self) -> Result<AiResponse, BackendError> {
        Ok(AiResponse {
            ready: false,
            providers: Vec::new(),
        })
    }
    fn connect_ai(&self, _: &str) -> Result<(), BackendError> {
        Ok(())
    }
    fn connect_ai_key(&self, provider: &str, key: &str) -> Result<(), BackendError> {
        if provider != "openrouter" {
            return Err(BackendError::BadRequest("unknown provider for key".into()));
        }
        if key == "bad-key" || key.contains("invalid") {
            return Err(BackendError::BadRequest(
                "invalid OpenRouter key: OpenRouter returned HTTP 401".into(),
            ));
        }
        Ok(())
    }
    fn save(&self, _: &SaveRequest) -> Result<SaveResponse, BackendError> {
        Ok(SaveResponse {
            step: SaveStep::Saved,
            url: None,
            copy: None,
            commit: None,
            next_url: None,
            name: None,
            files: Vec::new(),
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
