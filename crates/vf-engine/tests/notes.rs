//! Messages of the person while a task runs: taken before the next step of a plan (or before an
//! interrupted step runs again), put into the prompt, answered with `note.taken` in the stream.

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
use vf_protocol::canvas::{CanvasEvent, NoteAttachment};
use vf_protocol::note::MAX_TEXT_ATTACHMENT;
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, Cancel, CostClass, ModelInfo, TaskRequest};
use vf_providers::{Answer, AskRequest, Output, Provider, RunRequest, RunResult};

/// Plans two steps; the first run tells the test it started and waits for `release` (or for a
/// pause when `pause_first` is set). Every prompt is kept, with whether the files it names exist.
struct Fake {
    plan: &'static str,
    pause_first: bool,
    prompts: Mutex<Vec<String>>,
    seen_files: Mutex<Vec<PathBuf>>,
    runs: AtomicUsize,
    started: Mutex<Option<mpsc::Sender<()>>>,
    release: Mutex<Option<mpsc::Receiver<()>>>,
}

impl Provider for Fake {
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
    fn ask(&self, _: &AskRequest<'_>, _: &Cancel) -> Result<Answer, String> {
        Ok(Answer {
            text: self.plan.into(),
            cost_usd: None,
        })
    }
    fn run(&self, req: &RunRequest<'_>, _: &mut dyn FnMut(Output), cancel: &Cancel) -> RunResult {
        self.prompts.lock().unwrap().push(req.prompt.to_owned());
        for line in req.prompt.lines() {
            if let Some((_, rest)) = line.split_once("saved at ") {
                let path = PathBuf::from(rest.split(" (").next().unwrap());
                if path.exists() {
                    self.seen_files.lock().unwrap().push(path);
                }
            }
        }
        let n = self.runs.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            if let Some(tx) = self.started.lock().unwrap().take() {
                tx.send(()).unwrap();
            }
            if self.pause_first {
                while !cancel.wait(Duration::from_millis(20)) {}
                return RunResult {
                    code: 1,
                    cancelled: true,
                    ..RunResult::default()
                };
            }
            let release = self.release.lock().unwrap().take().unwrap();
            release.recv_timeout(Duration::from_secs(20)).unwrap();
        }
        std::fs::write(req.dir.join(format!("step{n}.txt")), "x").unwrap();
        RunResult {
            code: 0,
            ok: true,
            summary: "done".into(),
            ..RunResult::default()
        }
    }
}

const TWO_STEPS: &str = r#"{"steps":[{"title":"tags","prompt":"add tags","complexity":"simple"},{"title":"colors","prompt":"color the tags","complexity":"simple"}]}"#;

struct Setup {
    base: PathBuf,
    engine: Arc<Engine>,
    p: Arc<Fake>,
    started: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
}

fn setup(pause_first: bool) -> Setup {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let base = std::env::temp_dir().join(format!(
        "vf-notes-{}-{}",
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
    let (tx, started) = mpsc::channel();
    let (release, rx) = mpsc::channel();
    let p = Arc::new(Fake {
        plan: TWO_STEPS,
        pause_first,
        prompts: Mutex::new(Vec::new()),
        seen_files: Mutex::new(Vec::new()),
        runs: AtomicUsize::new(0),
        started: Mutex::new(Some(tx)),
        release: Mutex::new(Some(rx)),
    });
    let engine = Arc::new(Engine::new(&settings, vec![p.clone()]));
    Setup {
        base,
        engine,
        p,
        started,
        release,
    }
}

fn start(
    engine: &Arc<Engine>,
    cancel: &Cancel,
    provider: &str,
    lang: &str,
) -> (std::thread::JoinHandle<()>, mpsc::Receiver<ServerMessage>) {
    let task = TaskRequest {
        project: "notes-app".into(),
        prompt: "add tags to notes".into(),
        provider: Some(provider.into()),
        model: Some("m1".into()),
        continue_: false,
        lang: Some(lang.into()),
    };
    let (tx, rx) = mpsc::channel();
    let (engine, cancel) = (engine.clone(), cancel.clone());
    let worker = std::thread::spawn(move || {
        engine.run_task(
            task,
            &mut |m| {
                let _ = tx.send(m);
            },
            &cancel,
        );
    });
    (worker, rx)
}

fn until_end(rx: &mpsc::Receiver<ServerMessage>) -> Vec<ServerMessage> {
    let mut seen = vec![];
    loop {
        let m = rx.recv_timeout(Duration::from_secs(20)).expect("message");
        let end = matches!(m, ServerMessage::Done { .. } | ServerMessage::Cancelled);
        seen.push(m);
        if end {
            return seen;
        }
    }
}

fn taken(m: &ServerMessage) -> Option<(&str, &str)> {
    match m {
        ServerMessage::Canvas {
            ev: CanvasEvent::NoteTaken { note_id, reply, .. },
            ..
        } => Some((note_id, reply)),
        _ => None,
    }
}

#[test]
fn a_note_during_a_plan_step_goes_into_the_next_step() {
    let s = setup(false);
    let cancel = Cancel::new();
    let (worker, rx) = start(&s.engine, &cancel, "plan", "en");
    s.started.recv_timeout(Duration::from_secs(20)).unwrap();

    let long = "t".repeat(MAX_TEXT_ATTACHMENT + 500);
    let id = cancel
        .post_note(
            "make the tags blue".into(),
            vec![
                NoteAttachment {
                    name: "palette.txt".into(),
                    mime: "text/plain".into(),
                    data: long,
                },
                NoteAttachment {
                    name: "screen shot.png".into(),
                    mime: "image/png".into(),
                    data: "data:image/png;base64,iVBORw0KGgo=".into(),
                },
            ],
        )
        .unwrap();
    assert_eq!(id, 1);
    s.release.send(()).unwrap();
    let all = until_end(&rx);
    worker.join().unwrap();

    assert!(
        matches!(all.last(), Some(ServerMessage::Done { code: 0, .. })),
        "{all:?}"
    );
    // Taken once, after step 1 ended and before step 2 started.
    let notes: Vec<usize> = all
        .iter()
        .enumerate()
        .filter_map(|(i, m)| taken(m).map(|_| i))
        .collect();
    assert_eq!(notes.len(), 1, "{all:?}");
    let step1_done = all
        .iter()
        .position(|m| matches!(m, ServerMessage::StepDone { n: 1, .. }))
        .unwrap();
    let step2 = all
        .iter()
        .position(|m| matches!(m, ServerMessage::Started { step: Some(st), .. } if st.n == 2))
        .unwrap();
    assert!(step1_done < notes[0] && notes[0] < step2, "{all:?}");
    let (note_id, reply) = taken(&all[notes[0]]).unwrap();
    assert_eq!(note_id, "1");
    assert!(reply.contains("next step"), "{reply}");

    let prompts = s.p.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 2);
    assert!(!prompts[0].contains("make the tags blue"));
    let p2 = &prompts[1];
    assert!(
        p2.contains("color the tags") && p2.contains("make the tags blue"),
        "{p2}"
    );
    // The text attachment is cut, the image is saved outside the project and named by path.
    assert!(p2.contains(&"t".repeat(MAX_TEXT_ATTACHMENT)));
    assert!(!p2.contains(&"t".repeat(MAX_TEXT_ATTACHMENT + 1)));
    assert!(p2.contains("(cut: 500 more characters)"));
    let files = s.p.seen_files.lock().unwrap().clone();
    assert_eq!(files.len(), 1, "{p2}");
    assert!(files[0].starts_with(s.base.join("caches")));
    assert!(files[0].ends_with("1-2-screenshot.png"));
    // The folder is gone with the task; the project has no trace of the message.
    assert!(!files[0].exists());
    assert!(
        !s.base
            .join("projects/notes-app/1-2-screenshot.png")
            .exists()
    );

    // The stream keeps the answer.
    let events = s.engine.canvas("notes-app", 1).unwrap().events;
    assert!(
        events
            .iter()
            .any(|r| matches!(&r.ev, CanvasEvent::NoteTaken { note_id, .. } if note_id == "1"))
    );
    let _ = std::fs::remove_dir_all(&s.base);
}

#[test]
fn a_note_during_a_pause_goes_into_the_step_run_again() {
    let s = setup(true);
    let cancel = Cancel::new();
    let (worker, rx) = start(&s.engine, &cancel, "codex", "ru");
    s.started.recv_timeout(Duration::from_secs(20)).unwrap();
    cancel.pause();
    loop {
        let m = rx.recv_timeout(Duration::from_secs(20)).unwrap();
        if matches!(m, ServerMessage::Paused) {
            break;
        }
    }
    cancel
        .post_note("use a sans-serif font".into(), Vec::new())
        .unwrap();
    cancel.resume();
    let rest = until_end(&rx);
    worker.join().unwrap();
    assert!(
        matches!(rest.last(), Some(ServerMessage::Done { code: 0, .. })),
        "{rest:?}"
    );
    let (_, reply) = rest.iter().find_map(taken).unwrap();
    assert!(reply.contains("учту"), "{reply}");
    let prompts = s.p.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("use a sans-serif font"));
    let _ = std::fs::remove_dir_all(&s.base);
}

#[test]
fn a_note_after_the_task_is_refused() {
    let s = setup(false);
    let cancel = Cancel::new();
    let (worker, rx) = start(&s.engine, &cancel, "plan", "en");
    s.started.recv_timeout(Duration::from_secs(20)).unwrap();
    cancel.cancel();
    s.release.send(()).unwrap();
    until_end(&rx);
    worker.join().unwrap();
    assert_eq!(
        cancel.post_note("too late".into(), Vec::new()),
        Err(vf_protocol::note::NoteRejected::NotRunning)
    );
    let _ = std::fs::remove_dir_all(&s.base);
}
