//! UI bundle freshness, rollback, and certified subkey handling.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::routing::get;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use vf_updater::Manifest;
use vf_updater::signing::{SubkeyCert, sha256_hex, sign, sign_subkey_cert};
use vf_updater::{SignatureError, UiBundle};

async fn bundle_server(
    root: &SigningKey,
    signer: &SigningKey,
    version: &str,
    expires_at: Option<i64>,
    cert_purpose: Option<&str>,
    file_requests: Arc<AtomicUsize>,
) -> String {
    let content = b"bundle bytes".to_vec();
    let files = BTreeMap::from([("index.html".to_owned(), sha256_hex(&content))]);
    let manifest = Manifest {
        version: version.to_owned(),
        files,
        expires_at,
    };
    let manifest_json = serde_json::to_vec(&manifest).unwrap();
    let manifest_sig = sign(&manifest_json, signer);
    let request_counter = file_requests.clone();
    let mut app = Router::new()
        .route(
            "/b/manifest.json",
            get(move || async move { manifest_json }),
        )
        .route("/b/manifest.sig", get(move || async move { manifest_sig }))
        .route(
            "/b/index.html",
            get(move || {
                let counter = request_counter.clone();
                let content = content.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    content
                }
            }),
        );
    if let Some(purpose) = cert_purpose {
        let cert = SubkeyCert {
            public_key: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                signer.verifying_key().to_bytes(),
            ),
            expires_at: i64::MAX,
            purpose: purpose.to_owned(),
        };
        let cert_json = serde_json::to_vec(&cert).unwrap();
        let cert_sig = sign_subkey_cert(&cert_json, root);
        app = app
            .route("/b/subkey.cert.json", get(move || async move { cert_json }))
            .route("/b/subkey.cert.sig", get(move || async move { cert_sig }));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://127.0.0.1:{port}/b")
}

fn counter() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nonce: u64 = rand::random();
    std::env::temp_dir().join(format!("vf-bundle-{tag}-{nonce}"))
}

#[tokio::test]
async fn expired_download_is_rejected_but_expired_cache_still_loads() {
    let root = SigningKey::generate(&mut OsRng);
    let network_file_requests = counter();
    let expired_url = bundle_server(
        &root,
        &root,
        "1.0.0",
        Some(0),
        None,
        network_file_requests.clone(),
    )
    .await;
    let cache = temp_dir("expiry");
    assert!(matches!(
        UiBundle::download(&expired_url, &root.verifying_key(), &cache).await,
        Err(vf_updater::bundle::BundleError::Signature(
            SignatureError::Expired
        ))
    ));
    assert_eq!(network_file_requests.load(Ordering::SeqCst), 0);

    let cache_dir = cache.join("1.0.0");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let manifest = Manifest {
        version: "1.0.0".into(),
        files: BTreeMap::from([("index.html".to_owned(), sha256_hex(b"cached"))]),
        expires_at: Some(0),
    };
    let manifest_json = serde_json::to_vec(&manifest).unwrap();
    std::fs::write(cache_dir.join("manifest.json"), &manifest_json).unwrap();
    std::fs::write(cache_dir.join("manifest.sig"), sign(&manifest_json, &root)).unwrap();
    std::fs::write(cache_dir.join("index.html"), b"cached").unwrap();
    std::fs::write(cache.join("current"), "1.0.0").unwrap();
    assert_eq!(
        UiBundle::load_cached(&cache, &root.verifying_key())
            .unwrap()
            .file("index.html"),
        Some(b"cached".as_slice())
    );
    let _ = std::fs::remove_dir_all(cache);
}

#[tokio::test]
async fn bundle_download_rejects_rollback_and_does_not_refetch_equal_version() {
    let root = SigningKey::generate(&mut OsRng);
    let cache = temp_dir("rollback");
    let initial_requests = counter();
    let initial_url =
        bundle_server(&root, &root, "2.0.0", None, None, initial_requests.clone()).await;
    assert_eq!(
        UiBundle::download(&initial_url, &root.verifying_key(), &cache)
            .await
            .unwrap()
            .version,
        "2.0.0"
    );
    assert_eq!(initial_requests.load(Ordering::SeqCst), 1);

    let old_requests = counter();
    let old_url = bundle_server(&root, &root, "1.9.0", None, None, old_requests.clone()).await;
    assert!(matches!(
        UiBundle::download(&old_url, &root.verifying_key(), &cache).await,
        Err(vf_updater::bundle::BundleError::Signature(
            SignatureError::Rollback
        ))
    ));
    assert_eq!(old_requests.load(Ordering::SeqCst), 0);

    let equal_requests = counter();
    let equal_url = bundle_server(&root, &root, "2.0.0", None, None, equal_requests.clone()).await;
    assert_eq!(
        UiBundle::download(&equal_url, &root.verifying_key(), &cache)
            .await
            .unwrap()
            .version,
        "2.0.0"
    );
    assert_eq!(equal_requests.load(Ordering::SeqCst), 0);

    let newer_url = bundle_server(&root, &root, "2.1.0", None, None, counter()).await;
    assert_eq!(
        UiBundle::download(&newer_url, &root.verifying_key(), &cache)
            .await
            .unwrap()
            .version,
        "2.1.0"
    );
    let _ = std::fs::remove_dir_all(cache);
}

#[tokio::test]
async fn subkey_signed_bundle_and_certificate_load_from_cache() {
    let root = SigningKey::generate(&mut OsRng);
    let subkey = SigningKey::generate(&mut OsRng);
    let url = bundle_server(&root, &subkey, "3.0.0", None, Some("bundle"), counter()).await;
    let cache = temp_dir("subkey");
    assert!(
        UiBundle::download(&url, &root.verifying_key(), &cache)
            .await
            .is_ok()
    );
    let version_dir = cache.join("3.0.0");
    assert!(version_dir.join("subkey.cert.json").is_file());
    assert!(version_dir.join("subkey.cert.sig").is_file());
    assert_eq!(
        UiBundle::load_cached(&cache, &root.verifying_key())
            .unwrap()
            .version,
        "3.0.0"
    );
    let _ = std::fs::remove_dir_all(cache);
}

#[tokio::test]
async fn ui_rejects_a_release_purpose_subkey() {
    let root = SigningKey::generate(&mut OsRng);
    let subkey = SigningKey::generate(&mut OsRng);
    let url = bundle_server(&root, &subkey, "1.0.0", None, Some("release"), counter()).await;
    assert!(matches!(
        UiBundle::download(&url, &root.verifying_key(), &temp_dir("purpose")).await,
        Err(vf_updater::bundle::BundleError::Signature(
            SignatureError::SubkeyCertMismatch
        ))
    ));
}
