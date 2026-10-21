//! The `vibefork` command. Without arguments in a terminal it shows the status and a short menu.
//!
//! ```text
//! vibefork serve                      run the agent in this terminal
//! vibefork start | stop | status      the background agent (status: --json)
//! vibefork open [--local]             open the interface (hosted, or the agent's own page)
//! vibefork import <owner/name | url>   take a project from GitHub (no git needed)
//! vibefork save <project> [--to owner/name]   save to your copy on GitHub (walks through the steps)
//! vibefork github [connect | status | disconnect]   the agent's GitHub sign-in
//! vibefork pairs [revoke <key id>]    paired browsers
//! vibefork keys [set|remove <name>]   API keys in the OS secret store
//! vibefork storage [move <kind> <dir>]  heavy folders
//! vibefork doctor [--json]            what works on this machine
//! vibefork update [--check|--rollback|--auto on|off]  signed self-update
//! vibefork install [--autostart]      per-user, no administrator rights
//! vibefork uninstall [--full [--with-projects]]   programs; --full: all agent data; --with-projects: projects too
//! vibefork autostart [on | off | status]   start at login (off by default)
//! vibefork logs [<lines>]             the background agent's log
//! vibefork handle <vibefork://…>      the link handler
//! vibefork version
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)] // the CLI is the one crate that prints

mod agent;
mod doctor;
mod github;
mod keys;
mod menu;
mod setup;
mod storage;
pub mod update;

use std::io::IsTerminal as _;
use std::process::ExitCode;

/// What the commands do, provided by the binary (`vf-agent`) that wires the crates together.
pub trait App {
    /// Runs the agent until it is stopped.
    fn serve(&self) -> ExitCode;
    /// The engine, for commands that do the work themselves (import, save).
    fn backend(&self) -> Box<dyn vf_protocol::Backend>;
}

const USAGE: &str = "usage: vibefork <serve | start | stop | status | open | import | save | github | pairs | keys | storage | doctor | update | install | uninstall | autostart | logs | git-shim | version>";

/// Entry point used by the `vibefork` binary.
pub fn run(args: &[String], app: &dyn App) -> ExitCode {
    let rest = args.get(1..).unwrap_or_default();
    let json = rest.iter().any(|a| a == "--json");
    match args.first().map(String::as_str) {
        None if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() => menu::run(),
        Some("serve") => app.serve(),
        Some("git-shim") => git_shim(rest),
        // Internal: the agent runs sandboxed programs through itself.
        Some(vf_sandbox::EXEC_COMMAND) => {
            let raw: Vec<std::ffi::OsString> = std::env::args_os().skip(2).collect();
            vf_sandbox::exec_from_env(&raw)
        }
        Some("start") => agent::start(),
        Some("stop") => agent::stop(),
        Some("status") => agent::status(json),
        Some("open") => agent::open(rest.iter().any(|a| a == "--local")),
        Some("handle") => match rest.first() {
            Some(url) => agent::handle(url),
            None => usage(),
        },
        Some("logs") => agent::logs(rest.first().and_then(|n| n.parse().ok()).unwrap_or(50)),
        Some("pairs") => setup::pairs(rest),
        Some("import") => github::import(app.backend().as_ref(), rest),
        Some("save") => github::save(app.backend().as_ref(), rest),
        Some("github") => github::github(app.backend().as_ref(), rest),
        Some("keys") => keys::run(rest),
        Some("storage") => storage::run(rest),
        Some("doctor") => doctor::run(json),
        Some("update") => update::run(rest),
        Some("install") => setup::install(rest),
        Some("autostart") => setup::autostart(rest),
        Some("uninstall") => setup::uninstall(rest),
        None | Some("--version" | "-V" | "version") => {
            println!("vibefork {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help" | "-h" | "help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("vibefork: unknown command `{other}` (try `vibefork help`)");
            ExitCode::from(2)
        }
    }
}

fn git_shim(args: &[String]) -> ExitCode {
    let dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("vibefork git: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(repo) = vf_git::Repo::discover(&dir) else {
        eprintln!("fatal: not a git repository (or any of the parent directories)");
        return ExitCode::from(128);
    };
    match repo.shim(args) {
        Ok(vf_git::ShimOutput::Text(text)) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Ok(vf_git::ShimOutput::Recorded) => {
            eprintln!("changes are recorded by Vibefork automatically");
            ExitCode::SUCCESS
        }
        Ok(vf_git::ShimOutput::Unsupported(reason)) => {
            eprintln!("vibefork git: {reason}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("vibefork git: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}
