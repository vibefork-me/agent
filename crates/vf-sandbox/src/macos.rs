//! Seatbelt: the helper executes the program through the system `/usr/bin/sandbox-exec` with a
//! profile built from the policy (no administrator rights); the restriction is inherited by
//! everything the program starts.
//!
//! Filesystem: reading and writing only under the policy's paths; anywhere else only file
//! metadata (resolving paths) and the list of the root folder, which `dyld` reads along with
//! mapping executable code (both are needed since macOS 26). Processes, signals within the sandbox, Mach and IPC services and the
//! network are allowed. The policy's paths reach the profile as `-D` parameters, never as text of
//! the profile.

use std::ffi::OsString;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use vf_core::sandbox::SandboxPolicy;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Whether Seatbelt works here: a trivial profile runs `/usr/bin/true` (it does not inside
/// another Seatbelt sandbox).
pub(crate) fn available() -> bool {
    Path::new(SANDBOX_EXEC).exists()
        && Command::new(SANDBOX_EXEC)
            .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

/// Executes `program` inside the sandbox; returns the error when it cannot.
pub(crate) fn exec(policy: &SandboxPolicy, program: &OsString, args: &[OsString]) -> String {
    let (profile, params) = profile(policy);
    let mut cmd = Command::new(SANDBOX_EXEC);
    for (k, v) in &params {
        let mut d = OsString::from(format!("{k}="));
        d.push(v);
        cmd.arg("-D").arg(d);
    }
    let err = cmd
        .arg("-p")
        .arg(profile)
        .arg(program)
        .args(args)
        .env_remove(crate::POLICY_VAR)
        .exec();
    format!("cannot start {}: {err}", program.to_string_lossy())
}

/// The profile and its parameters (`R<n>` — readable, `W<n>` — writable).
pub(crate) fn profile(policy: &SandboxPolicy) -> (String, Vec<(String, PathBuf)>) {
    let mut p = String::from(
        "(version 1)\n\
         (deny default)\n\
         (allow process-fork process-exec process-info*)\n\
         (allow signal (target same-sandbox))\n\
         (allow sysctl-read)\n\
         (allow mach-lookup mach-register ipc-posix* ipc-sysv*)\n\
         (allow network*)\n\
         (allow iokit-open)\n\
         (allow file-read-metadata)\n\
         (allow file-map-executable)\n\
         (allow file-read-data (literal \"/\"))\n",
    );
    let mut params = Vec::new();
    for (i, path) in real(&policy.read).into_iter().enumerate() {
        p.push_str(&format!("(allow file-read* (subpath (param \"R{i}\")))\n"));
        params.push((format!("R{i}"), path));
    }
    for (i, path) in real(&policy.write).into_iter().enumerate() {
        p.push_str(&format!(
            "(allow file-read* file-write* file-ioctl (subpath (param \"W{i}\")))\n"
        ));
        params.push((format!("W{i}"), path));
    }
    (p, params)
}

/// Seatbelt matches resolved paths (`/tmp` is `/private/tmp`); a missing path is left out. Links
/// themselves are followed through the metadata rule.
fn real(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if let Ok(r) = p.canonicalize()
            && !out.contains(&r)
        {
            out.push(r);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_parameters_not_profile_text() {
        let policy = SandboxPolicy {
            read: vec!["/usr".into()],
            write: vec!["/tmp".into(), "/does/not/exist".into()],
            temp: None,
        };
        let (profile, params) = profile(&policy);
        assert!(profile.contains("(deny default)"));
        assert!(!profile.contains("/usr") && !profile.contains("/tmp"));
        assert!(
            params
                .iter()
                .any(|(k, v)| k == "R0" && v == Path::new("/usr"))
        );
        assert!(
            params
                .iter()
                .any(|(k, v)| k == "W0" && v == Path::new("/private/tmp")),
            "{params:?}"
        );
        assert_eq!(params.len(), 2, "a missing path is left out");
    }
}
