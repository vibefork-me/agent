//! The request guard: Host, Origin, preflight, JSON + custom header, CORS headers.

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};

use crate::AppState;
use crate::routes::error;

/// The origin a request is attributed to after the guard (an extension on the request).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveOrigin(pub String);

/// Custom header our pages send on every call.
pub(crate) const MARKER: &str = "x-vibefork";

pub(crate) async fn guard(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let headers = req.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    if !crate::is_allowed_host(&host, state.config.port) {
        tracing::warn!(%host, path = %req.uri().path(), "rejected: bad host");
        return error(StatusCode::FORBIDDEN, "bad host");
    }

    // Fallback UI: top-level navigation carries no Origin. One canonical address (127.0.0.1) keeps
    // one browser origin, so pairing done on the UI stays valid.
    let path = req.uri().path();
    let is_ui = path == "/" || path == "/ui" || path.starts_with("/ui/");
    if is_ui && req.method() == Method::GET && headers.get(header::ORIGIN).is_none() {
        if host.starts_with("localhost:") || host.starts_with("[::1]:") {
            let target = format!(
                "http://127.0.0.1:{}{}",
                state.config.port,
                req.uri()
                    .path_and_query()
                    .map(|p| p.as_str())
                    .unwrap_or("/")
            );
            return Redirect::temporary(&target).into_response();
        }
        return next.run(req).await;
    }

    let marked = headers.get(MARKER).is_some_and(|v| v == "1");
    let origin = match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(o) => o.to_owned(),
        // Same-origin GETs from our own pages carry no Origin; they are marked with x-vibefork.
        None if marked => format!("http://{host}"),
        // Direct navigation: only the liveness check is answered.
        None if req.uri().path() == "/v1/hello" && req.method() == Method::GET => {
            return next.run(req).await;
        }
        None => return error(StatusCode::FORBIDDEN, "origin required"),
    };
    if !state.config.origin_allowed(&origin) {
        tracing::warn!(%origin, path = %req.uri().path(), "rejected: origin not allowed");
        return error(StatusCode::FORBIDDEN, "origin not allowed");
    }

    if req.method() == Method::OPTIONS {
        let mut res = StatusCode::NO_CONTENT.into_response();
        add_cors(res.headers_mut(), &origin);
        return res;
    }

    if req.method() == Method::POST {
        let json = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/json"));
        if !marked || !json {
            let mut res = error(
                StatusCode::BAD_REQUEST,
                "x-vibefork header and JSON required",
            );
            add_cors(res.headers_mut(), &origin);
            return res;
        }
    }

    req.extensions_mut().insert(EffectiveOrigin(origin.clone()));
    let mut res = next.run(req).await;
    add_cors(res.headers_mut(), &origin);
    res
}

fn add_cors(headers: &mut HeaderMap, origin: &str) {
    if let Ok(value) = HeaderValue::from_str(origin) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, x-vibefork, authorization"),
    );
    headers.insert(
        "access-control-allow-private-network",
        HeaderValue::from_static("true"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("60"),
    );
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
}
