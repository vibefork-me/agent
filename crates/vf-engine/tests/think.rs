//! While a task waits for a model the stream says what the agent is thinking about: before the
//! rating, before the plan and before every step, in the person's language.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vf_engine::Engine;
use vf_platform::Settings;
use vf_platform::settings::Storage;
use vf_protocol::canvas::CanvasEvent;
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, Cancel, CostClass, ModelInfo, TaskRequest};
use vf_providers::{Answer, AskRequest, Output, Provider, RunRequest, RunResult};

/// Answers every question with a two-step plan (not a rating) and makes every run succeed.
struct Fake(&'static str);

impl Provider for Fake {
    fn id(&self) -> &'static str {
        self.0
    }
    fn models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            provider: self.0.into(),
            model: "m1".into(),
            tier: 1,
            cost: CostClass::Free,
            label: "m1".into(),
        }]
    }
    fn can_resume(&self) -> bool {
        false
    }
    fn ask(&self, _: &AskRequest<'_>, _: &Cancel) -> Result<Answer, String> {
        Ok(Answer {
            text: r#"{"steps":[{"title":"tags","prompt":"add tags","complexity":"simple"},{"title":"colors","prompt":"color the tags","complexity":"simple"}]}"#.into(),
            cost_usd: None,
        })
    }
    fn run(&self, req: &RunRequest<'_>, _: &mut dyn FnMut(Output), _: &Cancel) -> RunResult {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        std::fs::write(req.dir.join(format!("step{n}.txt")), "x").unwrap();
        RunResult {
            code: 0,
            ok: true,
            summary: "done".into(),
            ..RunResult::default()
        }
    }
}

fn engine(provider: &'static str) -> Engine {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let base = std::env::temp_dir().join(format!(
        "vf-think-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("projects/notes-app")).unwrap();
    std::fs::write(base.join("projects/notes-app/README.md"), "notes").unwrap();
    let settings = Settings {
        state_dir: base.join("state"),
        port: 1,
        extra_origins: vec![],
        ignored_extra_origins: vec![],
        ui_bundle_url: String::new(),
        storage: Storage {
            projects: base.join("projects"),
            tools: PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("vf-engine-tools"),
            caches: base.join("caches"),
            artifacts: base.join("artifacts"),
        },
        simulate_clean_machine: true,
        update_url: String::new(),
        auto_update: false,
    };
    Engine::new(&settings, vec![Arc::new(Fake(provider))])
}

/// Runs a task; returns the canvas events (with their numbers) and the other messages' types.
fn run(engine: &Engine, provider: &str, lang: &str) -> (Vec<(u64, CanvasEvent)>, Vec<String>) {
    let task = TaskRequest {
        project: "notes-app".into(),
        prompt: "add tags to notes".into(),
        provider: Some(provider.into()),
        model: Some("m1".into()),
        continue_: false,
        lang: Some(lang.into()),
    };
    let mut events = Vec::new();
    let mut kinds = Vec::new();
    engine.run_task(
        task,
        &mut |m| match m {
            ServerMessage::Canvas { seq, ev, .. } => events.push((seq, ev)),
            other => kinds.push(
                serde_json::to_value(&other).unwrap()["type"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            ),
        },
        &Cancel::new(),
    );
    (events, kinds)
}

fn thoughts(events: &[(u64, CanvasEvent)]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|(_, ev)| match ev {
            CanvasEvent::Think { id, text } => Some((id.clone(), text.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_plan_thinks_before_planning_and_before_every_step() {
    let engine = engine("codex");
    let (events, kinds) = run(&engine, "plan", "ru");
    assert_eq!(kinds.last().map(String::as_str), Some("done"), "{kinds:?}");
    assert_eq!(
        thoughts(&events),
        [
            ("root".into(), "Разбиваю задачу на шаги".into()),
            ("root".into(), "Шаг 1 из 2: tags".into()),
            ("root".into(), "Шаг 2 из 2: colors".into()),
        ]
    );
    // Numbered by the session stream like every other event, with no gaps.
    let seqs: Vec<u64> = events.iter().map(|(s, _)| *s).collect();
    assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "{seqs:?}");
    // The plan thought comes after the plan stage, a step's thought before the step starts.
    let at = |pred: &dyn Fn(&CanvasEvent) -> bool| events.iter().position(|(_, e)| pred(e));
    let stage_plan = at(&|e| {
        matches!(
            e,
            CanvasEvent::Stage {
                id: vf_protocol::canvas::Stage::Plan
            }
        )
    })
    .unwrap();
    let first_think = at(&|e| matches!(e, CanvasEvent::Think { .. })).unwrap();
    let first_step = at(&|e| matches!(e, CanvasEvent::Step { .. })).unwrap();
    assert!(stage_plan < first_think && first_think < first_step);
}

#[test]
fn auto_thinks_before_rating_and_before_the_work() {
    let engine = engine("openrouter");
    let (events, kinds) = run(&engine, "auto", "en");
    assert_eq!(kinds.last().map(String::as_str), Some("done"), "{kinds:?}");
    let thoughts = thoughts(&events);
    assert_eq!(
        thoughts.first(),
        Some(&("root".into(), "Working out how big the task is".into()))
    );
    assert!(thoughts.len() >= 2, "{thoughts:?}");
    assert!(thoughts.iter().all(|(id, _)| id == "root"));
}

#[test]
fn one_model_thinks_once_before_its_run() {
    let engine = engine("codex");
    let (events, _) = run(&engine, "codex", "en");
    assert_eq!(
        thoughts(&events),
        [("root".into(), "Working on the task".into())]
    );
}
