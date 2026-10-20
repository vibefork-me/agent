//! A sandboxed process (AppContainer): the project is writable, a file elsewhere in the user's
//! folders is closed, the exit code and output come through.
#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::os::windows::process::CommandExt as _;
use std::process::Command;

use vf_sandbox::{SandboxPolicy, wrapper};

fn run(policy: &SandboxPolicy, script: &str) -> (Option<i32>, String) {
    let w = wrapper(
        policy,
        std::path::Path::new(env!("CARGO_BIN_EXE_vf-sandbox-helper")),
    )
    .unwrap();
    let out = Command::new(&w.program)
        .args(&w.prefix)
        .arg("cmd.exe")
        .args(["/d", "/s", "/c"])
        .raw_arg(format!("\"{script}\""))
        .envs(w.env.iter().map(|(k, v)| (k, v)))
        .output()
        .unwrap();
    (
        out.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn canary_outside_the_policy_is_closed() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("vf-sb-{}-{nanos}", std::process::id()));
    let (project, secret) = (base.join("project"), base.join("home-secret"));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::write(secret.join("id_rsa"), "PRIVATE").unwrap();
    let policy = SandboxPolicy {
        read: vec![],
        write: vec![project.clone()],
        temp: None,
    };

    let (code, out) = run(
        &policy,
        &format!("echo hi> \"{0}\\f\" && type \"{0}\\f\"", project.display()),
    );
    assert_eq!(code, Some(0), "project is writable: {out}");
    assert!(out.contains("hi"), "{out}");
    let (code, out) = run(&policy, &format!("type \"{}\\id_rsa\"", secret.display()));
    assert_ne!(code, Some(0), "the canary is closed: {out}");
    assert!(!out.contains("PRIVATE"), "{out}");
    let (code, _) = run(
        &policy,
        &format!("echo x> \"{}\\planted\"", secret.display()),
    );
    assert_ne!(code, Some(0));
    assert!(
        !secret.join("planted").exists(),
        "no writes outside the policy"
    );
    let (code, _) = run(&policy, "exit 7");
    assert_eq!(code, Some(7), "the exit code comes through");
    let _ = std::fs::remove_dir_all(&base);
}
