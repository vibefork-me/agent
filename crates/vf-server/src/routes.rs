//! HTTP handlers of `/v1`. Every handler except `hello` gets the origin from the guard and,
//! where needed, a session from [`session_from`].

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use vf_auth::{AuthError, SessionInfo};
use vf_protocol::http::{
    ActionRequest, ActionResponse, ChallengeRequest, ChallengeResponse, EchoResponse, ErrorBody,
    Hello, Me, PairStartRequest, PairStartResponse, PairStatusResponse, RevokeRequest,
    RevokeResponse, RollbackRequest, SessionRequest, SessionResponse,
};
use vf_protocol::{Backend, BackendError};

use crate::AppState;
use crate::guard::EffectiveOrigin;

pub(crate) fn error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(ErrorBody {
            error: message.to_owned(),
        }),
    )
        .into_response()
}

fn auth_error(e: &AuthError) -> Response {
    let status = match e {
        AuthError::BadKey => StatusCode::BAD_REQUEST,
        AuthError::Storage => StatusCode::INTERNAL_SERVER_ERROR,
        AuthError::NotPaired | AuthError::BadChallenge | AuthError::BadSignature => {
            StatusCode::UNAUTHORIZED
        }
    };
    error(status, &e.to_string())
}

/// The session of a request: `Authorization: Bearer <token>` bound to the request's origin.
fn session_from(state: &AppState, headers: &HeaderMap, origin: &str) -> Option<SessionInfo> {
    let token = headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?;
    state.auth.session(token, origin)
}

/// The `401` response for a request without a valid session.
fn no_session() -> Response {
    error(StatusCode::UNAUTHORIZED, "no session")
}

pub(crate) async fn hello() -> Json<Hello> {
    Json(Hello {
        agent: "vibefork".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        platform: platform().into(),
    })
}

fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

pub(crate) async fn echo(body: String) -> Json<EchoResponse> {
    Json(EchoResponse { echo: body })
}

pub(crate) async fn pair_start(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    Json(req): Json<PairStartRequest>,
) -> Response {
    match state
        .auth
        .start_pairing(&origin.0, &req.public_key_jwk, req.name.as_deref())
    {
        Ok((pair_id, code)) => Json(PairStartResponse { pair_id, code }).into_response(),
        Err(e) => auth_error(&e),
    }
}

pub(crate) async fn pair_status(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let id = q.get("id").map(String::as_str).unwrap_or_default();
    match state.auth.pairing_status(id, &origin.0) {
        Some((status, key_id)) => Json(PairStatusResponse { status, key_id }).into_response(),
        None => error(StatusCode::NOT_FOUND, "unknown pairing"),
    }
}

pub(crate) async fn challenge(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    Json(req): Json<ChallengeRequest>,
) -> Response {
    match state.auth.challenge(&req.key_id, &origin.0) {
        Ok(nonce) => Json(ChallengeResponse { nonce }).into_response(),
        Err(e) => auth_error(&e),
    }
}

pub(crate) async fn session(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    Json(req): Json<SessionRequest>,
) -> Response {
    match state
        .auth
        .open_session(&req.key_id, &req.nonce, &req.signature, &origin.0)
    {
        Ok(token) => Json(SessionResponse {
            token: token.expose().clone(),
            expires_in: vf_auth::SESSION_TTL.as_secs(),
        })
        .into_response(),
        Err(e) => auth_error(&e),
    }
}

pub(crate) async fn me(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
) -> Response {
    let Some(s) = session_from(&state, &headers, &origin.0) else {
        return no_session();
    };
    Json(Me {
        key_id: s.key_id,
        origin: s.origin,
        name: s.name,
    })
    .into_response()
}

pub(crate) async fn action(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Json(req): Json<ActionRequest>,
) -> Response {
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    let auth = state.auth.clone();
    let (o, a, t) = (origin.0.clone(), req.action.clone(), req.target.clone());
    let approved = tokio::task::spawn_blocking(move || auth.confirm_action(&o, &a, t.as_deref()))
        .await
        .unwrap_or(false);
    let status = if approved {
        StatusCode::OK
    } else {
        StatusCode::FORBIDDEN
    };
    (
        status,
        Json(ActionResponse {
            action: req.action,
            approved,
        }),
    )
        .into_response()
}

pub(crate) async fn pairs(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
) -> Response {
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    Json(state.auth.pairs()).into_response()
}

pub(crate) async fn revoke(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Json(req): Json<RevokeRequest>,
) -> Response {
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    match state.auth.revoke(&req.key_id) {
        Ok(()) => Json(RevokeResponse {
            revoked: req.key_id,
        })
        .into_response(),
        Err(e) => auth_error(&e),
    }
}

/// Whether the session's browser may work in `project` (native dialog the first time).
async fn project_allowed(
    state: &AppState,
    headers: &HeaderMap,
    origin: &str,
    project: &str,
) -> bool {
    let Some(s) = session_from(state, headers, origin) else {
        return false;
    };
    let (auth, o, p) = (state.auth.clone(), origin.to_owned(), project.to_owned());
    tokio::task::spawn_blocking(move || auth.project_allowed(&s.key_id, &o, &p))
        .await
        .unwrap_or(false)
}

fn backend_error(e: &BackendError) -> Response {
    match e {
        BackendError::BadRequest(m) => error(StatusCode::BAD_REQUEST, m),
        BackendError::NotFound(m) => error(StatusCode::NOT_FOUND, m),
        BackendError::Internal(m) => {
            tracing::warn!(error = %m, "backend call failed");
            error(StatusCode::INTERNAL_SERVER_ERROR, m)
        }
    }
}

/// Runs a blocking backend call for a request with a session.
async fn with_backend<T, F>(state: AppState, headers: &HeaderMap, origin: &str, call: F) -> Response
where
    T: serde::Serialize + Send + 'static,
    F: FnOnce(&dyn Backend) -> Result<T, BackendError> + Send + 'static,
{
    if session_from(&state, headers, origin).is_none() {
        return no_session();
    }
    let backend = state.backend.clone();
    match tokio::task::spawn_blocking(move || call(backend.as_ref())).await {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(e)) => backend_error(&e),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
    }
}

pub(crate) async fn projects(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
) -> Response {
    with_backend(state, &headers, &origin.0, |b| b.projects()).await
}

pub(crate) async fn journal(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let project = q.get("project").cloned().unwrap_or_default();
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    if !project_allowed(&state, &headers, &origin.0, &project).await {
        return error(StatusCode::FORBIDDEN, "no access to this project");
    }
    with_backend(state, &headers, &origin.0, move |b| b.journal(&project)).await
}

pub(crate) async fn models(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
) -> Response {
    with_backend(state, &headers, &origin.0, |b| b.models()).await
}

/// Resets a project to a journal checkpoint after the native dialog confirms it (invariant 3).
pub(crate) async fn rollback(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Json(req): Json<RollbackRequest>,
) -> Response {
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    if !project_allowed(&state, &headers, &origin.0, &req.project).await {
        return error(StatusCode::FORBIDDEN, "no access to this project");
    }
    let auth = state.auth.clone();
    let (o, target) = (
        origin.0.clone(),
        format!("{} → {}", req.project, req.commit),
    );
    let approved = tokio::task::spawn_blocking(move || {
        auth.confirm_action(
            &o,
            "roll the project back (later changes are dropped)",
            Some(&target),
        )
    })
    .await
    .unwrap_or(false);
    if !approved {
        return error(StatusCode::FORBIDDEN, "rollback not confirmed");
    }
    with_backend(state, &headers, &origin.0, move |b| {
        b.rollback(&req.project, &req.commit)
    })
    .await
}
