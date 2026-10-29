//! `vibefork doctor` — what works on this machine and what to fix.

use std::process::ExitCode;

use vf_platform::{Settings, install, secrets};
use vf_providers::Provider as _;
use vf_providers::claude::Claude;
use vf_providers::codex::Codex;
use vf_providers::copilot::Copilot;
use vf_providers::process::find_program;
use vf_toolchains::Mise;

struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
    /// Needed for the agent to work at all.
    required: bool,
}

pub(crate) fn run(json: bool) -> ExitCode {
    let s = Settings::from_env();
    let mut checks = Vec::new();
    let mut add = |name, ok, detail: String, required| {
        checks.push(Check {
            name,
            ok,
            detail,
            required,
        })
    };

    let git = find_program("git");
    add(
        "git",
        git.is_some(),
        git.map(|p| p.display().to_string())
            .unwrap_or_else(|| "not found — install Git".into()),
        true,
    );
    let alive = crate::agent::hello(s.port);
    add(
        "agent",
        alive.is_some(),
        alive
            .map(|a| format!("running, version {}, port {}", a.version, s.port))
            .unwrap_or_else(|| "not running — `vibefork start`".into()),
        false,
    );
    let claude = Claude::detect()
        .with_home(s.storage.tools.join("cli-home").join("claude"))
        .with_mode(s.cli_home_mode("claude"))
        .models();
    add(
        "claude",
        !claude.is_empty(),
        if claude.is_empty() {
            "Claude Code CLI not found".into()
        } else {
            "Claude Code CLI found".into()
        },
        false,
    );
    let codex = Codex::detect()
        .with_home(s.storage.tools.join("cli-home").join("codex"))
        .with_mode(s.cli_home_mode("codex"))
        .models();
    add(
        "codex",
        !codex.is_empty(),
        if codex.is_empty() {
            "Codex CLI not found or not signed in".into()
        } else {
            format!("signed in, {} models", codex.len())
        },
        false,
    );
    let copilot = Copilot::detect()
        .with_home(s.storage.tools.join("cli-home").join("copilot"))
        .with_mode(s.cli_home_mode("copilot"))
        .models();
    add(
        "copilot",
        !copilot.is_empty(),
        if copilot.is_empty() {
            "GitHub Copilot CLI not found or not signed in".into()
        } else {
            "signed in".into()
        },
        false,
    );
    let or_key = secrets::openrouter_key().is_some();
    add(
        "openrouter",
        or_key,
        if or_key {
            "key set".into()
        } else {
            "no key — `vibefork keys set openrouter`".into()
        },
        false,
    );
    let any_model = !claude.is_empty() || !codex.is_empty() || !copilot.is_empty() || or_key;
    add(
        "models",
        any_model,
        if any_model {
            "at least one provider".into()
        } else {
            "no provider — install Claude Code, Codex or GitHub Copilot CLI, or set an OpenRouter key".into()
        },
        true,
    );
    let mise = Mise::new(&s.storage.tools);
    add(
        "mise",
        mise.is_installed(),
        if mise.is_installed() {
            mise.bin().display().to_string()
        } else {
            "downloaded on first need".into()
        },
        false,
    );
    let free = vf_builds::Machine::measure(&s.state_dir).disk_free;
    add(
        "disk",
        free > 5 * 1024 * 1024 * 1024,
        format!(
            "{:.1} GB free for the agent's folders",
            free as f64 / 1073741824.0
        ),
        false,
    );
    let handler = install::protocol_target();
    add(
        "vibefork://",
        handler.is_some(),
        handler.unwrap_or_else(|| "not registered — `vibefork install`".into()),
        false,
    );

    if json {
        let v: Vec<_> = checks
            .iter()
            .map(|c| serde_json::json!({ "check": c.name, "ok": c.ok, "detail": c.detail }))
            .collect();
        println!("{}", serde_json::Value::Array(v));
    } else {
        for c in &checks {
            println!(
                "{} {:<12} {}",
                if c.ok {
                    "✓"
                } else if c.required {
                    "✗"
                } else {
                    "·"
                },
                c.name,
                c.detail
            );
        }
    }
    if checks.iter().all(|c| c.ok || !c.required) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
