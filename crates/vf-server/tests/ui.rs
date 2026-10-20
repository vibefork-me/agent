//! The fallback UI: only a signed, unmodified bundle is served, with security headers; one canonical
//! address. Bundles are downloaded from a real HTTP server started by the test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Redirect;
use axum::routing::get;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use tower::ServiceExt as _;
use vf_auth::Auth;
use vf_core::ConfirmDialog;
mod common;

use vf_server::{Config, UiSlot, router};
use vf_updater::UiBundle;
use vf_updater::signing::{Manifest, sha256_hex, sign};

struct No;
impl ConfirmDialog for No {
    fn confirm(&self, _: &str, _: &str) -> bool {
        false
    }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let mut b = [0u8; 8];
    rand::RngCore::fill_bytes(&mut OsRng, &mut b);
    std::env::temp_dir().join(format!("vf-ui-{tag}-{}", u64::from_le_bytes(b)))
}

/// Serves `files` (+ signed manifest) at `http://127.0.0.1:<port>/b/…`; `tamper` replaces a file
/// after signing; `redirect` makes the manifest a redirect.
async fn bundle_server(
    key: &SigningKey,
    files: &[(&str, &str)],
    tamper: Option<(&str, &str)>,
    redirect: bool,
) -> String {
    let manifest = Manifest {
        version: "t1".into(),
        files: files
            .iter()
            .map(|(p, c)| ((*p).to_owned(), sha256_hex(c.as_bytes())))
            .collect::<BTreeMap<_, _>>(),
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let sig = sign(&manifest_bytes, key);
    let mut app = Router::new();
    app = if redirect {
        app.route(
            "/b/manifest.json",
            get(|| async { Redirect::temporary("http://example.com/manifest.json") }),
        )
    } else {
        let m = manifest_bytes.clone();
        app.route("/b/manifest.json", get(move || async move { m }))
    };
    app = app.route("/b/manifest.sig", get(move || async move { sig }));
    for (path, content) in files {
        let body = match tamper {
            Some((p, evil)) if p == *path => evil.to_owned(),
            _ => (*content).to_owned(),
        };
        app = app.route(&format!("/b/{path}"), get(move || async move { body }));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://127.0.0.1:{port}/b")
}

fn agent(ui: UiSlot) -> Router {
    let auth = Auth::load(temp_dir("auth"), Arc::new(No)).unwrap();
    router(
        Config {
            port: 47801,
            allowed_origins: vec![],
        },
        auth,
        ui,
        common::backend(),
    )
}

async fn get_ui(app: &Router, host: &str, path: &str) -> axum::response::Response {
    let req = Request::builder()
        .uri(path)
        .header("host", host)
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(req).await.unwrap()
}

const FILES: &[(&str, &str)] = &[("index.html", "<h1>ui</h1>"), ("app.js", "console.log(1)")];

#[tokio::test]
async fn signed_bundle_is_served_with_security_headers() {
    let key = SigningKey::generate(&mut OsRng);
    let url = bundle_server(&key, FILES, None, false).await;
    let bundle = UiBundle::download(&url, &key.verifying_key(), &temp_dir("cache"))
        .await
        .unwrap();
    let app = agent(Arc::new(RwLock::new(Some(bundle))));

    let res = get_ui(&app, "127.0.0.1:47801", "/ui/").await;
    assert_eq!(res.status(), StatusCode::OK);
    let h = res.headers();
    assert_eq!(h["x-frame-options"], "DENY");
    assert!(
        h["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(
        get_ui(&app, "127.0.0.1:47801", "/ui/app.js").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get_ui(&app, "127.0.0.1:47801", "/ui/other.js")
            .await
            .status(),
        StatusCode::NOT_FOUND,
        "only manifest files"
    );
    assert_eq!(
        get_ui(&app, "127.0.0.1:47801", "/").await.status(),
        StatusCode::SEE_OTHER
    );
}

#[tokio::test]
async fn tampered_file_wrong_key_and_redirects_are_rejected() {
    let key = SigningKey::generate(&mut OsRng);
    let tampered = bundle_server(&key, FILES, Some(("app.js", "steal()")), false).await;
    assert!(
        UiBundle::download(&tampered, &key.verifying_key(), &temp_dir("c1"))
            .await
            .is_err(),
        "tampered file"
    );
    let honest = bundle_server(&key, FILES, None, false).await;
    let other = SigningKey::generate(&mut OsRng);
    assert!(
        UiBundle::download(&honest, &other.verifying_key(), &temp_dir("c2"))
            .await
            .is_err(),
        "signed by another key"
    );
    let redirecting = bundle_server(&key, FILES, None, true).await;
    assert!(
        UiBundle::download(&redirecting, &key.verifying_key(), &temp_dir("c3"))
            .await
            .is_err(),
        "redirects are not followed"
    );
}

#[tokio::test]
async fn cached_bundle_is_verified_again_on_load() {
    let key = SigningKey::generate(&mut OsRng);
    let url = bundle_server(&key, FILES, None, false).await;
    let cache = temp_dir("cache");
    UiBundle::download(&url, &key.verifying_key(), &cache)
        .await
        .unwrap();
    assert!(UiBundle::load_cached(&cache, &key.verifying_key()).is_some());
    std::fs::write(cache.join("t1").join("app.js"), "steal()").unwrap();
    assert!(
        UiBundle::load_cached(&cache, &key.verifying_key()).is_none(),
        "modified cache is refused"
    );
}

#[tokio::test]
async fn one_canonical_address_and_no_bundle_means_503() {
    let app = agent(Arc::new(RwLock::new(None)));
    let res = get_ui(&app, "localhost:47801", "/ui/task.html").await;
    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        res.headers()["location"],
        "http://127.0.0.1:47801/ui/task.html"
    );
    assert_eq!(
        get_ui(&app, "127.0.0.1:47801", "/ui/").await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        get_ui(&app, "evil.test", "/ui/").await.status(),
        StatusCode::FORBIDDEN,
        "host check applies to the UI too"
    );
}
