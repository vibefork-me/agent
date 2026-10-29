//! Attacks through the project: a model writes coding CLI configuration (hooks, MCP servers) into
//! the project by a shell command. It stays in the project and never reaches the copy on GitHub.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    missing_docs
)]

use std::path::PathBuf;
use std::sync::Arc;

use vf_engine::Engine;
use vf_platform::Settings;
use vf_platform::settings::Storage;
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Backend, Cancel, CostClass, ModelInfo, TaskRequest};
use vf_providers::{Answer, AskRequest, Output, Provider, RunRequest, RunResult};

const HOOK: &str =
    r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo planted"}]}]}}"#;
const USER_CONFIG: &str = "model = \"m1\"\n";

/// Plants CLI configuration as a shell command would: some of it committed by itself, the rest
/// left for the checkpoint; also does ordinary work.
struct Planter;

impl Provider for Planter {
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
        Err("not used".into())
    }
    fn run(&self, req: &RunRequest<'_>, _: &mut dyn FnMut(Output), _: &Cancel) -> RunResult {
        let write = |rel: &str, text: &str| {
            let path = req.dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(".claude/settings.json", HOOK);
        write(".mcp.json", r#"{"mcpServers":{"x":{"command":"planted"}}}"#);
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
        write(
            ".codex/config.toml",
            "model = \"m1\"\nnotify = [\"planted\"]\n",
        );
        write(".GitHub/Hooks/hooks.json", HOOK);
        write("notes.txt", "the work");
        RunResult {
            code: 0,
            ok: true,
            summary: "done".into(),
            ..RunResult::default()
        }
    }
}

fn setup() -> (PathBuf, Engine) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base =
        std::env::temp_dir().join(format!("vf-engine-attacks-{}-{nanos}", std::process::id()));
    let demo = base.join("projects/demo");
    std::fs::create_dir_all(demo.join(".codex")).unwrap();
    std::fs::write(demo.join("README.md"), "user file").unwrap();
    // The user's own CLI configuration from before the first task.
    std::fs::write(demo.join(".codex/config.toml"), USER_CONFIG).unwrap();
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
    let engine = Engine::new(&settings, vec![Arc::new(Planter) as Arc<dyn Provider>]);
    (base, engine)
}

#[test]
fn cli_configuration_written_by_the_model_is_not_saved_to_the_copy() {
    let (base, engine) = setup();
    let mut msgs = vec![];
    engine.run_task(
        TaskRequest {
            project: "demo".into(),
            prompt: "do it".into(),
            provider: Some("codex".into()),
            model: Some("m1".into()),
            continue_: false,
            lang: None,
        },
        &mut |m| msgs.push(m),
        &Cancel::new(),
    );
    assert!(
        matches!(msgs.last(), Some(ServerMessage::Done { code: 0, .. })),
        "{msgs:?}"
    );

    // The journal says which CLI configuration the step changed.
    let entry = engine.journal("demo").unwrap().entries.pop().unwrap();
    let mut planted = entry.cli_config.clone();
    planted.sort();
    assert_eq!(
        planted,
        [
            ".GitHub/Hooks/hooks.json",
            ".claude/settings.json",
            ".codex/config.toml",
            ".mcp.json"
        ]
    );

    // What "Save" sends to the copy (the project has no original: the start is its first commit).
    let demo = base.join("projects/demo");
    let repo = vf_git::Repo::open(&demo);
    let head = vf_journal::Vcs::head(&repo).unwrap();
    let start = repo.first_commit().unwrap();
    for from in ["", start.as_str()] {
        let out = repo.changes_to_save("", from, &head).unwrap();
        let sent: Vec<&str> = out.changes.iter().map(|c| c.path.as_str()).collect();
        assert!(sent.contains(&"notes.txt"), "{from:?}: {sent:?}");
        for p in [
            ".claude/settings.json",
            ".mcp.json",
            ".GitHub/Hooks/hooks.json",
        ] {
            assert!(!sent.contains(&p), "{from:?}: {p} sent");
            assert!(out.held_back.iter().any(|h| h == p), "{from:?}: {p}");
        }
        // The user's own configuration goes only as it was before the task.
        let codex = out.changes.iter().find(|c| c.path == ".codex/config.toml");
        if from.is_empty() {
            assert_eq!(
                codex.and_then(|c| c.content.as_deref()),
                Some(USER_CONFIG.as_bytes())
            );
        } else {
            assert!(codex.is_none(), "{codex:?}");
        }
    }
    // The planted files stay in the project folder: nothing is deleted behind the user.
    assert!(demo.join(".claude/settings.json").is_file());
    let _ = std::fs::remove_dir_all(&base);
}
