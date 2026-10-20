//! Test helper: `vf-sandbox-helper sandbox-exec -- <program> [args…]`, like the agent binary.

use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match args.split_first() {
        Some((cmd, rest)) if cmd == vf_sandbox::EXEC_COMMAND => vf_sandbox::exec_from_env(rest),
        _ => ExitCode::from(2),
    }
}
