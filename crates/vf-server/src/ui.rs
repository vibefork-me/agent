//! The fallback UI at `/ui/`: files of the signed bundle only (verified by `vf-updater`), with
//! security headers. Same origin as the API, so the page needs no browser permission.

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use vf_updater::bundle::content_type;

use crate::AppState;
use crate::routes::error;

pub(crate) async fn root() -> Redirect {
    Redirect::to("/ui/")
}

pub(crate) async fn index(State(state): State<AppState>) -> Response {
    serve(&state, "")
}

pub(crate) async fn file(State(state): State<AppState>, Path(path): Path<String>) -> Response {
    serve(&state, &path)
}

fn serve(state: &AppState, path: &str) -> Response {
    let guard = state
        .ui
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(bundle) = guard.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "interface bundle not available yet",
        );
    };
    let Some(bytes) = bundle.file(path) else {
        return error(StatusCode::NOT_FOUND, "not in the bundle");
    };
    let name = if path.is_empty() || path.ends_with('/') {
        "index.html"
    } else {
        path
    };
    let mut res = (StatusCode::OK, bytes.to_vec()).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(name)),
    );
    let port = state.config.port;
    let csp = format!(
        "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
         connect-src 'self' http://127.0.0.1:{port} http://localhost:{port} ws://127.0.0.1:{port} ws://localhost:{port}; \
         frame-ancestors 'none'; base-uri 'none'; form-action 'none'"
    );
    if let Ok(v) = HeaderValue::from_str(&csp) {
        h.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}
