//! Attack tests of the local API: every request a hostile page or process could send must fail.
//! The router is called directly (no network); the native dialog is replaced by test doubles.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::BodyExt as _;
use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{Signature, SigningKey};
use rand::rngs::OsRng;
use serde_json::{Value, json};
use std::sync::RwLock;
use tower::ServiceExt as _;
use vf_auth::Auth;
use vf_core::{ConfirmDialog, Secret};
mod common;

use vf_server::{Config, router};

const PORT: u16 = 47801;
const UI: &str = "https://ui.test";
const OTHER: &str = "https://other.test";
const EVIL: &str = "https://evil.test";

struct Always(bool);
impl ConfirmDialog for Always {
    fn confirm(&self, _title: &str, _text: &str) -> bool {
        self.0
    }
}

struct DialogWithSecret {
    allow: bool,
    secret: Option<String>,
}
impl ConfirmDialog for DialogWithSecret {
    fn confirm(&self, _title: &str, _text: &str) -> bool {
        self.allow
    }
    fn ask_secret(&self, _title: &str, _text: &str) -> Option<Secret<String>> {
        if self.allow {
            self.secret.as_ref().map(|s| Secret::new(s.clone()))
        } else {
            None
        }
    }
}

fn temp_dir() -> PathBuf {
    let mut bytes = [0u8; 8];
    rand::RngCore::fill_bytes(&mut OsRng, &mut bytes);
    std::env::temp_dir().join(format!("vf-attacks-{}", u64::from_le_bytes(bytes)))
}

fn app(dialog_allows: bool) -> Router {
    let auth = Auth::load(temp_dir(), Arc::new(Always(dialog_allows))).unwrap();
    router(
        Config {
            port: PORT,
            allowed_origins: vec![UI.into(), OTHER.into()],
            extra_origins: vec![],
        },
        auth,
        Arc::new(RwLock::new(None)),
        common::backend(),
    )
}

struct Req<'a> {
    method: &'a str,
    path: &'a str,
    host: &'a str,
    origin: Option<&'a str>,
    marker: bool,
    content_type: Option<&'a str>,
    token: Option<&'a str>,
    body: String,
}

impl<'a> Req<'a> {
    fn get(path: &'a str, origin: Option<&'a str>) -> Self {
        Self {
            method: "GET",
            path,
            host: "127.0.0.1:47801",
            origin,
            marker: false,
            content_type: None,
            token: None,
            body: String::new(),
        }
    }
    fn post(path: &'a str, origin: &'a str, body: Value) -> Self {
        Self {
            method: "POST",
            path,
            host: "127.0.0.1:47801",
            origin: Some(origin),
            marker: true,
            content_type: Some("application/json"),
            token: None,
            body: body.to_string(),
        }
    }
}

async fn call(app: &Router, r: Req<'_>) -> (StatusCode, Value, Option<String>) {
    let mut b = Request::builder()
        .method(r.method)
        .uri(r.path)
        .header("host", r.host);
    if let Some(o) = r.origin {
        b = b.header("origin", o);
    }
    if r.marker {
        b = b.header("x-vibefork", "1");
    }
    if let Some(ct) = r.content_type {
        b = b.header("content-type", ct);
    }
    if let Some(t) = r.token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::from(r.body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let acao = res
        .headers()
        .get("access-control-allow-origin")
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        acao,
    )
}

struct Browser {
    key: SigningKey,
}

impl Browser {
    fn new() -> Self {
        let mut b = [0u8; 32];
        rand::RngCore::fill_bytes(&mut OsRng, &mut b);
        b[0] &= 0x7f;
        b[31] |= 0x01;
        Self {
            key: SigningKey::from_slice(&b).unwrap(),
        }
    }
    fn jwk(&self) -> Value {
        let point = self.key.verifying_key().to_sec1_point(false);
        json!({ "kty": "EC", "crv": "P-256", "x": URL_SAFE_NO_PAD.encode(point.x().unwrap()), "y": URL_SAFE_NO_PAD.encode(point.y().unwrap()) })
    }
    fn sign(&self, nonce: &str) -> String {
        let sig: Signature = self.key.sign(nonce.as_bytes());
        URL_SAFE_NO_PAD.encode(sig.to_bytes())
    }
}

async fn pair(app: &Router, browser: &Browser, origin: &str) -> String {
    let (s, v, _) = call(
        app,
        Req::post(
            "/v1/pair/start",
            origin,
            json!({ "publicKeyJwk": browser.jwk(), "name": "test" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let id = v["pairId"].as_str().unwrap().to_owned();
    for _ in 0..200 {
        let path = format!("/v1/pair/status?id={id}");
        let (_, v, _) = call(
            app,
            Req {
                marker: true,
                ..Req::get(&path, Some(origin))
            },
        )
        .await;
        if v["status"] == "approved" {
            return v["keyId"].as_str().unwrap().to_owned();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("pairing was not approved");
}

async fn login(app: &Router, browser: &Browser, key_id: &str, origin: &str) -> String {
    let (_, v, _) = call(
        app,
        Req::post("/v1/auth/challenge", origin, json!({ "keyId": key_id })),
    )
    .await;
    let nonce = v["nonce"].as_str().unwrap();
    let (s, v, _) = call(
        app,
        Req::post(
            "/v1/auth/session",
            origin,
            json!({ "keyId": key_id, "nonce": nonce, "signature": browser.sign(nonce) }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn hello_answers_direct_navigation_only() {
    let app = app(true);
    assert_eq!(
        call(&app, Req::get("/v1/hello", None)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, Req::get("/v1/me", None)).await.0,
        StatusCode::FORBIDDEN,
        "no Origin, no marker"
    );
}

#[tokio::test]
async fn foreign_host_is_rejected() {
    let app = app(true);
    for host in ["evil.test", "127.0.0.1:47802", "127.0.0.1.evil.test:47801"] {
        assert_eq!(
            call(
                &app,
                Req {
                    host,
                    ..Req::get("/v1/hello", Some(UI))
                }
            )
            .await
            .0,
            StatusCode::FORBIDDEN,
            "{host}"
        );
    }
}

#[tokio::test]
async fn foreign_origin_is_rejected_everywhere() {
    let app = app(true);
    assert_eq!(
        call(&app, Req::get("/v1/hello", Some(EVIL))).await.0,
        StatusCode::FORBIDDEN
    );
    let browser = Browser::new();
    let (s, _, _) = call(
        &app,
        Req::post(
            "/v1/pair/start",
            EVIL,
            json!({ "publicKeyJwk": browser.jwk() }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "pairing from a foreign origin");
}

#[tokio::test]
async fn preflight_and_cors_echo_the_allowed_origin_only() {
    let app = app(true);
    let (s, _, acao) = call(
        &app,
        Req {
            method: "OPTIONS",
            ..Req::get("/v1/pair/start", Some(UI))
        },
    )
    .await;
    assert_eq!((s, acao.as_deref()), (StatusCode::NO_CONTENT, Some(UI)));
}

#[tokio::test]
async fn simple_csrf_post_is_rejected() {
    let app = app(true);
    let no_marker = Req {
        marker: false,
        ..Req::post("/v1/pair/start", UI, json!({}))
    };
    assert_eq!(call(&app, no_marker).await.0, StatusCode::BAD_REQUEST);
    let text_plain = Req {
        content_type: Some("text/plain"),
        ..Req::post("/v1/pair/start", UI, json!({}))
    };
    assert_eq!(call(&app, text_plain).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn private_key_or_wrong_curve_is_rejected_at_pairing() {
    let app = app(true);
    let mut jwk = Browser::new().jwk();
    jwk["d"] = json!("c2VjcmV0");
    assert_eq!(
        call(
            &app,
            Req::post("/v1/pair/start", UI, json!({ "publicKeyJwk": jwk }))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let mut jwk = Browser::new().jwk();
    jwk["crv"] = json!("P-384");
    assert_eq!(
        call(
            &app,
            Req::post("/v1/pair/start", UI, json!({ "publicKeyJwk": jwk }))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn denied_dialog_means_no_pairing() {
    let app = app(false);
    let browser = Browser::new();
    let (_, v, _) = call(
        &app,
        Req::post(
            "/v1/pair/start",
            UI,
            json!({ "publicKeyJwk": browser.jwk() }),
        ),
    )
    .await;
    let path = format!("/v1/pair/status?id={}", v["pairId"].as_str().unwrap());
    let mut status = Value::Null;
    for _ in 0..200 {
        status = call(
            &app,
            Req {
                marker: true,
                ..Req::get(&path, Some(UI))
            },
        )
        .await
        .1["status"]
            .clone();
        if status != "pending" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(status, "denied");
}

#[tokio::test]
async fn happy_path_pair_login_me() {
    let app = app(true);
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;
    let (s, v, _) = call(
        &app,
        Req {
            token: Some(&token),
            ..Req::get("/v1/me", Some(UI))
        },
    )
    .await;
    assert_eq!(
        (s, v["keyId"].as_str()),
        (StatusCode::OK, Some(key_id.as_str()))
    );
}

#[tokio::test]
async fn no_or_unknown_token_is_rejected() {
    let app = app(true);
    assert_eq!(
        call(&app, Req::get("/v1/me", Some(UI))).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            Req {
                token: Some("made-up"),
                ..Req::get("/v1/me", Some(UI))
            }
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn key_is_bound_to_its_origin() {
    let app = app(true);
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let (s, _, _) = call(
        &app,
        Req::post("/v1/auth/challenge", OTHER, json!({ "keyId": key_id })),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "same key, another allowed origin"
    );
}

#[tokio::test]
async fn stolen_token_does_not_work_from_another_origin() {
    let app = app(true);
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;
    let (s, _, _) = call(
        &app,
        Req {
            token: Some(&token),
            ..Req::get("/v1/me", Some(OTHER))
        },
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn forged_signature_and_replayed_nonce_are_rejected() {
    let app = app(true);
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let (_, v, _) = call(
        &app,
        Req::post("/v1/auth/challenge", UI, json!({ "keyId": key_id })),
    )
    .await;
    let nonce = v["nonce"].as_str().unwrap().to_owned();
    let forged = Browser::new().sign(&nonce); // signed by another key
    let (s, _, _) = call(
        &app,
        Req::post(
            "/v1/auth/session",
            UI,
            json!({ "keyId": key_id, "nonce": nonce, "signature": forged }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "forged signature");
    let (s, _, _) = call(
        &app,
        Req::post(
            "/v1/auth/session",
            UI,
            json!({ "keyId": key_id, "nonce": nonce, "signature": browser.sign(&nonce) }),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "the nonce was already used, even with a valid signature"
    );
}

#[tokio::test]
async fn revoke_kills_sessions_and_login() {
    let app = app(true);
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;
    let revoke = Req {
        token: Some(&token),
        ..Req::post("/v1/pairs/revoke", UI, json!({ "keyId": key_id }))
    };
    assert_eq!(call(&app, revoke).await.0, StatusCode::OK);
    assert_eq!(
        call(
            &app,
            Req {
                token: Some(&token),
                ..Req::get("/v1/me", Some(UI))
            }
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            Req::post("/v1/auth/challenge", UI, json!({ "keyId": key_id }))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn dangerous_operation_follows_the_dialog() {
    for (allows, expected) in [(true, StatusCode::OK), (false, StatusCode::FORBIDDEN)] {
        // Pair with an approving dialog, then roll back through an app whose dialog says `allows`.
        let dir = temp_dir();
        let browser = Browser::new();
        let pairing = router(
            Config {
                port: PORT,
                allowed_origins: vec![UI.into()],
                extra_origins: vec![],
            },
            Auth::load(dir.clone(), Arc::new(Always(true))).unwrap(),
            Arc::new(RwLock::new(None)),
            common::backend(),
        );
        let key_id = pair(&pairing, &browser, UI).await;
        let acting = router(
            Config {
                port: PORT,
                allowed_origins: vec![UI.into()],
                extra_origins: vec![],
            },
            Auth::load(dir, Arc::new(Always(allows))).unwrap(),
            Arc::new(RwLock::new(None)),
            common::backend(),
        );
        let token = login(&acting, &browser, &key_id, UI).await;
        let req = Req {
            token: Some(&token),
            ..Req::post(
                "/v1/rollback",
                UI,
                json!({ "project": "demo", "commit": "abcdef1" }),
            )
        };
        assert_eq!(
            call(&acting, req).await.0,
            expected,
            "dialog allows = {allows}"
        );
    }
}

#[tokio::test]
async fn same_origin_get_without_origin_needs_the_marker() {
    let app = app(true);
    let browser = Browser::new();
    let self_origin = "http://127.0.0.1:47801";
    let (_, v, _) = call(
        &app,
        Req::post(
            "/v1/pair/start",
            self_origin,
            json!({ "publicKeyJwk": browser.jwk() }),
        ),
    )
    .await;
    let path = format!("/v1/pair/status?id={}", v["pairId"].as_str().unwrap());
    assert_eq!(
        call(
            &app,
            Req {
                marker: true,
                ..Req::get(&path, None)
            }
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, Req::get(&path, None)).await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn oversized_body_is_rejected() {
    let app = app(true);
    let big = "x".repeat(vf_server::BODY_LIMIT + 1);
    let (s, _, _) = call(
        &app,
        Req::post("/v1/pair/start", UI, json!({ "data": big })),
    )
    .await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn project_routes_need_a_session_and_check_names() {
    let app = app(true);
    for path in ["/v1/projects", "/v1/journal?project=demo", "/v1/models"] {
        assert_eq!(
            call(&app, Req::get(path, Some(UI))).await.0,
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
        assert_eq!(
            call(&app, Req::get(path, Some(EVIL))).await.0,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;
    let get = |path| Req {
        token: Some(&token),
        ..Req::get(path, Some(UI))
    };
    let (s, v, _) = call(&app, get("/v1/projects")).await;
    assert_eq!(
        (s, v["projects"][0].as_str()),
        (StatusCode::OK, Some("demo"))
    );
    assert_eq!(
        call(&app, get("/v1/journal?project=demo")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, get("/v1/journal?project=..")).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn rollback_needs_a_session_and_the_dialog() {
    let body = json!({ "project": "demo", "commit": "abcdef1" });
    for (allows, expected) in [(false, StatusCode::FORBIDDEN), (true, StatusCode::OK)] {
        let app = app(allows);
        assert_eq!(
            call(&app, Req::post("/v1/rollback", UI, body.clone()))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        if !allows {
            // Pairing itself needs an approving dialog: pair through an approving app with the same state.
            continue;
        }
        let browser = Browser::new();
        let key_id = pair(&app, &browser, UI).await;
        let token = login(&app, &browser, &key_id, UI).await;
        let (s, _, _) = call(
            &app,
            Req {
                token: Some(&token),
                ..Req::post("/v1/rollback", UI, body.clone())
            },
        )
        .await;
        assert_eq!(s, expected);
    }
}

#[tokio::test]
async fn projects_need_a_grant_from_the_dialog() {
    let dir = temp_dir();
    let browser = Browser::new();
    let make = |allows: bool| {
        router(
            Config {
                port: PORT,
                allowed_origins: vec![UI.into()],
                extra_origins: vec![],
            },
            Auth::load(dir.clone(), Arc::new(Always(allows))).unwrap(),
            Arc::new(RwLock::new(None)),
            common::backend(),
        )
    };
    let key_id = pair(&make(true), &browser, UI).await;
    // A denying dialog: no access to the project's journal.
    let denying = make(false);
    let token = login(&denying, &browser, &key_id, UI).await;
    fn get(token: &str) -> Req<'_> {
        Req {
            token: Some(token),
            ..Req::get("/v1/journal?project=demo", Some(UI))
        }
    }
    assert_eq!(call(&denying, get(&token)).await.0, StatusCode::FORBIDDEN);
    // Allowed once: remembered for this browser, even with a denying dialog later.
    let allowing = make(true);
    let token = login(&allowing, &browser, &key_id, UI).await;
    assert_eq!(call(&allowing, get(&token)).await.0, StatusCode::OK);
    let denying = make(false);
    let token = login(&denying, &browser, &key_id, UI).await;
    assert_eq!(call(&denying, get(&token)).await.0, StatusCode::OK);
    let (_, v, _) = call(
        &denying,
        Req {
            token: Some(&token),
            ..Req::get("/v1/pairs", Some(UI))
        },
    )
    .await;
    assert_eq!(v[0]["projects"], json!(["demo"]));
}

/// Everything logged by the test binary, at every level.
fn captured_logs() -> Arc<std::sync::Mutex<Vec<u8>>> {
    static LOGS: std::sync::OnceLock<Arc<std::sync::Mutex<Vec<u8>>>> = std::sync::OnceLock::new();
    LOGS.get_or_init(|| {
        let buf = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = buf.clone();
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || LogWriter(writer.clone()))
            .try_init();
        buf
    })
    .clone()
}

struct LogWriter(Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for LogWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn secrets_never_reach_the_logs() {
    let logs = captured_logs();
    let app = app(true);
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;
    for (path, origin) in [
        ("/v1/me", UI),
        ("/v1/journal?project=demo", UI),
        ("/v1/me", EVIL),
    ] {
        call(
            &app,
            Req {
                token: Some(&token),
                ..Req::get(path, Some(origin))
            },
        )
        .await;
    }
    call(
        &app,
        Req {
            token: Some(&token),
            host: "evil.test",
            ..Req::get("/v1/me", Some(UI))
        },
    )
    .await;

    // AI API key never reaches the logs or API responses
    let api_key = "sk-or-v1-super-secret-openrouter-key-98765";
    let auth = Auth::load(
        temp_dir(),
        Arc::new(DialogWithSecret {
            allow: true,
            secret: Some(api_key.into()),
        }),
    )
    .unwrap();
    let app_secret = router(
        Config {
            port: PORT,
            allowed_origins: vec![UI.into()],
            extra_origins: vec![],
        },
        auth,
        Arc::new(RwLock::new(None)),
        common::backend(),
    );
    let key_id_secret = pair(&app_secret, &browser, UI).await;
    let token_secret = login(&app_secret, &browser, &key_id_secret, UI).await;
    let (status, connect_res, _) = call(
        &app_secret,
        Req {
            token: Some(&token_secret),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "openrouter" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(connect_res["started"], json!(true));
    assert!(
        !connect_res.to_string().contains(api_key),
        "API key leaked in ai/connect response"
    );

    let (_, ai_res, _) = call(
        &app_secret,
        Req {
            token: Some(&token_secret),
            ..Req::get("/v1/ai", Some(UI))
        },
    )
    .await;
    assert!(
        !ai_res.to_string().contains(api_key),
        "API key leaked in ai status response"
    );

    let (_, models_res, _) = call(
        &app_secret,
        Req {
            token: Some(&token_secret),
            ..Req::get("/v1/models", Some(UI))
        },
    )
    .await;
    assert!(
        !models_res.to_string().contains(api_key),
        "API key leaked in models response"
    );

    // Failed verification with invalid key: error message and logs never contain the key
    let invalid_key = "sk-or-v1-invalid-secret-key-11111";
    let auth_invalid = Auth::load(
        temp_dir(),
        Arc::new(DialogWithSecret {
            allow: true,
            secret: Some(invalid_key.into()),
        }),
    )
    .unwrap();
    let app_invalid = router(
        Config {
            port: PORT,
            allowed_origins: vec![UI.into()],
            extra_origins: vec![],
        },
        auth_invalid,
        Arc::new(RwLock::new(None)),
        common::backend(),
    );
    let key_id_invalid = pair(&app_invalid, &browser, UI).await;
    let token_invalid = login(&app_invalid, &browser, &key_id_invalid, UI).await;
    let (status, err_res, _) = call(
        &app_invalid,
        Req {
            token: Some(&token_invalid),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "openrouter" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        !err_res.to_string().contains(invalid_key),
        "API key leaked in 400 error response"
    );

    let text = String::from_utf8_lossy(&logs.lock().unwrap()).into_owned();
    assert!(text.contains("rejected"), "logging is captured: {text}");
    assert!(!text.contains(&token), "session token in the logs");
    assert!(!text.contains(api_key), "API key in the logs");
    assert!(!text.contains(invalid_key), "invalid API key in the logs");
}

#[tokio::test]
async fn extra_origins_in_hello_and_guard_logs() {
    let auth = Auth::load(temp_dir(), Arc::new(Always(true))).unwrap();
    let app = router(
        Config {
            port: PORT,
            allowed_origins: vec![UI.into(), "http://localhost:5173".into()],
            extra_origins: vec!["http://localhost:5173".into()],
        },
        auth,
        Arc::new(RwLock::new(None)),
        common::backend(),
    );
    let (s, v, acao) = call(&app, Req::get("/v1/hello", Some("http://localhost:5173"))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(acao.as_deref(), Some("http://localhost:5173"));
    assert_eq!(v["extraOriginsActive"], json!(true));
    assert!(v.get("extraOrigins").is_none(), "the list is not exposed");
}

/// A dialog nobody answers: pairing requests stay waiting.
struct Unanswered;
impl ConfirmDialog for Unanswered {
    fn confirm(&self, _title: &str, _text: &str) -> bool {
        std::thread::sleep(std::time::Duration::from_secs(3));
        false
    }
}

#[tokio::test]
async fn a_page_cannot_flood_the_screen_with_pairing_dialogs() {
    let app = router(
        Config {
            port: PORT,
            allowed_origins: vec![UI.into(), OTHER.into()],
            extra_origins: vec![],
        },
        Auth::load(temp_dir(), Arc::new(Unanswered)).unwrap(),
        Arc::new(RwLock::new(None)),
        common::backend(),
    );
    let start = |origin: &'static str| {
        let jwk = Browser::new().jwk();
        Req::post("/v1/pair/start", origin, json!({ "publicKeyJwk": jwk }))
    };
    for _ in 0..vf_auth::MAX_WAITING_PAIRINGS {
        assert_eq!(call(&app, start(UI)).await.0, StatusCode::OK);
    }
    assert_eq!(call(&app, start(UI)).await.0, StatusCode::TOO_MANY_REQUESTS);
    // Another origin is not affected.
    assert_eq!(call(&app, start(OTHER)).await.0, StatusCode::OK);
}

#[tokio::test]
async fn files_need_a_session_and_a_project_grant() {
    let dir = temp_dir();
    let browser = Browser::new();
    let make = |allows: bool| {
        router(
            Config {
                port: PORT,
                allowed_origins: vec![UI.into()],
                extra_origins: vec![],
            },
            Auth::load(dir.clone(), Arc::new(Always(allows))).unwrap(),
            Arc::new(RwLock::new(None)),
            common::backend(),
        )
    };
    let key_id = pair(&make(true), &browser, UI).await;
    let denying = make(false);
    for path in [
        "/v1/file?project=demo&path=a.txt",
        "/v1/diff?project=demo&path=a.txt",
        "/v1/files?project=demo",
        "/v1/graph?project=demo",
    ] {
        assert_eq!(
            call(&denying, Req::get(path, Some(UI))).await.0,
            StatusCode::UNAUTHORIZED
        );
        let token = login(&denying, &browser, &key_id, UI).await;
        let r = Req {
            token: Some(&token),
            ..Req::get(path, Some(UI))
        };
        assert_eq!(call(&denying, r).await.0, StatusCode::FORBIDDEN);
    }
    let allowing = make(true);
    let token = login(&allowing, &browser, &key_id, UI).await;
    let get = |path: &'static str| Req {
        token: Some(&token),
        ..Req::get(path, Some(UI))
    };
    let (status, v, _) = call(&allowing, get("/v1/file?project=demo&path=a.txt")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["content"], json!("text"));
    assert_eq!(
        call(&allowing, get("/v1/diff?project=demo&path=a.txt"))
            .await
            .0,
        StatusCode::OK
    );
    let (status, v, _) = call(&allowing, get("/v1/files?project=demo")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["files"], json!(["README.md", "src/main.rs"]));
    assert_eq!(v["truncated"], json!(false));
    // Describing the map needs the project grant too.
    let body = json!({ "project": "demo" });
    assert_eq!(
        call(&denying, Req::post("/v1/graph/describe", UI, body.clone()))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let denied = login(&denying, &browser, &key_id, UI).await;
    let r = Req {
        token: Some(&denied),
        ..Req::post("/v1/graph/describe", UI, body.clone())
    };
    assert_eq!(call(&denying, r).await.0, StatusCode::FORBIDDEN);
    let r = Req {
        token: Some(&token),
        ..Req::post("/v1/graph/describe", UI, body)
    };
    assert_eq!(call(&allowing, r).await.0, StatusCode::BAD_REQUEST);
    // A file the engine refuses comes back as 403, not as content.
    assert_eq!(
        call(&allowing, get("/v1/file?project=demo&path=.env"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn import_progress_belongs_to_the_session_that_imports() {
    let app = app(true);
    let path = "/v1/projects/import/progress?repo=octo/notes-app";
    assert_eq!(
        call(&app, Req::get(path, Some(UI))).await.0,
        StatusCode::UNAUTHORIZED
    );
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app,
        Req {
            token: Some(&token),
            ..Req::get(path, Some(UI))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["state"], json!("downloading"));
    // The owner is the session's key and origin, never something the request names.
    assert_eq!(v["error"], json!(format!("{key_id} {UI}|octo/notes-app")));
    let other = Browser::new();
    let other_key = pair(&app, &other, UI).await;
    let other_token = login(&app, &other, &other_key, UI).await;
    let (_, v, _) = call(
        &app,
        Req {
            token: Some(&other_token),
            ..Req::get(path, Some(UI))
        },
    )
    .await;
    assert_eq!(
        v["error"],
        json!(format!("{other_key} {UI}|octo/notes-app"))
    );
}

#[tokio::test]
async fn the_environment_of_a_project_needs_its_grant() {
    let dir = temp_dir();
    let browser = Browser::new();
    let make = |allows: bool| {
        router(
            Config {
                port: PORT,
                allowed_origins: vec![UI.into()],
                extra_origins: vec![],
            },
            Auth::load(dir.clone(), Arc::new(Always(allows))).unwrap(),
            Arc::new(RwLock::new(None)),
            common::backend(),
        )
    };
    let key_id = pair(&make(true), &browser, UI).await;
    let denying = make(false);
    for path in ["/v1/environment", "/v1/environment?project=demo"] {
        assert_eq!(
            call(&denying, Req::get(path, Some(UI))).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let denied = login(&denying, &browser, &key_id, UI).await;
    let r = Req {
        token: Some(&denied),
        ..Req::get("/v1/environment?project=demo", Some(UI))
    };
    assert_eq!(call(&denying, r).await.0, StatusCode::FORBIDDEN);
    // Every tool the agent can install needs no project grant.
    let r = Req {
        token: Some(&denied),
        ..Req::get("/v1/environment", Some(UI))
    };
    let (status, v, _) = call(&denying, r).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["tools"].as_array().map(Vec::len), Some(2));
    let allowing = make(true);
    let token = login(&allowing, &browser, &key_id, UI).await;
    let r = Req {
        token: Some(&token),
        ..Req::get("/v1/environment?project=demo", Some(UI))
    };
    let (status, v, _) = call(&allowing, r).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["tools"][0]["spec"], json!("node@lts"));
    assert_eq!(v["totalBytes"], json!(10));
    assert_eq!(v["totalDownloadBytes"], json!(4));
}

#[tokio::test]
async fn ai_status_needs_a_session() {
    let dir = temp_dir();
    let browser = Browser::new();
    let app = router(
        Config {
            port: PORT,
            allowed_origins: vec![UI.into()],
            extra_origins: vec![],
        },
        Auth::load(dir, Arc::new(Always(true))).unwrap(),
        Arc::new(RwLock::new(None)),
        common::backend(),
    );
    assert_eq!(
        call(&app, Req::get("/v1/ai", Some(UI))).await.0,
        StatusCode::UNAUTHORIZED
    );
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app,
        Req {
            token: Some(&token),
            ..Req::get("/v1/ai", Some(UI))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["ready"], json!(false));
}

#[tokio::test]
async fn ai_sign_in_needs_a_session_and_the_dialog() {
    let dir = temp_dir();
    let browser = Browser::new();
    let make = |allows: bool| {
        router(
            Config {
                port: PORT,
                allowed_origins: vec![UI.into()],
                extra_origins: vec![],
            },
            Auth::load(dir.clone(), Arc::new(Always(allows))).unwrap(),
            Arc::new(RwLock::new(None)),
            common::backend(),
        )
    };
    let key_id = pair(&make(true), &browser, UI).await;
    let body = json!({ "provider": "claude" });
    let denying = make(false);
    assert_eq!(
        call(&denying, Req::post("/v1/ai/connect", UI, body.clone()))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let token = login(&denying, &browser, &key_id, UI).await;
    let r = Req {
        token: Some(&token),
        ..Req::post("/v1/ai/connect", UI, body.clone())
    };
    assert_eq!(call(&denying, r).await.0, StatusCode::FORBIDDEN);
    let r = Req {
        token: Some(&token),
        ..Req::post(
            "/v1/ai/connect",
            UI,
            json!({ "provider": "unknown_provider" }),
        )
    };
    assert_eq!(call(&denying, r).await.0, StatusCode::BAD_REQUEST);
    let r = Req {
        token: Some(&token),
        ..Req::post("/v1/ai/connect", UI, json!({ "provider": "openrouter" }))
    };
    assert_eq!(call(&denying, r).await.0, StatusCode::FORBIDDEN);
    let allowing = make(true);
    let token = login(&allowing, &browser, &key_id, UI).await;
    let r = Req {
        token: Some(&token),
        ..Req::post("/v1/ai/connect", UI, body)
    };
    let (status, v, _) = call(&allowing, r).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(true));
    // OpenRouter with cancelled dialog returns started: false
    let r = Req {
        token: Some(&token),
        ..Req::post("/v1/ai/connect", UI, json!({ "provider": "openrouter" }))
    };
    let (status, v, _) = call(&allowing, r).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(false));
}

#[tokio::test]
async fn ai_connect_cli_home_mode_switch_requires_confirm_for_user_not_agent() {
    let dir = temp_dir();
    let browser = Browser::new();
    let make = |allow: bool| {
        router(
            Config {
                port: PORT,
                allowed_origins: vec![UI.into()],
                extra_origins: vec![],
            },
            Auth::load(dir.clone(), Arc::new(Always(allow))).unwrap(),
            Arc::new(RwLock::new(None)),
            common::backend(),
        )
    };
    let key_id = pair(&make(true), &browser, UI).await;
    let denying = make(false);
    let token = login(&denying, &browser, &key_id, UI).await;

    // Switching to user with denying dialog -> FORBIDDEN action_not_confirmed
    let r = Req {
        token: Some(&token),
        ..Req::post(
            "/v1/ai/connect",
            UI,
            json!({ "provider": "claude", "mode": "user" }),
        )
    };
    let (status, v, _) = call(&denying, r).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(v["code"], json!("action_not_confirmed"));

    // Switching to agent with denying dialog -> OK started: true (requires no confirm dialog)
    let r = Req {
        token: Some(&token),
        ..Req::post(
            "/v1/ai/connect",
            UI,
            json!({ "provider": "claude", "mode": "agent" }),
        )
    };
    let (status, v, _) = call(&denying, r).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(true));

    // Switching to user with allowing dialog -> OK started: true
    let allowing = make(true);
    let token = login(&allowing, &browser, &key_id, UI).await;
    let r = Req {
        token: Some(&token),
        ..Req::post(
            "/v1/ai/connect",
            UI,
            json!({ "provider": "claude", "mode": "user" }),
        )
    };
    let (status, v, _) = call(&allowing, r).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(true));
}

#[tokio::test]
async fn openrouter_connect_with_key_and_cancel_or_empty() {
    let browser = Browser::new();
    let make = |secret: Option<String>| {
        let auth = Auth::load(
            temp_dir(),
            Arc::new(DialogWithSecret {
                allow: true,
                secret,
            }),
        )
        .unwrap();
        router(
            Config {
                port: PORT,
                allowed_origins: vec![UI.into()],
                extra_origins: vec![],
            },
            auth,
            Arc::new(RwLock::new(None)),
            common::backend(),
        )
    };

    // 1. Cancelled dialog (returns None) -> started: false
    let app_cancel = make(None);
    let key_id = pair(&app_cancel, &browser, UI).await;
    let token = login(&app_cancel, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app_cancel,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "openrouter" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(false));

    // 2. Empty secret string -> started: false
    let app_empty = make(Some("   ".into()));
    let key_id = pair(&app_empty, &browser, UI).await;
    let token = login(&app_empty, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app_empty,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "openrouter" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(false));

    // 3. Valid secret provided -> started: true
    let app_valid = make(Some("sk-or-valid-test-key".into()));
    let key_id = pair(&app_valid, &browser, UI).await;
    let token = login(&app_valid, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app_valid,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "openrouter" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(true));

    // 4. Invalid key -> 400 Bad Request immediately
    let app_invalid = make(Some("invalid-key".into()));
    let key_id = pair(&app_invalid, &browser, UI).await;
    let token = login(&app_invalid, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app_invalid,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "openrouter" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("invalid OpenRouter key")
    );
}

#[tokio::test]
async fn gemini_connect_with_key_and_disconnect() {
    let browser = Browser::new();
    let make = |secret: Option<String>| {
        let auth = Auth::load(
            temp_dir(),
            Arc::new(DialogWithSecret {
                allow: true,
                secret,
            }),
        )
        .unwrap();
        router(
            Config {
                port: PORT,
                allowed_origins: vec![UI.into()],
                extra_origins: vec![],
            },
            auth,
            Arc::new(RwLock::new(None)),
            common::backend(),
        )
    };

    // 1. Cancelled dialog -> started: false
    let app_cancel = make(None);
    let key_id = pair(&app_cancel, &browser, UI).await;
    let token = login(&app_cancel, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app_cancel,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "gemini" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(false));

    // 2. Valid secret -> started: true
    let app_valid = make(Some("valid-gemini-key".into()));
    let key_id = pair(&app_valid, &browser, UI).await;
    let token = login(&app_valid, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app_valid,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "gemini" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(true));

    // 3. Disconnect route
    let (status, v, _) = call(
        &app_valid,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/disconnect", UI, json!({ "provider": "gemini" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["ok"], json!(true));

    // 4. Invalid key -> 400 Bad Request
    let app_invalid = make(Some("invalid-key".into()));
    let key_id = pair(&app_invalid, &browser, UI).await;
    let token = login(&app_invalid, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app_invalid,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "gemini" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("invalid Google AI Studio key")
    );
}

#[tokio::test]
async fn import_rate_limit_returns_429_with_structured_error() {
    let app = app(true);
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;
    let (status, v, _) = call(
        &app,
        Req {
            token: Some(&token),
            ..Req::post("/v1/projects/import", UI, json!({ "repo": "rate/limit" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(v["code"], json!("rate_limit"));
    assert_eq!(v["resetMinutes"], json!(42));
    assert!(v["error"].as_str().unwrap().contains("try again in 42 min"));
}

struct CustomAiDialog {
    allow_pair: bool,
    allow_confirm: bool,
    endpoint: Option<String>,
    secret: Option<String>,
    model: Option<String>,
}

impl ConfirmDialog for CustomAiDialog {
    fn confirm(&self, title: &str, _text: &str) -> bool {
        if title.contains("browser") || title.contains("браузер") {
            self.allow_pair
        } else {
            self.allow_confirm
        }
    }

    fn ask_text(&self, title: &str, _text: &str) -> Option<String> {
        if title.contains("value") || _text.contains("URL") || _text.contains("адрес") {
            self.endpoint.clone()
        } else {
            self.model.clone()
        }
    }

    fn ask_secret_optional(&self, _title: &str, _text: &str) -> Option<Secret<String>> {
        self.secret.as_ref().map(|s| Secret::new(s.clone()))
    }
}

#[tokio::test]
async fn custom_ai_endpoint_rejects_non_loopback_http() {
    let dir = temp_dir();
    let browser = Browser::new();
    let dialog = Arc::new(CustomAiDialog {
        allow_pair: true,
        allow_confirm: true,
        endpoint: Some("http://evil.com/v1".into()),
        secret: Some("".into()),
        model: Some("llama3".into()),
    });
    let app = router(
        Config {
            port: PORT,
            allowed_origins: vec![UI.into()],
            extra_origins: vec![],
        },
        Auth::load(dir.clone(), dialog).unwrap(),
        Arc::new(RwLock::new(None)),
        common::backend(),
    );
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;

    let (status, v, _) = call(
        &app,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "custom" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(v["code"], json!("invalid_endpoint"));
    assert!(v["error"].as_str().unwrap().contains("loopback"));
}

#[tokio::test]
async fn custom_ai_endpoint_cannot_be_injected_from_request_body() {
    let dir = temp_dir();
    let browser = Browser::new();
    // Native dialog returns None for endpoint
    let dialog = Arc::new(CustomAiDialog {
        allow_pair: true,
        allow_confirm: true,
        endpoint: None,
        secret: Some("".into()),
        model: None,
    });
    let app = router(
        Config {
            port: PORT,
            allowed_origins: vec![UI.into()],
            extra_origins: vec![],
        },
        Auth::load(dir.clone(), dialog).unwrap(),
        Arc::new(RwLock::new(None)),
        common::backend(),
    );
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;

    // Page attempts to pass endpoint in body
    let (status, v, _) = call(
        &app,
        Req {
            token: Some(&token),
            ..Req::post(
                "/v1/ai/connect",
                UI,
                json!({ "provider": "custom", "endpoint": "http://localhost:11434" }),
            )
        },
    )
    .await;
    // Dialog returned None, so agent dialog was cancelled, returned started: false
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["started"], json!(false));
}

#[tokio::test]
async fn custom_ai_endpoint_requires_confirmation_with_warning() {
    let dir = temp_dir();
    let browser = Browser::new();
    // User clicks Block on warning confirmation
    let dialog = Arc::new(CustomAiDialog {
        allow_pair: true,
        allow_confirm: false,
        endpoint: Some("http://localhost:11434/v1".into()),
        secret: Some("".into()),
        model: Some("llama3".into()),
    });
    let app = router(
        Config {
            port: PORT,
            allowed_origins: vec![UI.into()],
            extra_origins: vec![],
        },
        Auth::load(dir.clone(), dialog).unwrap(),
        Arc::new(RwLock::new(None)),
        common::backend(),
    );
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;

    let (status, v, _) = call(
        &app,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/connect", UI, json!({ "provider": "custom" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(v["code"], json!("action_not_confirmed"));
}

#[tokio::test]
async fn custom_ai_disconnect_route() {
    let app = app(true);
    let browser = Browser::new();
    let key_id = pair(&app, &browser, UI).await;
    let token = login(&app, &browser, &key_id, UI).await;

    let (status, v, _) = call(
        &app,
        Req {
            token: Some(&token),
            ..Req::post("/v1/ai/disconnect", UI, json!({ "provider": "custom" }))
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["ok"], json!(true));
}
