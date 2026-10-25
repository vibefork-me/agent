//! `vibefork import <owner/name | url>`, `vibefork save <project>`, `vibefork github [status |
//! connect | disconnect]` — the same steps as in the interface, in the terminal.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use vf_platform::agent_process::open_url;
use vf_protocol::Backend;
use vf_protocol::http::{ImportRequest, SaveRequest, SaveStep};

/// How long the terminal waits for a step done on github.com.
const STEP_TIMEOUT: Duration = Duration::from_secs(15 * 60);

fn fail(e: impl std::fmt::Display) -> ExitCode {
    eprintln!("vibefork: {e}");
    ExitCode::FAILURE
}

pub(crate) fn import(backend: &dyn Backend, args: &[String]) -> ExitCode {
    let Some(repo) = args.first() else {
        eprintln!(
            "usage: vibefork import <owner/name | https://github.com/owner/name> [ref] [original]"
        );
        return ExitCode::from(2);
    };
    match backend.import(&ImportRequest {
        repo: repo.clone(),
        git_ref: args.get(1).cloned(),
        upstream: args.get(2).cloned(),
    }) {
        Ok(r) => {
            println!(
                "project {} — from {} at {}",
                r.project,
                r.upstream,
                &r.commit[..7.min(r.commit.len())]
            );
            if let Some(f) = &r.foreign {
                println!(
                    "code not from the original: {} — {} changed files",
                    f.source, f.files
                );
                for (kind, list) in [
                    ("dependencies", &f.dependencies),
                    ("build scripts", &f.build_scripts),
                    ("binary files", &f.binaries),
                ] {
                    if !list.is_empty() {
                        println!("  changes {kind}: {}", list.join(", "));
                    }
                }
                if !f.complete {
                    println!(
                        "  not every changed file was checked — see {}",
                        f.compare_url
                    );
                }
            }
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

/// Saves, walking through the steps the user does on github.com. `--to owner/name` — the
/// repository to save into.
pub(crate) fn save(backend: &dyn Backend, args: &[String]) -> ExitCode {
    let (Some(project), copy) = (args.first(), args.iter().position(|a| a == "--to")) else {
        eprintln!("usage: vibefork save <project> [--to owner/name]");
        return ExitCode::from(2);
    };
    let req = SaveRequest {
        project: project.clone(),
        copy: copy.and_then(|i| args.get(i + 1)).cloned(),
    };
    let started = Instant::now();
    let mut last = None;
    while started.elapsed() < STEP_TIMEOUT {
        let r = match backend.save(&req) {
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
            SaveStep::Conflict => {
                eprintln!(
                    "vibefork: nothing saved — {} has moved on, and these files were changed both there and here:",
                    r.copy.unwrap_or_default()
                );
                for f in &r.files {
                    eprintln!("  {f}");
                }
                return ExitCode::FAILURE;
            }
            SaveStep::CreateCopy
            | SaveStep::CreateRepo
            | SaveStep::AllowApp
            | SaveStep::NarrowAccess => {
                if first_time {
                    let name = r.name.as_deref().unwrap_or_default();
                    let what = match r.step {
                        SaveStep::CreateCopy if name.is_empty() => {
                            "Create your copy of the project on GitHub (one button: Create fork)".to_owned()
                        }
                        SaveStep::CreateCopy => format!(
                            "Create your copy of the project on GitHub: name it {name} (your repository with the usual name is something else), then Create fork"
                        ),
                        SaveStep::CreateRepo => format!(
                            "Create an empty private repository {name} on GitHub (one button: Create repository)"
                        ),
                        SaveStep::NarrowAccess => {
                            "Vibefork reaches all your repositories: choose \"Only select repositories\" and leave only your copy, then Save".to_owned()
                        }
                        _ => "Allow Vibefork to save into your copy (one button: Install)".to_owned(),
                    };
                    let url = r.url.unwrap_or_default();
                    println!("{what}: {url}");
                    let _ = open_url(&url);
                    if let Some(next) = r.next_url {
                        println!("Press Enter when it is created.");
                        let mut line = String::new();
                        let _ = std::io::stdin().read_line(&mut line);
                        println!(
                            "Allow Vibefork on it — choose \"Only select repositories\" and pick it, then Install: {next}"
                        );
                        let _ = open_url(&next);
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_secs(4));
    }
    eprintln!("vibefork: the step on GitHub was not done in time");
    ExitCode::FAILURE
}
