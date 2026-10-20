//! `vibefork import <owner/name | url>`, `vibefork save <project>`, `vibefork github [status |
//! connect | disconnect]` — the same steps as in the interface, in the terminal.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use vf_platform::agent_process::open_url;
use vf_protocol::Backend;
use vf_protocol::http::{ImportRequest, SaveStep};

/// How long the terminal waits for a step done on github.com.
const STEP_TIMEOUT: Duration = Duration::from_secs(15 * 60);

fn fail(e: impl std::fmt::Display) -> ExitCode {
    eprintln!("vibefork: {e}");
    ExitCode::FAILURE
}

pub(crate) fn import(backend: &dyn Backend, args: &[String]) -> ExitCode {
    let Some(repo) = args.first() else {
        eprintln!("usage: vibefork import <owner/name | https://github.com/owner/name>");
        return ExitCode::from(2);
    };
    match backend.import(&ImportRequest { repo: repo.clone() }) {
        Ok(r) => {
            println!(
                "project {} — from {} at {}",
                r.project,
                r.upstream,
                &r.commit[..7.min(r.commit.len())]
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

/// Signs the agent in to GitHub: shows the code, opens the page, waits.
fn connect(backend: &dyn Backend) -> Result<(), String> {
    let code = backend.github_connect().map_err(|e| e.to_string())?;
    println!(
        "Enter the code {} on {} (opening it now)",
        code.user_code, code.verification_uri
    );
    let _ = open_url(&code.verification_uri);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(code.expires_in) {
        std::thread::sleep(Duration::from_secs(3));
        let s = backend.github_status().map_err(|e| e.to_string())?;
        if s.connected {
            println!(
                "GitHub connected{}",
                s.login.map(|l| format!(" as {l}")).unwrap_or_default()
            );
            return Ok(());
        }
        if let Some(e) = s.error {
            return Err(e);
        }
    }
    Err("the code expired".into())
}

pub(crate) fn github(backend: &dyn Backend, args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None | Some("status") => match backend.github_status() {
            Ok(s) if s.connected => {
                println!(
                    "connected{}",
                    s.login.map(|l| format!(" as {l}")).unwrap_or_default()
                );
                ExitCode::SUCCESS
            }
            Ok(_) => {
                println!("not connected — `vibefork github connect`");
                ExitCode::from(3)
            }
            Err(e) => fail(e),
        },
        Some("connect") => match connect(backend) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(e),
        },
        Some("disconnect") => match vf_platform::secrets::remove("github") {
            Ok(()) => {
                println!(
                    "disconnected (the app stays installed on your copies; remove it in GitHub settings if you want)"
                );
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        Some(_) => {
            eprintln!("usage: vibefork github [status | connect | disconnect]");
            ExitCode::from(2)
        }
    }
}

/// Saves, walking through the steps the user does on github.com.
pub(crate) fn save(backend: &dyn Backend, args: &[String]) -> ExitCode {
    let Some(project) = args.first() else {
        eprintln!("usage: vibefork save <project>");
        return ExitCode::from(2);
    };
    let started = Instant::now();
    let mut last = None;
    while started.elapsed() < STEP_TIMEOUT {
        let r = match backend.save(project) {
            Ok(r) => r,
            Err(e) => return fail(e),
        };
        let first_time = last != Some(r.step);
        last = Some(r.step);
        match r.step {
            SaveStep::Saved | SaveStep::Unchanged => {
                let what = if r.step == SaveStep::Saved {
                    "saved"
                } else {
                    "nothing new to save"
                };
                println!("{what}: {}", r.url.unwrap_or_default());
                return ExitCode::SUCCESS;
            }
            SaveStep::ConnectGithub => {
                if let Err(e) = connect(backend) {
                    return fail(e);
                }
                continue;
            }
            SaveStep::CreateCopy | SaveStep::AllowApp => {
                if first_time {
                    let what = if r.step == SaveStep::CreateCopy {
                        "Create your copy of the project on GitHub (one button: Create fork)"
                    } else {
                        "Allow Vibefork to save into your copy (one button: Install)"
                    };
                    let url = r.url.unwrap_or_default();
                    println!("{what}: {url}");
                    let _ = open_url(&url);
                }
            }
        }
        std::thread::sleep(Duration::from_secs(4));
    }
    eprintln!("vibefork: the step on GitHub was not done in time");
    ExitCode::FAILURE
}
