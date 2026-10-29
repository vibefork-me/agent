//! "Explain": a question about the project or one of its parts is answered in words in the session
//! stream. Only a read-only question goes to the model; the project's files, its git history and
//! its journal stay as they were, and no tools are prepared.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use vf_engine::Engine;
use vf_platform::Settings;
use vf_platform::settings::Storage;
use vf_protocol::canvas::CanvasEvent;
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, Cancel, CostClass, ExplainRequest, ModelInfo};
use vf_providers::{Answer, AskRequest, Output, Provider, RunRequest, RunResult};

/// What the model was asked.
#[derive(Debug, Clone)]
struct Asked {
    system: String,
    prompt: String,
}

/// Answers every question with `answer` (or fails with it, when `fails`); a run is never expected.
struct Reader {
    id: &'static str,
    answer: &'static str,
    fails: bool,
    asked: Mutex<Vec<Asked>>,
}

impl Reader {
    fn new(id: &'static str, answer: &'static str, fails: bool) -> Arc<Self> {
        Arc::new(Self {
            id,
            answer,
            fails,
            asked: Mutex::new(Vec::new()),
        })
    }
}

impl Provider for Reader {
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
        false
    }
    fn ask(&self, req: &AskRequest<'_>, _: &Cancel) -> Result<Answer, String> {
        self.asked.lock().unwrap().push(Asked {
            system: req.system.unwrap_or_default().to_owned(),
            prompt: req.prompt.to_owned(),
        });
        if self.fails {
            return Err(self.answer.into());
        }
        Ok(Answer {
            text: self.answer.into(),
            cost_usd: None,
        })
    }
    fn run(&self, _: &RunRequest<'_>, _: &mut dyn FnMut(Output), _: &Cancel) -> RunResult {
        panic!("an explanation never runs the model on the project");
    }
}

fn settings(base: &Path) -> Settings {
    Settings {
        state_dir: base.join("state"),
        port: 1,
        extra_origins: vec![],
        ignored_extra_origins: vec![],
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
    }
}

/// A made-up notes app under git with a map of two parts.
fn setup(providers: Vec<Arc<dyn Provider>>) -> (PathBuf, Engine) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let base = std::env::temp_dir().join(format!(
        "vf-explain-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&base);
    let demo = base.join("projects/notes-app");
    std::fs::create_dir_all(demo.join("src/notes")).unwrap();
    std::fs::write(demo.join("README.md"), "A small notes app.").unwrap();
    std::fs::write(demo.join("src/notes/list.ts"), "export const notes = [];").unwrap();
    std::fs::write(demo.join("src/notes/tags.ts"), "export const tags = [];").unwrap();
    vf_git::Repo::init_with(demo, "start").unwrap();
    std::fs::create_dir_all(base.join("state/projects")).unwrap();
    std::fs::write(
        base.join("state/projects/notes-app.map.json"),
        r#"{"revision":"r","by":"t","parts":[
            {"name":"Guide","summary":"Tells what the app is.","paths":["README.md"]},
            {"name":"Notes","summary":"Keeps your notes and their tags.","paths":["src/notes"]}],"links":[]}"#,
    )
    .unwrap();
    let engine = Engine::new(&settings(&base), providers);
    (base, engine)
}

/// Every file under `dir` with its bytes, the git folder included.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut all = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                all.insert(p.clone(), std::fs::read(&p).unwrap());
            }
        }
    }
    all
}

fn request(question: &str, part: Option<&str>) -> ExplainRequest {
    ExplainRequest {
        project: "notes-app".into(),
        question: question.into(),
        part: part.map(str::to_owned),
        provider: None,
        model: None,
        lang: Some("ru".into()),
    }
}

/// Canvas events and other messages of one explanation.
fn explain(engine: &Engine, req: ExplainRequest) -> (Vec<CanvasEvent>, Vec<ServerMessage>) {
    let mut events = Vec::new();
    let mut other = Vec::new();
    engine.explain(
        req,
        &mut |m| match m {
            ServerMessage::Canvas { ev, .. } => events.push(ev),
            m => other.push(m),
        },
        &Cancel::new(),
    );
    (events, other)
}

fn code(m: &[ServerMessage]) -> Option<String> {
    match m {
        [ServerMessage::Error { code, .. }] => code.clone(),
        _ => None,
    }
}

#[test]
fn a_part_is_explained_in_words_and_nothing_changes() {
    let reader = Reader::new(
        "claude",
        "  Здесь хранятся ваши заметки и их метки.  ",
        false,
    );
    let (base, engine) = setup(vec![reader.clone()]);
    let demo = base.join("projects/notes-app");
    let files_before = snapshot(&demo);
    let journal_before = engine.journal("notes-app").unwrap();

    let (events, other) = explain(&engine, request("Что это за часть?", Some("p2")));

    assert!(other.is_empty(), "{other:?}");
    assert_eq!(
        events,
        [
            CanvasEvent::Question {
                text: "Что это за часть?".into(),
                id: Some("p2".into()),
            },
            CanvasEvent::Think {
                id: "p2".into(),
                text: "Разбираюсь с частью «Notes»".into(),
            },
            CanvasEvent::Explained {
                text: "Здесь хранятся ваши заметки и их метки.".into(),
                id: Some("p2".into()),
                code: None,
            },
        ]
    );
    // The model got the part and the question as data, and the read-only rules in Russian.
    let asked = reader.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].prompt.starts_with("PROJECT DATA"));
    assert!(asked[0].prompt.contains("Part of the map: Notes"));
    assert!(asked[0].prompt.contains("src/notes"));
    assert!(asked[0].prompt.ends_with("Что это за часть?"));
    assert!(
        asked[0]
            .system
            .contains("you do not change, create or delete files")
    );
    assert!(asked[0].system.ends_with("Answer in Russian."));

    // Not a byte of the project or its history changed, nothing was journaled, no tools came.
    assert_eq!(snapshot(&demo), files_before);
    assert_eq!(engine.journal("notes-app").unwrap(), journal_before);
    assert!(!base.join("tools").exists());

    // The question and the answer stay in the project's session stream for a reload.
    let kept = engine.canvas("notes-app", 1).unwrap();
    assert!(
        kept.events
            .iter()
            .any(|r| matches!(&r.ev, CanvasEvent::Explained { code: None, .. }))
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_question_about_the_project_has_no_part() {
    let reader = Reader::new("codex", "It keeps notes.", false);
    let (base, engine) = setup(vec![reader.clone()]);
    let mut req = request("What is this project?", Some("root"));
    req.lang = Some("en".into());
    let (events, _) = explain(&engine, req);
    assert_eq!(
        events.first(),
        Some(&CanvasEvent::Question {
            text: "What is this project?".into(),
            id: None,
        })
    );
    assert_eq!(
        events.get(1),
        Some(&CanvasEvent::Think {
            id: "root".into(),
            text: "Looking into your question".into(),
        })
    );
    assert!(
        !reader.asked.lock().unwrap()[0]
            .prompt
            .contains("Part of the map")
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn the_next_model_answers_when_one_fails_and_a_coded_end_when_none_does() {
    let broken = Reader::new("claude", "limit reached", true);
    let reader = Reader::new("codex", "It keeps notes.", false);
    let (base, engine) = setup(vec![broken.clone(), reader.clone()]);
    let (events, _) = explain(&engine, request("What is it?", None));
    assert!(matches!(
        events.last(),
        Some(CanvasEvent::Explained { text, code: None, .. }) if text == "It keeps notes."
    ));
    assert_eq!(broken.asked.lock().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&base);

    let (base, engine) = setup(vec![Reader::new("claude", "down", true)]);
    let (events, _) = explain(&engine, request("What is it?", None));
    assert_eq!(
        events.last(),
        Some(&CanvasEvent::Explained {
            text: String::new(),
            id: None,
            code: Some("explain_failed".into()),
        })
    );
    let _ = std::fs::remove_dir_all(&base);

    let (base, engine) = setup(Vec::new());
    let (events, _) = explain(&engine, request("What is it?", None));
    assert!(matches!(
        events.last(),
        Some(CanvasEvent::Explained { code: Some(c), .. }) if c == "no_models"
    ));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn the_chosen_model_is_asked_first() {
    let first = Reader::new("claude", "from claude", false);
    let chosen = Reader::new("codex", "from codex", false);
    let (base, engine) = setup(vec![first.clone(), chosen.clone()]);
    let mut req = request("What is it?", None);
    req.provider = Some("codex".into());
    req.model = Some("m1".into());
    let (events, _) = explain(&engine, req);
    assert!(matches!(
        events.last(),
        Some(CanvasEvent::Explained { text, .. }) if text == "from codex"
    ));
    assert!(first.asked.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn bad_requests_are_refused_with_a_code_before_anything_is_asked() {
    let reader = Reader::new("claude", "x", false);
    let (base, engine) = setup(vec![reader.clone()]);
    let (events, other) = explain(&engine, request("   ", None));
    assert!(events.is_empty());
    assert_eq!(code(&other).as_deref(), Some("bad_prompt"));

    let mut req = request("What?", None);
    req.project = "../notes-app".into();
    let (_, other) = explain(&engine, req);
    assert_eq!(code(&other).as_deref(), Some("not_found"));

    let (events, other) = explain(&engine, request("What?", Some("p9")));
    assert!(events.is_empty());
    assert_eq!(code(&other).as_deref(), Some("not_found"));

    let long = "x".repeat(vf_engine::MAX_PROMPT + 1);
    let (_, other) = explain(&engine, request(&long, None));
    assert_eq!(code(&other).as_deref(), Some("bad_prompt"));

    assert!(reader.asked.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&base);
}
