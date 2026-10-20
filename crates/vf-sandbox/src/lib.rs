//! 🔒 **Security boundary.** Isolation of untrusted execution: the user's coding CLI with
//! everything it runs, the agent's own model loop, builds.
//!
//! The model is not restricted inside: shell, MCP and tools work as usual. What is isolated is the
//! rest of the machine — a sandboxed process sees only the folders of its [`SandboxPolicy`].
//!
//! | OS | Implementation | Level |
//! |---|---|---|
//! | Linux | Landlock (no root): the agent re-runs itself as `vibefork sandbox-exec`, which restricts itself and executes the program | [`IsolationLevel::Os`] when the kernel has Landlock |
//! | macOS, Windows | not yet | [`IsolationLevel::None`] |
//!
//! `unsafe` is allowed in this crate only with a `// SAFETY:` justification at each site.

#[cfg(target_os = "linux")]
mod linux;
mod policy;

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

pub use policy::task_policy;
pub use vf_core::sandbox::{IsolationLevel, SandboxPolicy, Wrapper};

/// Variable carrying the policy to `sandbox-exec`.
pub const POLICY_VAR: &str = "VF_SANDBOX_POLICY";
/// Command of the agent binary that runs a program inside the sandbox.
pub const EXEC_COMMAND: &str = "sandbox-exec";

/// The isolation this machine provides.
pub fn available() -> IsolationLevel {
    #[cfg(target_os = "linux")]
    return if linux::available() {
        IsolationLevel::Os
    } else {
        IsolationLevel::None
    };
    #[cfg(not(target_os = "linux"))]
    IsolationLevel::None
}

/// How to start sandboxed commands with `policy`: a wrapper around them (none when the machine has
/// no sandbox) and the level they get. `helper` is the agent binary (it has [`EXEC_COMMAND`]).
pub fn prepare(policy: &SandboxPolicy, helper: &Path) -> (Option<Wrapper>, IsolationLevel) {
    let level = available();
    if level == IsolationLevel::None {
        return (None, level);
    }
    let Ok(json) = serde_json::to_string(policy) else {
        return (None, IsolationLevel::None);
    };
    let wrapper = Wrapper {
        program: helper.to_path_buf(),
        prefix: vec![EXEC_COMMAND.into(), "--".into()],
        env: vec![(POLICY_VAR.into(), json.into())],
    };
    (Some(wrapper), level)
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
    #[cfg(not(target_os = "linux"))]
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
