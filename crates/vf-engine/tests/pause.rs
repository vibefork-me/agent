//! Stop, resume and cancel of a running task: the step in progress stops at once, resume runs it again,
//! cancelling a paused task undoes its changes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    missing_docs
)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vf_engine::Engine;
use vf_platform::Settings;
use vf_platform::settings::Storage;
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, Cancel, CostClass, ModelInfo, TaskRequest};
use vf_providers::{Answer, AskRequest, Output, Provider, RunRequest, RunResult};

/// The first run writes `half.txt` and works until it is interrupted; later runs write `done.txt`.
struct Slow {
    runs: AtomicUsize,
    asks: AtomicUsize,
    started: Mutex<Option<mpsc::Sender<()>>>,
}

impl Provider for Slow {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            provider: "codex".into(),
            model: "m1".into(),
            tier: 1,
            cost: CostClass::Free,
            label: "m1".into(),
        }]
    }
    fn can_resume(&self) -> bool {
        false
    }
    /// The first call (making a plan) waits until it is interrupted; later calls answer a plan.
    fn ask(&self, _: &AskRequest<'_>, cancel: &Cancel) -> Result<Answer, String> {
        if self.asks.fetch_add(1, Ordering::SeqCst) == 0 {
            if let Some(tx) = self.started.lock().unwrap().take() {
                tx.send(()).unwrap();
            }
            while !cancel.wait(Duration::from_millis(20)) {}
            return Err("cancelled".into());
        }
        Ok(Answer {
            text: r#"{"steps":[{"title":"only","prompt":"do it","complexity":"simple"}]}"#.into(),
            cost_usd: None,
        })
    }
    fn run(&self, req: &RunRequest<'_>, _: &mut dyn FnMut(Output), cancel: &Cancel) -> RunResult {
        let n = self.runs.fetch_add(1, Ordering::SeqCst) + 1;
        if n == 1 {
            std::fs::write(req.dir.join("half.txt"), "half").unwrap();
            if let Some(tx) = self.started.lock().unwrap().take() {
                tx.send(()).unwrap();
            }
            while !cancel.wait(Duration::from_millis(20)) {}
            return RunResult {
                code: 1,
                cancelled: true,
                ..RunResult::default()
            };
        }
        std::fs::write(req.dir.join("done.txt"), "done").unwrap();
        RunResult {
            code: 0,
            ok: true,
            summary: "done".into(),
            ..RunResult::default()
        }
    }
}

fn setup() -> (PathBuf, Arc<Engine>, Arc<Slow>, mpsc::Receiver<()>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let base = std::env::temp_dir().join(format!(
        "vf-pause-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("projects/demo")).unwrap();
    std::fs::write(base.join("projects/demo/README.md"), "user file").unwrap();
    let settings = Settings {
        state_dir: base.join("state"),
        port: 1,
        extra_origins: vec![],
        ignored_extra_origins: vec![],
        ui_bundle_url: String::new(),
        storage: Storage {
            projects: base.join("projects"),
            // Shared with the other engine tests: toolchains are downloaded once.
            tools: PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("vf-engine-tools"),
            caches: base.join("caches"),
            artifacts: base.join("artifacts"),
        },
        simulate_clean_machine: true,
        update_url: String::new(),
        auto_update: false,
    };
    let (tx, rx) = mpsc::channel();
    let p = Arc::new(Slow {
        runs: AtomicUsize::new(0),
        asks: AtomicUsize::new(0),
        started: Mutex::new(Some(tx)),
    });
    let engine = Arc::new(Engine::new(&settings, vec![p.clone()]));
    (base, engine, p, rx)
}

fn task() -> TaskRequest {
    task_with("codex")
}

fn task_with(provider: &str) -> TaskRequest {
    TaskRequest {
        project: "demo".into(),
        prompt: "do it".into(),
        provider: Some(provider.into()),
        model: Some("m1".into()),
        continue_: false,
        lang: None,
    }
}

/// Runs the task on a thread; its messages of the existing protocol come through the channel.
fn start(
    engine: &Arc<Engine>,
    cancel: &Cancel,
) -> (std::thread::JoinHandle<()>, mpsc::Receiver<ServerMessage>) {
    start_task(engine, cancel, task())
}

fn start_task(
    engine: &Arc<Engine>,
    cancel: &Cancel,
    task: TaskRequest,
) -> (std::thread::JoinHandle<()>, mpsc::Receiver<ServerMessage>) {
    let (tx, rx) = mpsc::channel();
    let (engine, cancel) = (engine.clone(), cancel.clone());
    let worker = std::thread::spawn(move || {
        engine.run_task(
            task,
            &mut |m| {
                if !matches!(m, ServerMessage::Canvas { .. }) {
                    let _ = tx.send(m);
                }
            },
            &cancel,
        );
    });
    (worker, rx)
}

fn until(
    rx: &mpsc::Receiver<ServerMessage>,
    want: impl Fn(&ServerMessage) -> bool,
) -> Vec<ServerMessage> {
    let mut seen = vec![];
    loop {
        let m = rx.recv_timeout(Duration::from_secs(20)).expect("message");
        let hit = want(&m);
        seen.push(m);
        if hit {
            return seen;
        }
    }
}

#[test]
fn a_paused_step_runs_again_on_resume_and_the_task_finishes() {
    let (base, engine, p, started) = setup();
    let cancel = Cancel::new();
    let (worker, rx) = start(&engine, &cancel);
    started.recv_timeout(Duration::from_secs(20)).unwrap();
    cancel.pause();
    until(&rx, |m| matches!(m, ServerMessage::Paused));
    cancel.resume();
    let rest = until(&rx, |m| {
        matches!(m, ServerMessage::Done { .. } | ServerMessage::Cancelled)
    });
    worker.join().unwrap();
    assert!(
        matches!(rest.first(), Some(ServerMessage::Resumed)),
        "{rest:?}"
    );
    assert!(
        matches!(rest.last(), Some(ServerMessage::Done { code: 0, .. })),
        "{rest:?}"
    );
    assert_eq!(p.runs.load(Ordering::SeqCst), 2);
    let demo = base.join("projects/demo");
    assert!(demo.join("half.txt").exists() && demo.join("done.txt").exists());
    // The stream has the pause too.
    let types: Vec<String> = engine
        .canvas("demo", 1)
        .unwrap()
        .events
        .iter()
        .map(|r| {
            serde_json::to_value(&r.ev).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    let paused = types.iter().position(|t| t == "paused").unwrap();
    assert_eq!(
        types.get(paused + 1).map(String::as_str),
        Some("resumed"),
        "{types:?}"
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_pause_while_the_ai_makes_the_plan_asks_again_on_resume() {
    let (base, engine, p, started) = setup();
    // Steps run at once in this test.
    p.runs.store(1, Ordering::SeqCst);
    let cancel = Cancel::new();
    let (worker, rx) = start_task(&engine, &cancel, task_with("plan"));
    started.recv_timeout(Duration::from_secs(20)).unwrap();
    cancel.pause();
    until(&rx, |m| matches!(m, ServerMessage::Paused));
    cancel.resume();
    let rest = until(&rx, |m| {
        matches!(
            m,
            ServerMessage::Done { .. } | ServerMessage::Cancelled | ServerMessage::Error { .. }
        )
    });
    worker.join().unwrap();
    assert!(
        matches!(rest.first(), Some(ServerMessage::Resumed)),
        "{rest:?}"
    );
    assert!(
        rest.iter().any(|m| matches!(m, ServerMessage::Plan { .. })),
        "{rest:?}"
    );
    assert!(
        matches!(rest.last(), Some(ServerMessage::Done { code: 0, .. })),
        "{rest:?}"
    );
    assert_eq!(p.asks.load(Ordering::SeqCst), 2);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn cancelling_a_paused_task_undoes_its_changes() {
    let (base, engine, p, started) = setup();
    let demo = base.join("projects/demo");
    let cancel = Cancel::new();
    let (worker, rx) = start(&engine, &cancel);
    started.recv_timeout(Duration::from_secs(20)).unwrap();
    cancel.pause();
    until(&rx, |m| matches!(m, ServerMessage::Paused));
    assert!(
        demo.join("half.txt").exists(),
        "the interrupted step's work is kept while paused"
    );
    cancel.cancel_and_undo();
    let rest = until(&rx, |m| {
        matches!(m, ServerMessage::Done { .. } | ServerMessage::Cancelled)
    });
    worker.join().unwrap();
    assert!(
        matches!(rest.last(), Some(ServerMessage::Cancelled)),
        "{rest:?}"
    );
    assert_eq!(p.runs.load(Ordering::SeqCst), 1);
    assert!(!demo.join("half.txt").exists(), "the task's file is gone");
    assert_eq!(
        std::fs::read_to_string(demo.join("README.md")).unwrap(),
        "user file"
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_plain_cancel_keeps_the_work() {
    let (base, engine, _, started) = setup();
    let demo = base.join("projects/demo");
    let cancel = Cancel::new();
    let (worker, rx) = start(&engine, &cancel);
    started.recv_timeout(Duration::from_secs(20)).unwrap();
    cancel.cancel();
    let rest = until(&rx, |m| {
        matches!(m, ServerMessage::Done { .. } | ServerMessage::Cancelled)
    });
    worker.join().unwrap();
    assert!(
        matches!(rest.last(), Some(ServerMessage::Cancelled)),
        "{rest:?}"
    );
    assert!(demo.join("half.txt").exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_paused_build_builds_again_on_resume() {
    if vf_providers::process::find_program("npm").is_none() {
        eprintln!("skipped: npm is not installed");
        return;
    }
    let (base, engine, _, _) = setup();
    let demo = base.join("projects/demo");
    // The first build leaves a marker and hangs; the next one builds at once.
    let script = "node -e \"const fs=require('fs');if(fs.existsSync('marker')){fs.mkdirSync('dist',{recursive:true});fs.writeFileSync('dist/out.txt','x')}else{fs.writeFileSync('marker','1');setTimeout(()=>{},60000)}\"";
    std::fs::write(
        demo.join("package.json"),
        serde_json::json!({ "scripts": { "build": script } }).to_string(),
    )
    .unwrap();
    let cancel = Cancel::new();
    let (tx, rx) = mpsc::channel();
    let worker = {
        let (engine, cancel) = (engine.clone(), cancel.clone());
        std::thread::spawn(move || {
            engine.build(
                "demo",
                &mut |m| {
                    if !matches!(m, ServerMessage::Canvas { .. }) {
                        let _ = tx.send(m);
                    }
                },
                &cancel,
            );
        })
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(240);
    while !demo.join("marker").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the build did not start"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    cancel.pause();
    until(&rx, |m| matches!(m, ServerMessage::Paused));
    cancel.resume();
    let rest = until(&rx, |m| {
        matches!(
            m,
            ServerMessage::BuildDone { .. }
                | ServerMessage::Cancelled
                | ServerMessage::Error { .. }
        )
    });
    worker.join().unwrap();
    assert!(
        matches!(rest.first(), Some(ServerMessage::Resumed)),
        "{rest:?}"
    );
    assert!(
        matches!(rest.last(), Some(ServerMessage::BuildDone { code: 0, .. })),
        "{rest:?}"
    );
    let _ = std::fs::remove_dir_all(&base);
}
