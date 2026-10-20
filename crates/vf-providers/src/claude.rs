//! Claude Code CLI (the user's subscription).
//!
//! Runs `claude -p` with `stream-json` output. No restrictions on what the model may do in the
//! project, but none of the user's personal environment: only project / local settings, no MCP
//! servers, no slash commands (invariant 7).

use std::path::PathBuf;

use serde_json::Value;
use vf_protocol::{Cancel, CostClass, Event, ModelInfo, Usage};

use crate::cli::{collect, run_cli};
use crate::process;
use crate::{Answer, AskRequest, Output, Provider, RunRequest, RunResult, clip};

/// The Claude Code CLI.
#[derive(Debug, Clone)]
pub struct Claude {
    bin: Option<PathBuf>,
}

impl Claude {
    /// Finds the CLI: `VF_CLAUDE_BIN`, else `claude` on `PATH`.
    pub fn detect() -> Self {
        let bin = std::env::var_os("VF_CLAUDE_BIN")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| process::find_program("claude"));
        Self { bin }
    }
}

/// Command-line arguments (the prompt goes through stdin).
pub fn args(model: &str, resume: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode",
        "acceptEdits",
        "--setting-sources",
        "project,local",
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--allowedTools",
        "Bash",
        "PowerShell",
    ]
    .map(str::to_owned)
    .to_vec();
    if let Some(r) = resume {
        a.extend(["--resume".into(), r.into()]);
    }
    if !model.is_empty() {
        a.extend(["--model".into(), model.into()]);
    }
    a
}

/// Arguments of a read-only question: only the read tools, one JSON result.
pub fn ask_args(model: &str) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--output-format",
        "json",
        "--tools",
        "Read,Glob,Grep",
        "--setting-sources",
        "project,local",
        "--strict-mcp-config",
        "--disable-slash-commands",
    ]
    .map(str::to_owned)
    .to_vec();
    if !model.is_empty() {
        a.extend(["--model".into(), model.into()]);
    }
    a
}

/// Normalizes one `stream-json` line.
pub fn normalize(e: &Value) -> Vec<Event> {
    let mut out = Vec::new();
    let s = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    match e["type"].as_str() {
        Some("system") if e["subtype"] == "init" => out.push(Event::Init {
            session: e["session_id"].as_str().map(str::to_owned),
            model: e["model"].as_str().map(str::to_owned),
            cli: format!("Claude Code {}", s(&e["claude_code_version"])),
            mcp: e["mcp_servers"].as_array().map(|a| a.len() as u32),
            skills: e["skills"].as_array().map(|a| a.len() as u32),
        }),
        Some("assistant") => {
            for c in e["message"]["content"].as_array().into_iter().flatten() {
                match c["type"].as_str() {
                    Some("text") if !s(&c["text"]).trim().is_empty() => out.push(Event::Text {
                        text: s(&c["text"]),
                    }),
                    Some("tool_use") => {
                        let input = &c["input"];
                        let detail = ["command", "file_path", "pattern"]
                            .iter()
                            .find_map(|k| input[*k].as_str())
                            .map(str::to_owned)
                            .unwrap_or_else(|| clip(&input.to_string(), 200));
                        out.push(Event::Tool {
                            tool: s(&c["name"]),
                            detail,
                        });
                    }
                    _ => {}
                }
            }
        }
        Some("user") => {
            for c in e["message"]["content"].as_array().into_iter().flatten() {
                if c["type"] == "tool_result" {
                    let text = match &c["content"] {
                        Value::Array(parts) => parts
                            .iter()
                            .map(|p| s(&p["text"]))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        Value::String(t) => t.clone(),
                        _ => String::new(),
                    };
                    out.push(Event::ToolResult {
                        error: c["is_error"].as_bool().unwrap_or(false),
                        text: clip(&text, 2000),
                    });
                }
            }
        }
        Some("result") => {
            let u = &e["usage"];
            let usage = u.is_object().then(|| Usage {
                input_tokens: [
                    "input_tokens",
                    "cache_creation_input_tokens",
                    "cache_read_input_tokens",
                ]
                .iter()
                .map(|k| u[*k].as_u64().unwrap_or(0))
                .sum(),
                output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
            });
            out.push(Event::Result {
                ok: !e["is_error"].as_bool().unwrap_or(false),
                text: s(&e["result"]),
                turns: e["num_turns"].as_u64().map(|n| n as u32),
                ms: e["duration_ms"].as_u64(),
                cost_usd: e["total_cost_usd"].as_f64(),
                usage,
            });
        }
        _ => {}
    }
    out
}

impl Provider for Claude {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn models(&self) -> Vec<ModelInfo> {
        if self.bin.is_none() {
            return Vec::new();
        }
        [("haiku", 1), ("sonnet", 2), ("opus", 3)]
            .map(|(m, tier)| ModelInfo {
                provider: "claude".into(),
                model: m.into(),
                tier,
                cost: CostClass::Subscription,
                label: format!("Claude {m}"),
            })
            .to_vec()
    }

    fn can_resume(&self) -> bool {
        true
    }

    fn ask(&self, req: &AskRequest<'_>, cancel: &Cancel) -> Result<Answer, String> {
        let stdout = collect(
            self.bin.as_deref(),
            "claude",
            &ask_args(req.model),
            req,
            cancel,
        )?;
        let v: Value = serde_json::from_str(stdout.trim()).map_err(|e| format!("claude: {e}"))?;
        if v["is_error"].as_bool().unwrap_or(false) {
            return Err(format!(
                "claude: {}",
                clip(v["result"].as_str().unwrap_or("error"), 300)
            ));
        }
        Ok(Answer {
            text: v["result"].as_str().unwrap_or_default().to_owned(),
            cost_usd: v["total_cost_usd"].as_f64(),
        })
    }

    fn run(&self, req: &RunRequest<'_>, out: &mut dyn FnMut(Output), cancel: &Cancel) -> RunResult {
        run_cli(
            self.bin.as_deref(),
            "claude",
            args(req.model, req.resume),
            req,
            out,
            cancel,
            normalize,
        )
    }
}
