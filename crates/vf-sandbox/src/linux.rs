//! Landlock: the process restricts itself (no root needed), then executes the program; the
//! restriction is inherited by everything the program starts.
//!
//! Filesystem: only the policy's paths. Scopes (newer kernels): no signals to processes outside the
//! sandbox and no abstract Unix sockets outside it. Network is not restricted.

use std::ffi::OsString;
use std::os::unix::process::CommandExt as _;
use std::process::Command;

use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, Ruleset, RulesetAttr, RulesetCreatedAttr,
    Scope, path_beneath_rules,
};
use vf_core::sandbox::SandboxPolicy;

/// Highest Landlock ABI this code knows.
const ABI_VERSION: ABI = ABI::V6;

/// Whether the kernel has Landlock enabled: a ruleset can be created (it is not applied).
/// `/sys/kernel/security/lsm` is not used — securityfs is not mounted everywhere (e.g. WSL).
pub(crate) fn available() -> bool {
    Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(ABI::V1))
        .and_then(|r| r.create())
        .is_ok()
}

/// Restricts this process and executes `program`; returns the error when it cannot.
pub(crate) fn exec(policy: &SandboxPolicy, program: &OsString, args: &[OsString]) -> String {
    if let Err(e) = restrict(policy) {
        return format!("cannot set up Landlock: {e}");
    }
    let err = Command::new(program)
        .args(args)
        .env_remove(crate::POLICY_VAR)
        .exec();
    format!("cannot start {}: {err}", program.to_string_lossy())
}

fn restrict(policy: &SandboxPolicy) -> Result<(), landlock::RulesetError> {
    let read = AccessFs::from_read(ABI_VERSION);
    let all = AccessFs::from_all(ABI_VERSION);
    let status = Ruleset::default()
        // Best effort on older kernels: whatever the kernel supports is enforced.
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(all)?
        .scope(Scope::from_all(ABI_VERSION))?
        .create()?
        .add_rules(path_beneath_rules(&policy.read, read))?
        .add_rules(path_beneath_rules(&policy.write, all))?
        .restrict_self()?;
    let _ = status;
    Ok(())
}
