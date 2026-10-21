//! Self-update from a signed release channel served by a local HTTP server.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;

use axum::Router;
use axum::routing::get;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use vf_updater::Manifest;
use vf_updater::release::{Release, asset_name, install, previous, rollback};
use vf_updater::signing::{SubkeyCert, sha256_hex, sign, sign_subkey_cert};

async fn channel(key: &SigningKey, version: &str, binary: Vec<u8>, served: Vec<u8>) -> String {
    let asset = asset_name("vibefork");
    let manifest = Manifest {
        version: version.into(),
        files: BTreeMap::from([(asset.clone(), sha256_hex(&binary))]),
        expires_at: None,
    };
    let m = serde_json::to_vec(&manifest).unwrap();
    let sig = sign(&m, key);
    let app = Router::new()
        .route("/stable/manifest.json", get(move || async move { m }))
        .route("/stable/manifest.sig", get(move || async move { sig }))
        .route(
            &format!("/stable/{asset}"),
            get(move || async move { served }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://127.0.0.1:{port}/stable")
}

async fn subkey_channel(root: &SigningKey, subkey: &SigningKey, purpose: &str) -> String {
    let asset = asset_name("vibefork");
    let binary = b"signed release";
    let manifest = Manifest {
        version: "10.0.0".into(),
        files: BTreeMap::from([(asset.clone(), sha256_hex(binary))]),
        expires_at: None,
    };
    let manifest_json = serde_json::to_vec(&manifest).unwrap();
    let manifest_sig = sign(&manifest_json, subkey);
    let cert = SubkeyCert {
        public_key: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            subkey.verifying_key().to_bytes(),
        ),
        expires_at: i64::MAX,
        purpose: purpose.into(),
    };
    let cert_json = serde_json::to_vec(&cert).unwrap();
    let cert_sig = sign_subkey_cert(&cert_json, root);
    let app = Router::new()
        .route(
            "/stable/manifest.json",
            get(move || async move { manifest_json }),
        )
        .route(
            "/stable/manifest.sig",
            get(move || async move { manifest_sig }),
        )
        .route(
            "/stable/subkey.cert.json",
            get(move || async move { cert_json }),
        )
        .route(
            "/stable/subkey.cert.sig",
            get(move || async move { cert_sig }),
        )
        .route(
            &format!("/stable/{asset}"),
            get(move || async move { binary.to_vec() }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://127.0.0.1:{port}/stable")
}

#[tokio::test]
async fn verified_binary_only() {
    let key = SigningKey::generate(&mut OsRng);
    let url = channel(&key, "9.9.9", b"good".to_vec(), b"good".to_vec()).await;
    let r = Release::fetch(&url, &key.verifying_key()).await.unwrap();
    assert_eq!(r.version(), "9.9.9");
    assert_eq!(r.download(&asset_name("vibefork")).await.unwrap(), b"good");
    assert!(
        r.download("vibefork-other-os").await.is_err(),
        "not in the manifest"
    );

    let tampered = channel(&key, "9.9.9", b"good".to_vec(), b"evil".to_vec()).await;
    let r = Release::fetch(&tampered, &key.verifying_key())
        .await
        .unwrap();
    assert!(
        r.download(&asset_name("vibefork")).await.is_err(),
        "hash mismatch"
    );

    let other = SigningKey::generate(&mut OsRng);
    assert!(
        Release::fetch(&url, &other.verifying_key()).await.is_err(),
        "another key"
    );
}

#[tokio::test]
async fn tampered_manifest_and_unsigned_local_channel_are_rejected() {
    let key = SigningKey::generate(&mut OsRng);
    let original = serde_json::to_vec(&Manifest {
        version: "1.0.0".into(),
        files: BTreeMap::from([(asset_name("vibefork"), sha256_hex(b"trusted"))]),
        expires_at: None,
    })
    .unwrap();
    let sig = sign(&original, &key);
    let mut tampered = original;
    tampered[0] ^= 1;
    let app = Router::new()
        .route("/local/manifest.json", get(move || async move { tampered }))
        .route("/local/manifest.sig", get(move || async move { sig }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    let local_override = format!("http://127.0.0.1:{port}/local");

    assert!(
        Release::fetch(&local_override, &key.verifying_key())
            .await
            .is_err(),
        "a local URL override cannot make a tampered or unsigned manifest trusted"
    );
}

#[tokio::test]
async fn release_fetch_uses_certified_release_subkey_and_checks_purpose() {
    let root = SigningKey::generate(&mut OsRng);
    let subkey = SigningKey::generate(&mut OsRng);
    let url = subkey_channel(&root, &subkey, "release").await;
    let release = Release::fetch(&url, &root.verifying_key()).await.unwrap();
    assert_eq!(release.version(), "10.0.0");
    assert_eq!(
        release.download(&asset_name("vibefork")).await.unwrap(),
        b"signed release"
    );

    let wrong_purpose = subkey_channel(&root, &subkey, "bundle").await;
    assert!(matches!(
        Release::fetch(&wrong_purpose, &root.verifying_key()).await,
        Err(vf_updater::release::UpdateError::Signature(
            vf_updater::SignatureError::SubkeyCertMismatch
        ))
    ));
}

#[test]
fn install_checks_the_new_binary_and_keeps_the_old_one() {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "vf-install-{}-{nanos}-{counter}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join(if cfg!(windows) {
        "vibefork.exe"
    } else {
        "vibefork"
    });
    std::fs::write(&bin, b"old").unwrap();
    // A file that cannot run fails the self-check: nothing is replaced.
    assert!(install(&bin, b"not a program", "1.0.0").is_err());
    assert_eq!(std::fs::read(&bin).unwrap(), b"old");
    // A real program that does not print `<name> 1.0.0` fails too.
    let git = std::fs::read(which("git")).unwrap();
    assert!(install(&bin, &git, "1.0.0").is_err());
    assert_eq!(std::fs::read(&bin).unwrap(), b"old");
    assert!(!previous(&bin).exists());
    assert!(rollback(&bin).is_err(), "nothing to roll back to");
    let _ = std::fs::remove_dir_all(&dir);
}

fn which(name: &str) -> std::path::PathBuf {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|d| d.join(&exe))
        .find(|p| p.is_file())
        .unwrap()
}
