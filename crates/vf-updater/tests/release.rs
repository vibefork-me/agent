//! Self-update from a signed release channel served by a local HTTP server.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;

use axum::Router;
use axum::routing::get;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use vf_updater::Manifest;
use vf_updater::release::{Release, asset_name, install, previous, rollback};
use vf_updater::signing::{sha256_hex, sign};

async fn channel(key: &SigningKey, version: &str, binary: Vec<u8>, served: Vec<u8>) -> String {
    let asset = asset_name("vibefork");
    let manifest = Manifest {
        version: version.into(),
        files: BTreeMap::from([(asset.clone(), sha256_hex(&binary))]),
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

#[test]
fn install_checks_the_new_binary_and_keeps_the_old_one() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("vf-install-{}-{nanos}", std::process::id()));
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
