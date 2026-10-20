//! A sandboxed process: the project is writable, a file elsewhere in the home is closed.
#![cfg(target_os = "macos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    missing_docs
)]

use std::path::PathBuf;
use std::process::Command;

use vf_sandbox::{IsolationLevel, SandboxPolicy, available, prepare, task_policy};

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

fn base(tag: &str) -> PathBuf {
    // In the home, like the user's files (the temp folder is open to tasks).
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    home.join(format!("vf-sb-{tag}-{}", std::process::id()))
}

#[test]
fn canary_outside_the_policy_is_closed() {
    if available() != IsolationLevel::Os {
        eprintln!("skipped: no Seatbelt here");
        return;
    }
    let base = base("canary");
    let (project, secret) = (base.join("project"), base.join("home-secret"));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::write(secret.join("id_rsa"), "PRIVATE").unwrap();
    let system: Vec<PathBuf> = ["/usr", "/bin", "/System", "/Library", "/private/var/select"]
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
    let (ok, _) = run(&policy, &format!("ls {} > /dev/null", secret.display()));
    assert!(!ok, "a folder outside the policy is not listed");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn task_policy_runs_tools_and_keeps_the_home_closed() {
    if available() != IsolationLevel::Os {
        eprintln!("skipped: no Seatbelt here");
        return;
    }
    let base = base("task");
    let (project, tools, caches) = (base.join("p"), base.join("t"), base.join("c"));
    for d in [&project, &tools, &caches] {
        std::fs::create_dir_all(d).unwrap();
    }
    let secret = base.join("secret.txt");
    std::fs::write(&secret, "PRIVATE").unwrap();
    let policy = task_policy(&project, &tools, &caches);
    let script = format!(
        "cd {} && /usr/bin/env git --version >/dev/null 2>&1; uname -a > uname.txt && mktemp >/dev/null && cat uname.txt",
        project.display()
    );
    let (ok, out) = run(&policy, &script);
    assert!(
        ok && out.contains("Darwin"),
        "tools work in the project: {out}"
    );
    let (ok, out) = run(&policy, &format!("cat {}", secret.display()));
    assert!(!ok && !out.contains("PRIVATE"), "home is closed: {out}");
    let _ = std::fs::remove_dir_all(&base);
}
