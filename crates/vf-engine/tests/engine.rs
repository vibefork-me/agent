//! The engine with a scripted provider on a real git repository: journal, checkpoints, resume and
//! handoff, availability and busy checks.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    missing_docs
)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use vf_engine::Engine;
use vf_platform::Settings;
use vf_platform::settings::Storage;
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, Cancel, CostClass, Event, ModelInfo, TaskRequest};
use vf_providers::{Answer, AskRequest, Output, Provider, RunRequest, RunResult};

/// Writes `n.txt`, commits it itself, writes `left.txt` uncommitted; records the prompts it got.
struct Scripted {
    id: &'static str,
    prompts: Mutex<Vec<(String, Option<String>)>>,
}

impl Provider for Scripted {
    fn id(&self) -> &'static str {
        self.id
    }
    fn models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            provider: self.id.into(),
            model: "m1".into(),
            tier: 1,
            cost: CostClass::Free,
            label: "m1".into(),
        }]
    }
    fn can_resume(&self) -> bool {
        true
    }
    fn ask(&self, req: &AskRequest<'_>, _: &Cancel) -> Result<Answer, String> {
        let text = if req.system.is_some() {
            r#"{"complexity":"medium","decompose":true,"reason":"two parts"}"#
        } else {
            r#"{"steps":[{"title":"first","prompt":"do first","complexity":"simple"},{"title":"second","prompt":"do second","complexity":"medium"}]}"#
        };
        Ok(Answer {
            text: text.into(),
            cost_usd: Some(0.01),
        })
    }

    fn run(&self, req: &RunRequest<'_>, out: &mut dyn FnMut(Output), _: &Cancel) -> RunResult {
        let mut prompts = self.prompts.lock().unwrap();
        prompts.push((req.prompt.to_owned(), req.resume.map(str::to_owned)));
        let n = prompts.len();
        std::fs::write(req.dir.join(format!("{n}.txt")), "x").unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(req.dir)
                .args(["-c", "user.name=m", "-c", "user.email=m@m"])
                .args(args)
                .output()
                .unwrap()
        };
        git(&["add", "-A"]);
        git(&["commit", "-qm", "model commit"]);
        std::fs::write(req.dir.join("left.txt"), format!("{n}")).unwrap();
        out(Output::Event(Event::Init {
            session: Some(format!("s{n}")),
            model: None,
            cli: "scripted".into(),
            mcp: None,
            skills: None,
        }));
        out(Output::Event(Event::Text {
            text: format!("did {n}"),
        }));
        RunResult {
            code: 0,
            ok: true,
            session: Some(format!("s{n}")),
            summary: format!("did {n}"),
            ..RunResult::default()
        }
    }
}

fn setup() -> (PathBuf, Engine, Arc<Scripted>) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("vf-engine-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(base.join("projects/demo")).unwrap();
    std::fs::write(base.join("projects/demo/README.md"), "user file").unwrap();
    let settings = Settings {
        state_dir: base.join("state"),
        port: 1,
        extra_origins: vec![],
        ui_bundle_url: String::new(),
        storage: Storage {
            projects: base.join("projects"),
            tools: base.join("tools"),
            caches: base.join("caches"),
            artifacts: base.join("artifacts"),
        },
        simulate_clean_machine: true,
        update_url: String::new(),
        auto_update: false,
    };
    let p = Arc::new(Scripted {
        id: "codex",
        prompts: Mutex::new(vec![]),
    });
    let q: Arc<dyn Provider> = Arc::new(Scripted {
        id: "claude",
        prompts: Mutex::new(vec![]),
    });
    let engine = Engine::new(&settings, vec![p.clone(), q]);
    (base, engine, p)
}

fn task(provider: &str, continue_: bool) -> TaskRequest {
    TaskRequest {
        project: "demo".into(),
        prompt: "do it".into(),
        provider: Some(provider.into()),
        model: Some("m1".into()),
        continue_,
    }
}

fn run(engine: &Engine, t: TaskRequest) -> Vec<ServerMessage> {
    let mut msgs = vec![];
    engine.run_task(t, &mut |m| msgs.push(m), &Cancel::new());
    msgs
}

#[test]
fn task_is_journaled_with_all_commits_and_continued() {
    let (base, engine, p) = setup();
    let msgs = run(&engine, task("codex", false));
    assert!(matches!(
        &msgs[0],
        ServerMessage::Started {
            resume: false,
            handoff: false,
            ..
        }
    ));
    let Some(ServerMessage::Done {
        code: 0,
        commit: Some(c),
        diff,
        status,
        session_id,
        ..
    }) = msgs.last()
    else {
        panic!("{msgs:?}")
    };
    assert!(
        diff.contains("1.txt") && diff.contains("left.txt") && !diff.contains("README.md"),
        "user files are the base"
    );
    assert!(status.contains("2 files changed"), "{status}");
    assert_eq!(session_id.as_deref(), Some("s1"));

    let j = engine.journal("demo").unwrap().entries;
    assert_eq!(j.len(), 1);
    assert_eq!(j[0].commits.len(), 2, "model commit + checkpoint");
    assert_eq!(j[0].commit.as_ref(), Some(c));
    assert_eq!(j[0].summary, "did 1");

    // Same provider + model: native resume.
    let msgs = run(&engine, task("codex", true));
    assert!(
        matches!(
            &msgs[0],
            ServerMessage::Started {
                resume: true,
                handoff: false,
                ..
            }
        ),
        "{msgs:?}"
    );
    assert_eq!(p.prompts.lock().unwrap()[1].1.as_deref(), Some("s1"));

    // Another provider: handoff preamble with the history.
    let msgs = run(&engine, task("claude", true));
    assert!(
        matches!(
            &msgs[0],
            ServerMessage::Started {
                resume: false,
                handoff: true,
                ..
            }
        ),
        "{msgs:?}"
    );
    assert_eq!(engine.journal("demo").unwrap().entries.len(), 3);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn bad_requests_are_refused_before_running() {
    let (base, engine, p) = setup();
    for (t, error) in [
        (
            TaskRequest {
                project: "nope".into(),
                ..task("codex", false)
            },
            "unknown project",
        ),
        (
            TaskRequest {
                project: "..".into(),
                ..task("codex", false)
            },
            "unknown project",
        ),
        (
            TaskRequest {
                prompt: " ".into(),
                ..task("codex", false)
            },
            "prompt required",
        ),
        (
            TaskRequest {
                prompt: "x".repeat(8001),
                ..task("codex", false)
            },
            "prompt required",
        ),
        (
            TaskRequest {
                model: Some("other".into()),
                ..task("codex", false)
            },
            "model not available",
        ),
        (task("gemini", false), "model not available"),
    ] {
        let msgs = run(&engine, t);
        assert!(
            matches!(&msgs[..], [ServerMessage::Error { error: e }] if e.starts_with(error)),
            "{msgs:?}"
        );
    }
    assert!(p.prompts.lock().unwrap().is_empty());
    assert_eq!(engine.models().unwrap().models.len(), 2);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn plan_runs_every_step_on_the_cheapest_model() {
    let (base, engine, p) = setup();
    let msgs = run(&engine, task("plan", false));
    let kinds: Vec<&str> = msgs
        .iter()
        .map(|m| match m {
            ServerMessage::Planning { .. } => "planning",
            ServerMessage::Plan { .. } => "plan",
            ServerMessage::Started { .. } => "started",
            ServerMessage::StepDone { ok: true, .. } => "step-done",
            ServerMessage::Done {
                code: 0,
                plan: true,
                ..
            } => "done",
            _ => "",
        })
        .filter(|k| !k.is_empty())
        .collect();
    assert_eq!(
        kinds,
        [
            "planning",
            "plan",
            "started",
            "step-done",
            "started",
            "step-done",
            "done"
        ],
        "{msgs:?}"
    );
    let Some(ServerMessage::Plan { planner, steps, .. }) = msgs
        .iter()
        .find(|m| matches!(m, ServerMessage::Plan { .. }))
    else {
        panic!()
    };
    assert_eq!(planner, "codex/default", "the first planner that answers");
    assert_eq!(steps.len(), 2);
    let Some(ServerMessage::Done { diff, .. }) = msgs.last() else {
        panic!()
    };
    assert!(diff.contains("1.txt"), "diff of the whole plan");
    let j = engine.journal("demo").unwrap().entries;
    assert_eq!(
        j.iter().map(|e| e.provider.as_str()).collect::<Vec<_>>()[0],
        "planner"
    );
    assert_eq!(j.len(), 3);
    assert!(j[2].prompt.starts_with("[step 2/2]"));
    // Step 2 continues step 1: the second prompt has the plan preamble.
    assert!(
        p.prompts
            .lock()
            .unwrap()
            .iter()
            .all(|(prompt, _)| prompt.contains("Overall task"))
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn auto_without_classifier_uses_the_heuristic_and_routes() {
    let (base, engine, _) = setup();
    let msgs = run(&engine, task("auto", false));
    let Some(ServerMessage::Routing { rating, choice }) = msgs.first() else {
        panic!("{msgs:?}")
    };
    assert_eq!(
        (rating.classifier.as_str(), rating.complexity.as_str()),
        ("heuristic", "simple")
    );
    assert_eq!(choice.provider, "claude", "subscription CLIs first");
    assert!(matches!(
        msgs.last(),
        Some(ServerMessage::Done {
            code: 0,
            plan: false,
            ..
        })
    ));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn rollback_only_to_journal_checkpoints() {
    let (base, engine, _) = setup();
    run(&engine, task("codex", false));
    run(&engine, task("codex", false));
    let j = engine.journal("demo").unwrap().entries;
    let first = j[0].commit.clone().unwrap();
    assert!(base.join("projects/demo/2.txt").exists());
    let r = engine.rollback("demo", &first).unwrap();
    assert!(r.commit.starts_with(&first));
    assert!(
        !base.join("projects/demo/2.txt").exists() && base.join("projects/demo/1.txt").exists()
    );
    assert_eq!(
        engine
            .journal("demo")
            .unwrap()
            .entries
            .last()
            .map(|e| e.provider.as_str()),
        Some("rollback")
    );
    assert!(
        engine.rollback("demo", "0000000").is_err(),
        "unknown commit"
    );
    assert!(engine.rollback("demo", "HEAD~1").is_err(), "only hashes");
    assert!(engine.rollback("../x", &first).is_err());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn build_runs_the_recipe_and_keeps_artifacts() {
    if vf_providers::process::find_program("npm").is_none() {
        eprintln!("skipped: npm is not installed");
        return;
    }
    let (base, engine, _) = setup();
    let script = "node -e \"require('fs').mkdirSync('dist',{recursive:true});require('fs').writeFileSync('dist/out.txt','x')\"";
    std::fs::write(
        base.join("projects/demo/package.json"),
        serde_json::json!({ "scripts": { "build": script } }).to_string(),
    )
    .unwrap();
    let mut msgs = vec![];
    engine.build("demo", &mut |m| msgs.push(m), &Cancel::new());
    assert!(
        matches!(&msgs[0], ServerMessage::Estimate { .. }),
        "{msgs:?}"
    );
    let Some(ServerMessage::BuildDone {
        code: 0,
        artifacts: Some(dir),
        files,
        ..
    }) = msgs.last()
    else {
        panic!("{msgs:?}")
    };
    assert_eq!(files, &["dist/out.txt"]);
    assert!(std::path::Path::new(dir).join("dist/out.txt").exists());

    std::fs::remove_file(base.join("projects/demo/package.json")).unwrap();
    let mut msgs = vec![];
    engine.build("demo", &mut |m| msgs.push(m), &Cancel::new());
    assert!(
        matches!(&msgs[..], [ServerMessage::Error { error }] if error.contains("no known way"))
    );
    let _ = std::fs::remove_dir_all(&base);
}
