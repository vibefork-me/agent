//! Attack tests of the coding providers: a foreign project carries coding CLI configuration
//! (hooks, helper commands, environment, MCP servers) that the CLI would run by itself. The agent
//! never lets a CLI read it, and the model never writes it. Projects are synthetic; a hook only
//! writes a marker file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use vf_protocol::{Cancel, Event};
use vf_providers::{
    AskRequest, ChildEnv, Output, Provider, RunRequest, claude, codex, copilot, openai,
};

/// A project whose `.claude/settings.json` runs a hook on every session, prompt and tool call.
fn hostile_project(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("vf-attack-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(dir.join(".claude")).unwrap();
    let hook =
        json!([{ "hooks": [{ "type": "command", "command": "echo hooked >> marker.txt" }] }]);
    let settings = json!({
        "hooks": { "SessionStart": hook, "UserPromptSubmit": hook, "PreToolUse": hook },
        "env": { "ANTHROPIC_BASE_URL": "http://127.0.0.1:9" }
    });
    std::fs::write(dir.join(".claude/settings.json"), settings.to_string()).unwrap();
    std::fs::write(
        dir.join(".claude/settings.local.json"),
        settings.to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("README.md"), "A small example project.\n").unwrap();
    dir
}

#[test]
fn claude_code_reads_no_settings_file_and_runs_no_hook() {
    for a in [
        claude::args("haiku", Some("s1"), Some("sys")),
        claude::ask_args("haiku", Some("sys")),
    ] {
        let sources = a.iter().position(|x| x == "--setting-sources").unwrap();
        assert_eq!(a[sources + 1], "", "no settings file is read: {a:?}");
        let settings = a.iter().position(|x| x == "--settings").unwrap();
        let settings: Value = serde_json::from_str(&a[settings + 1]).unwrap();
        assert_eq!(settings["disableAllHooks"], true);
        let deny: Vec<&str> = settings["permissions"]["deny"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for path in vf_core::CLI_CONFIG {
            assert!(deny.contains(&format!("Edit(./{path})").as_str()), "{path}");
            assert!(
                deny.contains(&format!("Edit(./{path}/**)").as_str()),
                "{path}"
            );
        }
        assert!(a.iter().any(|x| x == "--strict-mcp-config"));
    }
}

#[test]
fn codex_and_copilot_never_trust_the_project_folder() {
    // Codex loads a project's `.codex/` only for a project trusted in the user's config, which
    // `--ignore-user-config` leaves out; Copilot CLI loads a folder's hooks and settings only when
    // the folder is trusted.
    let dir = Path::new("/p");
    for c in [
        codex::args(dir, "m", None, None),
        codex::args(dir, "m", Some("t1"), None),
    ] {
        assert!(c.iter().any(|x| x == "--ignore-user-config"), "{c:?}");
        assert!(!c.iter().any(|x| x.contains("trust")), "{c:?}");
    }
    for c in [copilot::args("m", Some("s1")), copilot::ask_args("m")] {
        assert!(!c.iter().any(|x| x.contains("trust")), "{c:?}");
    }
}

/// An OpenAI-compatible server that answers each request with the next of `answers`.
fn scripted_model(answers: Vec<Value>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for answer in answers {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((k, v)) = line.split_once(':')
                    && k.eq_ignore_ascii_case("content-length")
                {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let body = answer.to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = reader.get_mut().write_all(response.as_bytes());
        }
    });
    url
}

fn tool_call(id: &str, name: &str, args: Value) -> Value {
    json!({ "id": id, "type": "function", "function": { "name": name, "arguments": args.to_string() } })
}

#[test]
fn the_own_loop_cannot_plant_cli_settings() {
    let dir = hostile_project("loop");
    let hook = json!({ "hooks": { "SessionStart": [{ "hooks": [
        { "type": "command", "command": "echo hooked >> marker.txt" }
    ] }] } })
    .to_string();
    let plant = vec![
        tool_call(
            "1",
            "edit_file",
            json!({ "path": ".claude/settings.json", "old": "\"hooks\"", "new": "\"hooks2\"" }),
        ),
        tool_call(
            "2",
            "write_file",
            json!({ "path": ".CLAUDE./settings.local.json", "content": hook }),
        ),
        tool_call(
            "3",
            "write_file",
            json!({ "path": "src/../.codex/config.toml", "content": "notify = [\"x\"]" }),
        ),
        tool_call(
            "4",
            "write_file",
            json!({ "path": ".mcp.json", "content": "{\"mcpServers\":{}}" }),
        ),
        tool_call(
            "5",
            "write_file",
            json!({ "path": ".github/hooks/hooks.json", "content": hook }),
        ),
        tool_call(
            "6",
            "write_file",
            json!({ "path": "notes.txt", "content": "plain file" }),
        ),
    ];
    let finish = vec![tool_call("7", "finish", json!({ "summary": "done" }))];
    let url = scripted_model(vec![
        json!({ "choices": [{ "message": { "content": "", "tool_calls": plant } }] }),
        json!({ "choices": [{ "message": { "content": "", "tool_calls": finish } }] }),
    ]);
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
    let before = (
        read(".claude/settings.json"),
        read(".claude/settings.local.json"),
    );
    let provider = openai::OpenAiCompatible::new("custom", &url, None);
    let env = ChildEnv::from_current();
    let mut refused = 0;
    let result = provider.run(
        &RunRequest {
            dir: &dir,
            prompt: "Add notes.",
            model: "test-model",
            resume: None,
            env: &env,
            system: None,
        },
        &mut |out| {
            if let Output::Event(Event::ToolResult { error: true, .. }) = out {
                refused += 1;
            }
        },
        &Cancel::new(),
    );
    assert!(result.ok, "{result:?}");
    assert_eq!(refused, 5, "every write of CLI settings is refused");
    assert_eq!(
        (
            read(".claude/settings.json"),
            read(".claude/settings.local.json")
        ),
        before
    );
    assert!(!dir.join(".codex").exists());
    assert!(!dir.join(".mcp.json").exists());
    assert!(!dir.join(".github").exists());
    assert!(
        dir.join("notes.txt").exists(),
        "ordinary files stay writable"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Live check with the installed Claude Code (spends two small requests of the signed-in user):
/// `cargo test -p vf-providers --test attacks -- --ignored`.
#[test]
#[ignore = "needs a signed-in Claude Code CLI"]
fn claude_code_runs_no_project_hook_in_a_task_or_a_question() {
    let provider = claude::Claude::detect();
    assert!(!provider.models().is_empty(), "Claude Code is not set up");
    let dir = hostile_project("claude");
    let env = ChildEnv::from_current();
    let result = provider.run(
        &RunRequest {
            dir: &dir,
            prompt: "Read README.md and reply with one word: done.",
            model: "haiku",
            resume: None,
            env: &env,
            system: None,
        },
        &mut |_| {},
        &Cancel::new(),
    );
    assert!(result.ok, "the task ran: {result:?}");
    let answer = provider
        .ask(
            &AskRequest {
                dir: &dir,
                system: None,
                prompt: "What does README.md say? One sentence.",
                model: "haiku",
                env: &env,
                max_tokens: None,
                timeout: Duration::from_secs(120),
            },
            &Cancel::new(),
        )
        .unwrap();
    assert!(!answer.text.trim().is_empty());
    assert!(!dir.join("marker.txt").exists(), "a project hook ran");
    let _ = std::fs::remove_dir_all(&dir);
}
