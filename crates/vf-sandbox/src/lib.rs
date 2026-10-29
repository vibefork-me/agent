//! 🔒 **Security boundary.** Isolation of untrusted execution: the user's coding CLI with
//! everything it runs, the agent's own model loop, builds.
//!
//! The model is not restricted inside: shell, MCP and tools work as usual. What is isolated is the
//! rest of the machine — a sandboxed process sees only the folders of its [`SandboxPolicy`].
//!
//! | OS | Implementation | Level |
//! |---|---|---|
//! | Linux | Landlock (no root): the agent re-runs itself as `vibefork sandbox-exec`, which restricts itself and executes the program | [`IsolationLevel::Os`] when the kernel has Landlock |
//! | Windows | AppContainer (no administrator rights): `vibefork sandbox-exec` grants the policy's folders to the container and starts the program in it. **Off by default** ([`IsolationLevel::None`]); on with `VF_SANDBOX_WINDOWS=appcontainer` | see below |
//! | macOS | Seatbelt (no administrator rights): `vibefork sandbox-exec` executes the program through the system `sandbox-exec` with a profile of the policy | [`IsolationLevel::Os`] when Seatbelt works (not inside another sandbox) |
//!
//! **Known limits of the AppContainer on Windows** (why it is off): Git Bash (MSYS) and PowerShell 7
//! do not start in it, Codex CLI does not start; Node.js fails on projects outside the system drive
//! (an app container cannot read the root of another drive, and granting that needs an
//! administrator). Claude Code itself works.
//!
//! `unsafe` is allowed in this crate only with a `// SAFETY:` justification at each site.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod policy;
#[cfg(windows)]
mod windows;

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

pub use policy::task_policy;
pub use vf_core::sandbox::{IsolationLevel, SandboxPolicy, Wrapper};

/// `VF_SANDBOX_WINDOWS=appcontainer` turns the Windows AppContainer on (experimental).
pub const WINDOWS_OPT_IN: &str = "VF_SANDBOX_WINDOWS";
/// Variable carrying the policy to `sandbox-exec`.
pub const POLICY_VAR: &str = "VF_SANDBOX_POLICY";
/// Command of the agent binary that runs a program inside the sandbox.
pub const EXEC_COMMAND: &str = "sandbox-exec";

/// The isolation this machine provides.
pub fn available() -> IsolationLevel {
    #[cfg(target_os = "linux")]
    let ok = linux::available();
    #[cfg(target_os = "macos")]
    let ok = macos::available();
    // Off by default: see the module documentation.
    #[cfg(windows)]
    let ok =
        std::env::var(WINDOWS_OPT_IN).is_ok_and(|v| v == "appcontainer") && windows::available();
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let ok = false;
    if ok {
        IsolationLevel::Os
    } else {
        IsolationLevel::None
    }
}

/// Whether the user's coding CLIs (Claude Code, Codex, GitHub Copilot) work inside the sandbox. Windows: no —
/// Git Bash (MSYS) and PowerShell 7, the shells they run commands in, do not start in an
/// AppContainer; there they run without isolation, the agent's own loop and builds are isolated.
pub fn supports_coding_clis() -> bool {
    !cfg!(windows)
}

/// Removes what the sandbox created on this machine (the Windows AppContainer profile).
pub fn remove() {
    #[cfg(windows)]
    windows::remove();
}

/// How to start sandboxed commands with `policy`: a wrapper around them (none when the machine has
/// no sandbox) and the level they get. `helper` is the agent binary (it has [`EXEC_COMMAND`]).
pub fn prepare(policy: &SandboxPolicy, helper: &Path) -> (Option<Wrapper>, IsolationLevel) {
    let level = available();
    if level == IsolationLevel::None {
        return (None, level);
    }
    match wrapper(policy, helper) {
        Some(w) => (Some(w), level),
        None => (None, IsolationLevel::None),
    }
}

/// The wrapper for `policy`, whether or not the machine provides isolation (tests).
pub fn wrapper(policy: &SandboxPolicy, helper: &Path) -> Option<Wrapper> {
    let json = serde_json::to_string(policy).ok()?;
    let mut env: Vec<(OsString, OsString)> = vec![(POLICY_VAR.into(), json.into())];
    if let Some(tmp) = &policy.temp {
        let _ = std::fs::create_dir_all(tmp);
        for k in ["TEMP", "TMP"] {
            env.push((k.into(), tmp.clone().into_os_string()));
        }
    }
    Some(Wrapper {
        program: helper.to_path_buf(),
        prefix: vec![EXEC_COMMAND.into(), "--".into()],
        env,
    })
}

/// `sandbox-exec -- <program> <args…>`: restricts this process with the policy from
/// [`POLICY_VAR`] and executes the program. Returns only on failure.
pub fn exec_from_env(args: &[OsString]) -> ExitCode {
    let args = match args.first() {
        Some(a) if a == "--" => &args[1..],
        _ => args,
    };
    let Some((program, rest)) = args.split_first() else {
        return fail("usage: sandbox-exec -- <program> [args…]");
    };
    let policy: SandboxPolicy = match std::env::var(POLICY_VAR)
        .ok()
        .map(|j| serde_json::from_str(&j))
    {
        Some(Ok(p)) => p,
        _ => return fail("no sandbox policy"),
    };
    #[cfg(target_os = "linux")]
    return fail(&linux::exec(&policy, program, rest));
    #[cfg(target_os = "macos")]
    return fail(&macos::exec(&policy, program, rest));
    // Windows: the program's command line is taken as is from this process's own.
    #[cfg(windows)]
    {
        let _ = (program, rest);
        fail(&windows::exec(&policy))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (policy, program, rest);
        fail("no sandbox on this OS")
    }
}

#[allow(clippy::print_stderr)] // the helper reports its own failure on stderr
fn fail(message: &str) -> ExitCode {
    eprintln!("vibefork sandbox: {message}");
    ExitCode::from(126)
}
