//! 🔒 **Security boundary.** HTTP + WebSocket API for the web UI.
//!
//! Enforced here (see ARCHITECTURE.md):
//! - invariant 1: listens on loopback only — [`bind_address`] is the only way to get a listen address;
//! - invariant 2: every call except `GET /v1/hello` needs a session (guard + handlers);
//! - `Host` must be loopback with our port (DNS-rebinding guard);
//! - `Origin` must be allowed; same-origin GETs without `Origin` are accepted only with `x-vibefork: 1`
//!   (a cross-site page cannot send that header without a CORS preflight, which carries `Origin`);
//! - `POST` requires JSON + `x-vibefork: 1` (forces a preflight; no "simple" CSRF requests);
//! - request bodies are limited to [`BODY_LIMIT`].

mod api;
mod guard;
mod routes;
mod ui;
mod ws;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, RwLock};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use tokio::net::TcpListener;
use vf_auth::Auth;
use vf_protocol::Backend;
use vf_updater::UiBundle;

pub use api::{ROUTES, Route};
pub use guard::EffectiveOrigin;

/// How many ports after the preferred one are tried if it is busy.
pub const FALLBACK_PORTS: u16 = 4;
/// Maximum request body, bytes.
pub const BODY_LIMIT: usize = 20 * 1024;

/// The current verified UI bundle, replaced when a newer one is downloaded.
pub type UiSlot = Arc<RwLock<Option<UiBundle>>>;

/// The listen address for a port: **always loopback** (invariant 1). No other IP can be passed.
pub fn bind_address(port: u16) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
}

/// Whether a `Host` header value is one of ours (DNS-rebinding guard).
pub fn is_allowed_host(host: &str, port: u16) -> bool {
    host == format!("127.0.0.1:{port}")
        || host == format!("localhost:{port}")
        || host == format!("[::1]:{port}")
}

/// Server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// The port the server is bound to.
    pub port: u16,
    /// Allowed web origins; the agent's own loopback origins are always allowed.
    pub allowed_origins: Vec<String>,
    /// Extra origins configured by the user (if any).
    pub extra_origins: Vec<String>,
}

impl Config {
    fn origin_allowed(&self, origin: &str) -> bool {
        origin == format!("http://127.0.0.1:{}", self.port)
            || origin == format!("http://localhost:{}", self.port)
            || self.allowed_origins.iter().any(|o| o == origin)
    }

    /// Whether `origin` was allowed because of `extra_origins`.
    pub fn is_extra_origin(&self, origin: &str) -> bool {
        self.extra_origins.iter().any(|o| o == origin)
    }
}

/// Shared state of the handlers.
#[derive(Clone)]
pub struct AppState {
    pub(crate) config: Arc<Config>,
    pub(crate) auth: Auth,
    pub(crate) ui: UiSlot,
    pub(crate) backend: Arc<dyn Backend>,
    /// Subscriptions whose sign-in is running.
    pub(crate) connecting: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    /// Projects where a paired browser started work (`ws`).
    pub(crate) started: Arc<ws::StartedWork>,
}

/// The router (used by [`serve`] and by tests).
pub fn router(config: Config, auth: Auth, ui: UiSlot, backend: Arc<dyn Backend>) -> Router {
    let state = AppState {
        config: Arc::new(config),
        auth,
        ui,
        backend,
        connecting: Arc::default(),
        started: Arc::default(),
    };
    Router::new()
        .route("/", get(ui::root))
        .route("/ui", get(ui::root))
        .route("/ui/", get(ui::index))
        .route("/ui/{*path}", get(ui::file))
        .route("/v1/hello", get(routes::hello))
        .route("/v1/pair/start", post(routes::pair_start))
        .route("/v1/pair/status", get(routes::pair_status))
        .route("/v1/auth/challenge", post(routes::challenge))
        .route("/v1/auth/session", post(routes::session))
        .route("/v1/me", get(routes::me))
        .route("/v1/pairs", get(routes::pairs))
        .route("/v1/pairs/revoke", post(routes::revoke))
        .route("/v1/projects", get(routes::projects))
        .route("/v1/journal", get(routes::journal))
        .route("/v1/canvas", get(routes::canvas))
        .route("/v1/models", get(routes::models))
        .route("/v1/file", get(routes::file))
        .route("/v1/files", get(routes::files))
        .route("/v1/diff", get(routes::diff))
        .route("/v1/ai", get(routes::ai))
        .route("/v1/ai/connect", post(routes::ai_connect))
        .route("/v1/ai/disconnect", post(routes::ai_disconnect))
        .route("/v1/graph", get(routes::graph))
        .route("/v1/graph/describe", post(routes::describe_graph))
        .route("/v1/rollback", post(routes::rollback))
        .route("/v1/github/connect", post(routes::github_connect))
        .route("/v1/github/status", get(routes::github_status))
        .route("/v1/projects/import", post(routes::import))
        .route("/v1/projects/import/progress", get(routes::import_progress))
        .route("/v1/environment", get(routes::environment))
        .route("/v1/save", post(routes::save))
        .route("/v1/ws", get(ws::upgrade))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            guard::guard,
        ))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .with_state(state)
}

/// Binds the preferred port or one of the next [`FALLBACK_PORTS`] on loopback.
/// Returns the listener and the port actually bound.
pub async fn bind(preferred_port: u16) -> std::io::Result<(TcpListener, u16)> {
    let mut last_error = None;
    for port in preferred_port..=preferred_port.saturating_add(FALLBACK_PORTS) {
        match TcpListener::bind(bind_address(port)).await {
            Ok(listener) => return Ok((listener, port)),
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.unwrap_or_else(|| std::io::Error::other("no port")))
}

/// Serves on a bound listener until the process ends.
pub async fn serve(
    listener: TcpListener,
    config: Config,
    auth: Auth,
    ui: UiSlot,
    backend: Arc<dyn Backend>,
) -> std::io::Result<()> {
    tracing::info!(address = %bind_address(config.port), "agent listening");
    axum::serve(listener, router(config, auth, ui, backend)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_to_loopback_only() {
        for port in 47801..=47801 + FALLBACK_PORTS {
            assert!(bind_address(port).ip().is_loopback());
        }
    }

    #[test]
    fn host_check_rejects_foreign_hosts() {
        assert!(is_allowed_host("127.0.0.1:47801", 47801));
        assert!(is_allowed_host("localhost:47801", 47801));
        assert!(!is_allowed_host("evil.com", 47801));
        assert!(!is_allowed_host("127.0.0.1:47802", 47801));
        assert!(!is_allowed_host("127.0.0.1.evil.com:47801", 47801));
    }
}
