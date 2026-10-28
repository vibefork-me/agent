//! The time left of a task follows its stages and steps in the session stream, measured on the
//! project's earlier runs; the first run of a project measures, it has nothing to estimate by.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vf_engine::Engine;
use vf_platform::Settings;
use vf_platform::settings::Storage;
use vf_protocol::canvas::{CanvasEvent, EtaScope};
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
        "vf-eta-{}-{}",
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

fn etas(events: &[(u64, CanvasEvent)]) -> Vec<(usize, EtaScope)> {
    events
        .iter()
        .enumerate()
        .filter_map(|(i, (_, ev))| match ev {
            CanvasEvent::Eta { scope, .. } => Some((i, *scope)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_second_task_is_estimated_by_the_first() {
    let engine = engine("codex");
    let (first, kinds) = run(&engine, "plan", "en");
    assert_eq!(kinds.last().map(String::as_str), Some("done"), "{kinds:?}");
    assert!(etas(&first).is_empty(), "{first:?}");

    let (second, kinds) = run(&engine, "plan", "en");
    assert_eq!(kinds.last().map(String::as_str), Some("done"), "{kinds:?}");
    let etas = etas(&second);
    assert!(etas.iter().all(|(_, s)| *s == EtaScope::Task), "{etas:?}");
    // After the understanding stage, the plan stage, the plan and each of its two steps.
    let after = |i: usize| second.get(i.wrapping_sub(1)).map(|(_, e)| e.clone());
    let before: Vec<String> = etas
        .iter()
        .filter_map(|(i, _)| after(*i))
        .map(|e| {
            serde_json::to_value(e).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(before.len() >= 5, "{second:?}");
    assert_eq!(before.first().map(String::as_str), Some("stage"));
    assert_eq!(
        before.iter().filter(|t| *t == "step").count(),
        2,
        "{before:?}"
    );
    assert_eq!(
        before.iter().filter(|t| *t == "plan").count(),
        1,
        "{before:?}"
    );
    // Numbered like every other event, with no gaps.
    let seqs: Vec<u64> = second.iter().map(|(s, _)| *s).collect();
    assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "{seqs:?}");
}
