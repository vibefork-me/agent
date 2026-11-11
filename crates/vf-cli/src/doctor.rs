//! `vibefork doctor` — what works on this machine and what to fix.

use std::process::ExitCode;

use vf_platform::{Settings, access, install, secrets};
use vf_providers::Provider as _;
use vf_providers::claude::Claude;
use vf_providers::codex::Codex;
use vf_providers::copilot::Copilot;
use vf_providers::process::find_program;
use vf_toolchains::Mise;

/// Ports after the default one the agent may take when it is busy (`vf_server::FALLBACK_PORTS`).
const FALLBACK_PORTS: u16 = 4;

/// Ports the agent may listen on (the default one and the next ones) where another program listens:
/// every port that accepts a connection except the agent's own when it is running.
fn foreign_ports(agent_port: u16, agent_running: bool) -> Vec<u16> {
    use std::net::{SocketAddr, TcpStream};
    let first = vf_platform::settings::DEFAULT_PORT;
    (first..=first.saturating_add(FALLBACK_PORTS))
        .filter(|&port| !(agent_running && port == agent_port))
        .filter(|&port| {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300)).is_ok()
        })
        .collect()
}

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
    let foreign = foreign_ports(s.port, alive.is_some());
    add(
        "agent",
        alive.is_some(),
        alive
            .map(|a| format!("running, version {}, port {}", a.version, s.port))
            .unwrap_or_else(|| "not running — `vibefork start`".into()),
        false,
    );
    add(
        "ports",
        foreign.is_empty(),
        if foreign.is_empty() {
            "no other program on the agent's ports".into()
        } else {
            format!(
                "port {} is used by another program; the editor checks the agent's signature and does not talk to it",
                foreign
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
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
    // The agent makes its folders owner-only on start; a missing folder is made then too.
    let open: Vec<String> = s
        .dirs()
        .into_iter()
        .filter(|d| d.exists() && !access::is_owner_only(d).unwrap_or(false))
        .map(|d| d.display().to_string())
        .collect();
    add(
        "folders",
        open.is_empty(),
        if open.is_empty() {
            "only you can open the agent's folders".into()
        } else {
            format!(
                "other users of this computer can open {} — restart the agent (`vibefork stop`, `vibefork start`)",
                open.join(", ")
            )
        },
        false,
    );
    let kind = install::current_kind();
    add("installed as", true, kind.name().to_owned(), false);
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
