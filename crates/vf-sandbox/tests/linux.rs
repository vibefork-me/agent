//! A sandboxed process: the project is writable, a file elsewhere in the home is closed.
#![cfg(target_os = "linux")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    missing_docs
)]

use std::path::PathBuf;
use std::process::Command;

use vf_sandbox::{IsolationLevel, SandboxPolicy, available, prepare};

fn run(policy: &SandboxPolicy, script: &str) -> (bool, String) {
    let (wrapper, _) = prepare(
        policy,
        std::path::Path::new(env!("CARGO_BIN_EXE_vf-sandbox-helper")),
    );
    let w = wrapper.unwrap();
    let out = Command::new(&w.program)
        .args(&w.prefix)
        .args(["/bin/sh", "-c", script])
        .envs(w.env.iter().map(|(k, v)| (k, v)))
        .output()
        .unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn canary_outside_the_policy_is_closed() {
    if available() != IsolationLevel::Os {
        eprintln!("skipped: no Landlock in this kernel");
        return;
    }
    let base = std::env::temp_dir().join(format!("vf-sb-{}", std::process::id()));
    let (project, secret) = (base.join("project"), base.join("home-secret"));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::write(secret.join("id_rsa"), "PRIVATE").unwrap();
    let system: Vec<PathBuf> = ["/usr", "/bin", "/lib", "/lib64", "/etc"]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    let policy = SandboxPolicy {
        read: system,
        write: vec![project.clone(), "/dev".into()],
        temp: None,
    };

    let (ok, out) = run(
        &policy,
        &format!("echo hi > {0}/f && cat {0}/f", project.display()),
    );
    assert!(ok && out.contains("hi"), "project is writable: {out}");
    let (ok, out) = run(&policy, &format!("cat {}/id_rsa", secret.display()));
    assert!(
        !ok && !out.contains("PRIVATE"),
        "the canary is closed: {out}"
    );
    let (ok, _) = run(&policy, &format!("echo x > {}/planted", secret.display()));
    assert!(
        !ok && !secret.join("planted").exists(),
        "no writes outside the policy"
    );
    let (ok, _) = run(&policy, "ls / > /dev/null");
    assert!(!ok, "the root folder itself is not listed in the policy");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn canary_package_managers_in_home_are_closed() {
    if available() != IsolationLevel::Os {
        eprintln!("skipped: no Landlock in this kernel");
        return;
    }
    let base = std::env::temp_dir().join(format!("vf-sb-pkg-{}", std::process::id()));
    let (project, home, tools, caches) = (
        base.join("project"),
        base.join("fake-home"),
        base.join("tools"),
        base.join("caches"),
    );
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::create_dir_all(&caches).unwrap();

    let system: Vec<PathBuf> = ["/usr", "/bin", "/lib", "/lib64", "/etc"]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();

    let policy = SandboxPolicy {
        read: system,
        write: vec![project.clone(), tools.clone(), caches.clone(), "/dev".into()],
        temp: None,
    };

    // Attempts to write into package manager paths in home must fail under policy
    for blocked in [
        ".gradle",
        ".m2",
        ".npm",
        ".cache",
        ".local",
        ".bun",
        ".yarn",
        ".pnpm-store",
    ] {
        let (ok, _) = run(
            &policy,
            &format!(
                "mkdir -p \"{0}/{1}\" && touch \"{0}/{1}/leak\"",
                home.display(),
                blocked
            ),
        );
        assert!(!ok, "{blocked} in home must not be writable under policy");
    }

    let _ = std::fs::remove_dir_all(&base);
}
