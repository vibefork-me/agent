//! HTTP handlers of `/v1`. Every handler except `hello` gets the origin from the guard and,
//! where needed, a session from [`session_from`].

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use vf_auth::{AuthError, SessionInfo};
use vf_protocol::http::{
    AiConnectRequest, AiConnectResponse, AiDisconnectRequest, AiDisconnectResponse,
    ChallengeRequest, ChallengeResponse, CliHomeMode, DescribeRequest, ErrorBody, Hello,
    ImportRequest, Me, PairStartRequest, PairStartResponse, PairStatusResponse, RevokeRequest,
    RevokeResponse, RollbackRequest, SaveRequest, SessionRequest, SessionResponse,
};
use vf_protocol::{Backend, BackendError};

use crate::AppState;
use crate::guard::EffectiveOrigin;

pub(crate) fn error(status: StatusCode, message: &str) -> Response {
    error_with_code(status, message, None)
}

pub(crate) fn error_with_code(status: StatusCode, message: &str, code: Option<&str>) -> Response {
    (
        status,
        Json(ErrorBody {
            error: message.to_owned(),
            code: code.map(str::to_owned),
            reset_minutes: None,
        }),
    )
        .into_response()
}

fn auth_error(e: &AuthError) -> Response {
    let (status, code) = match e {
        AuthError::BadKey => (StatusCode::BAD_REQUEST, "bad_key"),
        AuthError::Storage => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
        AuthError::TooManyPairings => (StatusCode::TOO_MANY_REQUESTS, "too_many_pairings"),
        AuthError::NotPaired => (StatusCode::UNAUTHORIZED, "not_paired"),
        AuthError::BadChallenge => (StatusCode::UNAUTHORIZED, "bad_challenge"),
        AuthError::BadSignature => (StatusCode::UNAUTHORIZED, "bad_signature"),
    };
    error_with_code(status, &e.to_string(), Some(code))
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
    error_with_code(StatusCode::UNAUTHORIZED, "no session", Some("no_session"))
}

pub(crate) async fn hello(State(state): State<AppState>) -> Json<Hello> {
    Json(Hello {
        agent: "vibefork".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        platform: platform().into(),
        extra_origins_active: !state.config.extra_origins.is_empty(),
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
        BackendError::Forbidden(m) => error(StatusCode::FORBIDDEN, m),
        BackendError::RateLimited { reset_minutes } => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorBody {
                error: format!(
                    "GitHub's limit of requests is reached; connect GitHub or try again in {reset_minutes} min"
                ),
                code: Some("rate_limit".to_owned()),
                reset_minutes: Some(*reset_minutes),
            }),
        )
            .into_response(),
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

/// The session stream of a project from event `from` (query: `project`, `from`, default 1).
pub(crate) async fn canvas(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let project = q.get("project").cloned().unwrap_or_default();
    let from = match q.get("from").map(|f| f.parse::<u64>()) {
        None => 1,
        Some(Ok(from)) => from,
        Some(Err(_)) => return error(StatusCode::BAD_REQUEST, "from must be a number"),
    };
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    if !project_allowed(&state, &headers, &origin.0, &project).await {
        return error(StatusCode::FORBIDDEN, "no access to this project");
    }
    with_backend(state, &headers, &origin.0, move |b| {
        b.canvas(&project, from)
    })
    .await
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

/// Whether AI is connected; the editor is not usable without it.
pub(crate) async fn ai(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
) -> Response {
    with_backend(state, &headers, &origin.0, |b| b.ai()).await
}

/// Starts a provider connection (subscription sign-in or OpenRouter API key) after the user
/// confirms it in the native dialog; `GET /v1/ai` tells when it is done.
pub(crate) async fn ai_connect(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Json(req): Json<AiConnectRequest>,
) -> Response {
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    let is_openrouter = req.provider == "openrouter";
    let is_gemini = req.provider == "gemini";
    let is_custom = req.provider == "custom";
    let is_key_based = is_openrouter || is_gemini;
    let name = match req.provider.as_str() {
        "claude" => "Claude",
        "codex" => "ChatGPT (Codex)",
        "openrouter" => "OpenRouter",
        "gemini" => "Google AI Studio (Gemini)",
        "custom" => "OpenAI-compatible API",
        _ => {
            return error_with_code(
                StatusCode::BAD_REQUEST,
                "unknown provider",
                Some("unknown_provider"),
            );
        }
    };

    if let Some(target_mode) = req.mode {
        if is_key_based || is_custom {
            return error_with_code(
                StatusCode::BAD_REQUEST,
                "this provider does not support cli home modes",
                Some("invalid_provider_mode"),
            );
        }
        let dialog_name = match req.provider.as_str() {
            "claude" => "Claude Code",
            "codex" => "Codex",
            _ => name,
        };
        match target_mode {
            CliHomeMode::User => {
                let auth = state.auth.clone();
                let o = origin.0.clone();
                let lang = auth.dialog().language();
                let prompt = vf_core::catalog::action_connect_ai_user_home(lang, dialog_name);
                let approved =
                    tokio::task::spawn_blocking(move || auth.confirm_action(&o, &prompt, None))
                        .await
                        .unwrap_or(false);
                if !approved {
                    return error_with_code(
                        StatusCode::FORBIDDEN,
                        "not confirmed",
                        Some("action_not_confirmed"),
                    );
                }
                let backend = state.backend.clone();
                let provider = req.provider.clone();
                let res = tokio::task::spawn_blocking(move || {
                    backend.set_cli_home_mode(&provider, CliHomeMode::User)
                })
                .await
                .unwrap_or_else(|e| Err(BackendError::Internal(e.to_string())));

                return match res {
                    Ok(()) => Json(AiConnectResponse { started: true }).into_response(),
                    Err(BackendError::BadRequest(m)) => error(StatusCode::BAD_REQUEST, &m),
                    Err(BackendError::NotFound(m)) => error(StatusCode::NOT_FOUND, &m),
                    Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                };
            }
            CliHomeMode::Agent => {
                let backend = state.backend.clone();
                let provider = req.provider.clone();
                let res = tokio::task::spawn_blocking(move || {
                    backend.set_cli_home_mode(&provider, CliHomeMode::Agent)
                })
                .await
                .unwrap_or_else(|e| Err(BackendError::Internal(e.to_string())));

                return match res {
                    Ok(()) => Json(AiConnectResponse { started: true }).into_response(),
                    Err(BackendError::BadRequest(m)) => error(StatusCode::BAD_REQUEST, &m),
                    Err(BackendError::NotFound(m)) => error(StatusCode::NOT_FOUND, &m),
                    Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
                };
            }
        }
    }
    {
        let mut running = state.connecting.lock().unwrap_or_else(|p| p.into_inner());
        if !running.insert(req.provider.clone()) {
            return Json(AiConnectResponse { started: false }).into_response();
        }
    }
    let auth = state.auth.clone();
    let o = origin.0.clone();
    let lang = auth.dialog().language();

    if is_custom {
        // Step 1: Prompt for endpoint URL
        let auth = state.auth.clone();
        let o = origin.0.clone();
        let endpoint_prompt = vf_core::catalog::custom_openai_endpoint_prompt(lang);
        let endpoint_val = tokio::task::spawn_blocking(move || auth.ask_text(&o, &endpoint_prompt))
            .await
            .unwrap_or(None);

        let Some(raw_endpoint) = endpoint_val.filter(|s| !s.trim().is_empty()) else {
            state
                .connecting
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&req.provider);
            return Json(AiConnectResponse { started: false }).into_response();
        };

        // Validate endpoint (scheme http loopback vs https)
        let clean_endpoint = match vf_core::validate_custom_openai_endpoint(&raw_endpoint) {
            Ok(e) => e,
            Err(reason) => {
                state
                    .connecting
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&req.provider);
                return error_with_code(StatusCode::BAD_REQUEST, reason, Some("invalid_endpoint"));
            }
        };

        // Step 2: Show confirmation dialog with full endpoint and warning
        let auth = state.auth.clone();
        let o = origin.0.clone();
        let confirm_title = vf_core::catalog::confirm_action_title(lang);
        let confirm_text = vf_core::catalog::custom_openai_confirm_text(lang, &o, &clean_endpoint);
        let confirmed = tokio::task::spawn_blocking(move || {
            auth.dialog().confirm(&confirm_title, &confirm_text)
        })
        .await
        .unwrap_or(false);

        if !confirmed {
            state
                .connecting
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&req.provider);
            return error_with_code(
                StatusCode::FORBIDDEN,
                "not confirmed",
                Some("action_not_confirmed"),
            );
        }

        // Step 3: Ask for API key (masked, optional for local server)
        let auth = state.auth.clone();
        let o = origin.0.clone();
        let key_prompt = vf_core::catalog::custom_openai_key_prompt(lang);
        let key_secret =
            tokio::task::spawn_blocking(move || auth.ask_secret_optional(&o, &key_prompt))
                .await
                .unwrap_or(None);

        let Some(key) = key_secret else {
            state
                .connecting
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&req.provider);
            return Json(AiConnectResponse { started: false }).into_response();
        };

        let key_str = key.expose().trim().to_owned();
        let key_opt = if key_str.is_empty() {
            None
        } else {
            Some(key_str)
        };

        // Step 4: Verify endpoint / fetch models
        let ep = clean_endpoint.clone();
        let k = key_opt.clone();
        let b = state.backend.clone();
        let verify_res =
            tokio::task::spawn_blocking(move || b.verify_ai_custom(&ep, k.as_deref(), None))
                .await
                .unwrap_or_else(|e| Err(BackendError::Internal(e.to_string())));

        let mut model_chosen: Option<String> = None;
        if verify_res.is_err() {
            // /models did not provide models, prompt for model identifier
            let auth = state.auth.clone();
            let o = origin.0.clone();
            let model_prompt = vf_core::catalog::custom_openai_model_prompt(lang);
            let model_val = tokio::task::spawn_blocking(move || auth.ask_text(&o, &model_prompt))
                .await
                .unwrap_or(None);

            let Some(m) = model_val.filter(|s| !s.trim().is_empty()) else {
                state
                    .connecting
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&req.provider);
                return Json(AiConnectResponse { started: false }).into_response();
            };
            model_chosen = Some(m.trim().to_owned());
        }

        // Connect & save
        let backend = state.backend.clone();
        let ep = clean_endpoint;
        let k = key_opt;
        let m = model_chosen;
        let res = tokio::task::spawn_blocking(move || {
            backend.connect_ai_custom(&ep, k.as_deref(), m.as_deref())
        })
        .await
        .unwrap_or_else(|e| Err(BackendError::Internal(e.to_string())));

        state
            .connecting
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&req.provider);

        return match res {
            Ok(()) => Json(AiConnectResponse { started: true }).into_response(),
            Err(BackendError::BadRequest(m)) => error(StatusCode::BAD_REQUEST, &m),
            Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    }

    let prompt = if is_key_based {
        vf_core::catalog::action_connect_ai_key(lang, name)
    } else {
        vf_core::catalog::action_connect_ai_subscription(lang, name)
    };
    let approved = tokio::task::spawn_blocking(move || auth.confirm_action(&o, &prompt, None))
        .await
        .unwrap_or(false);
    if !approved {
        state
            .connecting
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&req.provider);
        return error_with_code(
            StatusCode::FORBIDDEN,
            "not confirmed",
            Some("action_not_confirmed"),
        );
    }

    if is_key_based {
        let auth = state.auth.clone();
        let o = origin.0.clone();
        let secret_prompt = if is_gemini {
            vf_core::catalog::gemini_key_prompt(lang)
        } else {
            vf_core::catalog::openrouter_key_prompt(lang)
        };
        let secret = tokio::task::spawn_blocking(move || auth.ask_secret(&o, &secret_prompt))
            .await
            .unwrap_or(None);

        let Some(key) = secret.filter(|s| !s.expose().trim().is_empty()) else {
            state
                .connecting
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&req.provider);
            return Json(AiConnectResponse { started: false }).into_response();
        };

        let backend = state.backend.clone();
        let provider = req.provider.clone();
        let res =
            tokio::task::spawn_blocking(move || backend.connect_ai_key(&provider, key.expose()))
                .await
                .unwrap_or_else(|e| Err(BackendError::Internal(e.to_string())));

        state
            .connecting
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&req.provider);

        return match res {
            Ok(()) => Json(AiConnectResponse { started: true }).into_response(),
            Err(BackendError::BadRequest(m)) => error(StatusCode::BAD_REQUEST, &m),
            Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    }

    let (backend, connecting, provider) = (
        state.backend.clone(),
        state.connecting.clone(),
        req.provider,
    );
    tokio::task::spawn_blocking(move || {
        if let Err(e) = backend.connect_ai(&provider) {
            tracing::info!(%provider, error = %e, "sign-in did not finish");
        }
        connecting
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&provider);
    });
    Json(AiConnectResponse { started: true }).into_response()
}

pub(crate) async fn ai_disconnect(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Json(req): Json<AiDisconnectRequest>,
) -> Response {
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    let backend = state.backend.clone();
    let provider = req.provider.clone();
    let res = tokio::task::spawn_blocking(move || backend.disconnect_ai(&provider))
        .await
        .unwrap_or_else(|e| Err(BackendError::Internal(e.to_string())));

    match res {
        Ok(()) => Json(AiDisconnectResponse { ok: true }).into_response(),
        Err(BackendError::BadRequest(m)) => error(StatusCode::BAD_REQUEST, &m),
        Err(BackendError::NotFound(m)) => error(StatusCode::NOT_FOUND, &m),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

pub(crate) async fn models(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
) -> Response {
    with_backend(state, &headers, &origin.0, |b| b.models()).await
}

/// A project file for the code viewer; the browser needs access to the project (native dialog).
pub(crate) async fn file(
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
    let path = q.get("path").cloned().unwrap_or_default();
    with_backend(state, &headers, &origin.0, move |b| {
        b.read_file(&project, &path)
    })
    .await
}

/// The files of a project for the file tree; the browser needs access to the project.
pub(crate) async fn files(
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
    with_backend(state, &headers, &origin.0, move |b| {
        b.project_files(&project)
    })
    .await
}

/// A project file as taken and as it is now; the browser needs access to the project.
pub(crate) async fn diff(
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
    let path = q.get("path").cloned().unwrap_or_default();
    with_backend(state, &headers, &origin.0, move |b| {
        b.read_diff(&project, &path)
    })
    .await
}

/// The map of a project; the browser needs access to the project.
pub(crate) async fn graph(
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
    let folders = q.get("view").is_some_and(|v| v == "folders");
    with_backend(state, &headers, &origin.0, move |b| {
        b.graph(&project, folders)
    })
    .await
}

/// Has a model draw the project's map; the browser needs access to the project.
pub(crate) async fn describe_graph(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Json(req): Json<DescribeRequest>,
) -> Response {
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    if !project_allowed(&state, &headers, &origin.0, &req.project).await {
        return error(StatusCode::FORBIDDEN, "no access to this project");
    }
    with_backend(state, &headers, &origin.0, move |b| b.describe_graph(&req)).await
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
        return error_with_code(
            StatusCode::FORBIDDEN,
            "no access to this project",
            Some("project_forbidden"),
        );
    }
    let auth = state.auth.clone();
    let lang = auth.dialog().language();
    let prompt = vf_core::catalog::action_rollback(lang);
    let (o, target) = (
        origin.0.clone(),
        format!("{} → {}", req.project, req.commit),
    );
    let approved =
        tokio::task::spawn_blocking(move || auth.confirm_action(&o, &prompt, Some(&target)))
            .await
            .unwrap_or(false);
    if !approved {
        return error_with_code(
            StatusCode::FORBIDDEN,
            "rollback not confirmed",
            Some("rollback_not_confirmed"),
        );
    }
    with_backend(state, &headers, &origin.0, move |b| {
        b.rollback(&req.project, &req.commit)
    })
    .await
}

pub(crate) async fn github_connect(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
) -> Response {
    with_backend(state, &headers, &origin.0, |b| b.github_connect()).await
}

pub(crate) async fn github_status(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
) -> Response {
    with_backend(state, &headers, &origin.0, |b| b.github_status()).await
}

pub(crate) async fn import(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Json(req): Json<ImportRequest>,
) -> Response {
    with_backend(state, &headers, &origin.0, move |b| b.import(&req)).await
}

/// Saves to the user's copy on GitHub. The first save of a project is confirmed in the native
/// dialog: it publishes the work (invariant 3).
pub(crate) async fn save(
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
    headers: HeaderMap,
    Json(req): Json<SaveRequest>,
) -> Response {
    if session_from(&state, &headers, &origin.0).is_none() {
        return no_session();
    }
    if !project_allowed(&state, &headers, &origin.0, &req.project).await {
        return error_with_code(
            StatusCode::FORBIDDEN,
            "no access to this project",
            Some("project_forbidden"),
        );
    }
    let (auth, backend, o, p) = (
        state.auth.clone(),
        state.backend.clone(),
        origin.0.clone(),
        req.project.clone(),
    );
    let lang = auth.dialog().language();
    let prompt = vf_core::catalog::action_save(lang);
    let approved = tokio::task::spawn_blocking(move || {
        !backend.save_needs_confirm(&p) || auth.confirm_action(&o, &prompt, Some(&p))
    })
    .await
    .unwrap_or(false);
    if !approved {
        return error_with_code(
            StatusCode::FORBIDDEN,
            "save not confirmed",
            Some("save_not_confirmed"),
        );
    }
    with_backend(state, &headers, &origin.0, move |b| b.save(&req)).await
}
