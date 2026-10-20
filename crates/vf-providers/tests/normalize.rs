//! Reference tests: recorded output of the real CLIs (`tests/fixtures`) gives the expected events.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use serde_json::Value;
use vf_protocol::Event;
use vf_providers::{Tracker, claude, codex};

fn events(file: &str, normalize: fn(&Value) -> Vec<Event>) -> Vec<Event> {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .flat_map(|v| normalize(&v))
        .collect()
}

fn kinds(evs: &[Event]) -> Vec<&'static str> {
    evs.iter()
        .map(|e| match e {
            Event::Init { .. } => "init",
            Event::Text { .. } => "text",
            Event::Tool { .. } => "tool",
            Event::ToolResult { .. } => "tool_result",
            Event::Result { .. } => "result",
            Event::Error { .. } => "error",
        })
        .collect()
}

#[test]
fn claude_stream() {
    let evs = events("claude.jsonl", claude::normalize);
    assert_eq!(
        kinds(&evs),
        [
            "init",
            "text",
            "tool",
            "tool_result",
            "tool",
            "tool_result",
            "tool",
            "tool_result",
            "text",
            "result"
        ]
    );
    assert!(
        matches!(&evs[0], Event::Init { session: Some(s), model: Some(m), cli, mcp: Some(0), skills: Some(0) }
        if s.starts_with("18c2b782") && m.starts_with("claude-haiku") && cli == "Claude Code 2.1.234")
    );
    assert!(
        matches!(&evs[2], Event::Tool { tool, detail } if tool == "Read" && detail.ends_with("app.py"))
    );
    assert!(
        matches!(&evs[6], Event::Tool { tool, detail } if tool == "PowerShell" && detail == "python app.py")
    );
    assert!(matches!(&evs[7], Event::ToolResult { error: false, text } if text.contains("bye")));
    let Event::Result {
        ok,
        turns,
        cost_usd,
        usage,
        ..
    } = &evs[9]
    else {
        panic!()
    };
    assert!(*ok && *turns == Some(4) && cost_usd.is_some_and(|c| c > 0.0));
    assert_eq!(
        usage.as_ref().map(|u| (u.input_tokens, u.output_tokens)),
        Some((33 + 21830 + 64065, 569))
    );

    let mut t = Tracker::new(None);
    evs.iter().for_each(|e| t.observe(e));
    let r = t.finish(0, false);
    assert!(
        r.ok && r.session.is_some_and(|s| s.starts_with("18c2b782")) && r.summary.contains("bye")
    );
}

#[test]
fn codex_stream() {
    let evs = events("codex.jsonl", codex::normalize);
    assert_eq!(
        kinds(&evs),
        [
            "init",
            "text",
            "tool",
            "tool_result",
            "tool",
            "tool_result",
            "text",
            "result"
        ]
    );
    assert!(
        matches!(&evs[0], Event::Init { session: Some(s), cli, .. } if s.starts_with("01a15671") && cli == "Codex CLI")
    );
    assert!(
        matches!(&evs[3], Event::ToolResult { error: false, text } if text.contains("print('hi')"))
    );
    let Event::Result {
        ok: true,
        usage: Some(u),
        ..
    } = &evs[7]
    else {
        panic!()
    };
    assert_eq!((u.input_tokens, u.output_tokens), (40853, 200));

    let mut t = Tracker::new(None);
    evs.iter().for_each(|e| t.observe(e));
    let r = t.finish(0, false);
    assert!(r.ok && r.summary.contains("bye"), "{r:?}");
    assert!(!t_failed_on_exit_code());
}

fn t_failed_on_exit_code() -> bool {
    let mut t = Tracker::new(Some("old"));
    t.observe(&Event::Result {
        ok: true,
        text: String::new(),
        turns: None,
        ms: None,
        cost_usd: None,
        usage: None,
    });
    let r = t.finish(1, false);
    assert_eq!(r.session.as_deref(), Some("old"), "resumed session is kept");
    r.ok
}

#[test]
fn codex_models_cache_lists_visible_models_and_default() {
    let json = r#"{"models":[
        {"slug":"gpt-hidden","visibility":"hide","description":"x"},
        {"slug":"gpt-luna","visibility":"list","description":"Fast and affordable model for easier tasks."},
        {"slug":"gpt-terra","visibility":"list","description":"Older balanced model."},
        {"slug":"codex-auto-review","visibility":"list","description":"review"}]}"#;
    let m: Vec<(String, u8)> = codex::models_from_cache(json)
        .into_iter()
        .map(|m| (m.model, m.tier))
        .collect();
    assert_eq!(
        m,
        [
            ("gpt-luna".into(), 1),
            ("gpt-terra".into(), 2),
            (String::new(), 3)
        ]
    );
}

#[test]
fn cli_arguments_keep_the_prompt_out_and_the_personal_environment_off() {
    let a = claude::args("haiku", Some("s1"));
    for flag in [
        "--setting-sources",
        "project,local",
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--resume",
        "s1",
        "--model",
        "haiku",
    ] {
        assert!(a.iter().any(|x| x == flag), "{flag}");
    }
    let c = codex::args(std::path::Path::new("/p"), "", Some("t1"));
    assert_eq!(&c[..2], ["exec", "resume"]);
    assert_eq!(
        &c[c.len() - 2..],
        ["t1", "-"],
        "session id, then the prompt from stdin"
    );
    assert!(
        !c.iter().any(|x| x == "-s" || x == "-C"),
        "not accepted by exec resume"
    );
    let fresh = codex::args(std::path::Path::new("/p"), "m", None);
    assert_eq!(
        &fresh[..5],
        ["exec", "-s", "danger-full-access", "-C", "/p"]
    );
    assert!(
        c.iter().any(|x| x == "--ignore-user-config") && c.iter().any(|x| x == "--ignore-rules")
    );
    assert_eq!(c.last().map(String::as_str), Some("-"), "prompt from stdin");
}
