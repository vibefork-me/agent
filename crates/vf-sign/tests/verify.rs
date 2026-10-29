//! `vibefork-sign verify` on a release directory signed with throwaway keys.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn sign(args: &[&Path]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vibefork-sign"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(args: &[&Path]) {
    let out = sign(args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A fresh directory with a root key, a release subkey certified by it, and a signed release.
fn signed_release(name: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("vf-sign-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let release = base.join("release");
    std::fs::create_dir_all(&release).unwrap();
    let p = |n: &str| base.join(n);
    ok(&[Path::new("keygen"), &p("root.key"), &p("root.pub")]);
    ok(&[Path::new("keygen"), &p("sub.key"), &p("sub.pub")]);
    ok(&[
        Path::new("certify-subkey"),
        &p("root.key"),
        &p("sub.pub"),
        Path::new("release"),
        Path::new("30"),
    ]);
    std::fs::copy(p("sub.pub.cert.json"), release.join("subkey.cert.json")).unwrap();
    std::fs::copy(p("sub.pub.cert.sig"), release.join("subkey.cert.sig")).unwrap();
    std::fs::write(release.join("vibefork-linux-x86_64"), b"binary").unwrap();
    std::fs::write(release.join("SHA256SUMS.txt"), b"sums").unwrap();
    ok(&[
        Path::new("release"),
        &release,
        &p("sub.key"),
        Path::new("1.2.3-rc.1"),
    ]);
    (release, p("root.pub"))
}

fn verify(release: &Path, root: &Path) -> std::process::Output {
    sign(&[Path::new("verify"), release, Path::new("--root"), root])
}

#[test]
fn accepts_a_release_signed_with_a_certified_subkey() {
    let (release, root) = signed_release("ok");
    let out = verify(&release, &root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "release 1.2.3-rc.1: signature and 4 files verified"
    );
}

#[test]
fn rejects_a_changed_file() {
    let (release, root) = signed_release("changed");
    std::fs::write(release.join("vibefork-linux-x86_64"), b"other").unwrap();
    assert!(!verify(&release, &root).status.success());
}

#[test]
fn rejects_an_unlisted_file() {
    let (release, root) = signed_release("unlisted");
    std::fs::write(release.join("extra.txt"), b"x").unwrap();
    let out = verify(&release, &root);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("extra.txt is not in the signed manifest")
    );
}

#[test]
fn rejects_a_release_without_the_subkey_certificate() {
    let (release, root) = signed_release("nocert");
    std::fs::remove_file(release.join("subkey.cert.sig")).unwrap();
    assert!(!verify(&release, &root).status.success());
}

#[test]
fn rejects_another_root_key() {
    let (release, _) = signed_release("root");
    // The built-in release key did not certify the throwaway subkey.
    let out = sign(&[Path::new("verify"), &release]);
    assert!(!out.status.success());
}
