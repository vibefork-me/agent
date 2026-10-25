//! Claude Code CLI (the user's subscription).
//!
//! Runs `claude -p` with `stream-json` output. No restrictions on what the model may do in the
//! project, but none of the user's personal environment: only project / local settings, no MCP
//! servers, no slash commands (invariant 7).

use std::path::{Path, PathBuf};

use serde_json::Value;
use vf_protocol::{Cancel, CostClass, Event, ModelInfo, Usage};

use crate::cli::{collect, run_cli};
use crate::process;
use crate::{Answer, AskRequest, ChildEnv, Output, Provider, RunRequest, RunResult, clip};

/// Where Claude Code keeps its sign-in, settings and sessions.
const HOME_VAR: &str = "CLAUDE_CONFIG_DIR";

/// The Claude Code CLI.
#[derive(Debug)]
pub struct Claude {
    bin: std::sync::RwLock<Option<PathBuf>>,
    /// When the sign-in was last checked, and its result.
    signed_in: std::sync::Mutex<Option<(std::time::Instant, bool)>>,
    /// `CLAUDE_CONFIG_DIR`: sign-in, settings and sessions; `None` — the user's `~/.claude`.
    home: Option<PathBuf>,
}

impl Claude {
    fn bin(&self) -> Option<PathBuf> {
        self.bin.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Finds the CLI: `VF_CLAUDE_BIN`, else `claude` on `PATH`.
    pub fn detect() -> Self {
        let bin = std::env::var_os("VF_CLAUDE_BIN")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| process::find_program("claude"));
        Self {
            bin: std::sync::RwLock::new(bin),
            signed_in: std::sync::Mutex::new(None),
            home: None,
        }
    }

    /// Keeps the CLI's sign-in, settings and sessions in `home` instead of `~/.claude`.
    pub fn with_home(mut self, home: PathBuf) -> Self {
        self.home = Some(home);
        self
    }

    fn command(&self, bin: &Path) -> std::process::Command {
        let mut cmd = process::command(bin);
        if let Some(home) = &self.home {
            let _ = std::fs::create_dir_all(home);
            cmd.env(HOME_VAR, home);
        }
        cmd
    }

    fn env(&self, env: &ChildEnv) -> ChildEnv {
        let mut env = env.clone();
        if let Some(home) = &self.home {
            let _ = std::fs::create_dir_all(home);
            env.set(HOME_VAR, home.as_os_str());
        }
        env
    }

    /// Whether the user is signed in (`claude auth status`), cached for a minute.
    fn is_signed_in(&self) -> bool {
        let Some(bin) = self.bin() else {
            return false;
        };
        let mut cache = self.signed_in.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, ok)) = *cache
            && at.elapsed() < std::time::Duration::from_secs(60)
        {
            return ok;
        }
        let mut out = String::new();
        let mut cmd = self.command(&bin);
        cmd.args(["auth", "status", "--json"]);
        let exit = process::run_streaming(
            cmd,
            None,
            &Cancel::new(),
            Some(std::time::Duration::from_secs(20)),
            &mut |stream, l| {
                if matches!(stream, process::Stream::Stdout) {
                    out.push_str(&l);
                }
            },
        );
        let ok = exit.is_ok_and(|e| e.code == 0) && signed_in_from_status(&out);
        *cache = Some((std::time::Instant::now(), ok));
        ok
    }
}

/// `loggedIn` of `claude auth status --json`.
pub fn signed_in_from_status(json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v["loggedIn"].as_bool())
        .unwrap_or(false)
}

/// Command-line arguments (the prompt goes through stdin).
pub fn args(model: &str, resume: Option<&str>, system: Option<&str>) -> Vec<String> {
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
    if let Some(sys) = system.filter(|s| !s.trim().is_empty()) {
        a.extend(["--append-system-prompt".into(), sys.to_owned()]);
    }
    a
}

/// Arguments of a read-only question: only the read tools, one JSON result.
pub fn ask_args(model: &str, system: Option<&str>) -> Vec<String> {
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
    if let Some(sys) = system.filter(|s| !s.trim().is_empty()) {
        a.extend(["--append-system-prompt".into(), sys.to_owned()]);
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
        if !self.is_signed_in() {
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

    fn use_cli(&self, bin: PathBuf) {
        *self.bin.write().unwrap_or_else(|p| p.into_inner()) = Some(bin);
    }

    fn status(&self) -> crate::Status {
        crate::Status {
            installed: self.bin().is_some(),
            signed_in: Some(self.is_signed_in()),
        }
    }

    fn connect(&self, timeout: std::time::Duration) -> Result<(), String> {
        let bin = self.bin().ok_or("Claude Code is not installed")?;
        let mut cmd = self.command(&bin);
        cmd.args(["auth", "login"]);
        let exit = process::run_streaming(cmd, None, &Cancel::new(), Some(timeout), &mut |_, _| {})
            .map_err(|e| e.to_string())?;
        *self.signed_in.lock().unwrap_or_else(|p| p.into_inner()) = None;
        if exit.code == 0 && self.is_signed_in() {
            Ok(())
        } else {
            Err(format!("sign-in did not finish (code {})", exit.code))
        }
    }

    fn ask(&self, req: &AskRequest<'_>, cancel: &Cancel) -> Result<Answer, String> {
        let env = self.env(req.env);
        let req = &AskRequest { env: &env, ..*req };
        let stdout = collect(
            self.bin().as_deref(),
            "claude",
            &ask_args(req.model, req.system),
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
        let env = self.env(req.env);
        let req = &RunRequest { env: &env, ..*req };
        run_cli(
            self.bin().as_deref(),
            "claude",
            args(req.model, req.resume, req.system),
            req,
            out,
            cancel,
            normalize,
        )
    }
}

#[cfg(test)]
mod status_tests {
    use super::signed_in_from_status;

    #[test]
    fn auth_status_json_says_whether_signed_in() {
        assert!(signed_in_from_status(
            r#"{"loggedIn": true, "authMethod": "claude.ai"}"#
        ));
        assert!(!signed_in_from_status(
            r#"{"loggedIn": false, "authMethod": "none"}"#
        ));
        assert!(!signed_in_from_status("not json"));
        assert!(!signed_in_from_status(r#"{"loggedIn": "yes"}"#));
    }
}

#[cfg(test)]
mod home_tests {
    use super::*;

    #[test]
    fn the_cli_home_reaches_every_process() {
        let home = std::env::temp_dir().join(format!("vf-claude-home-{}", std::process::id()));
        let claude = Claude::detect().with_home(home.clone());
        let env = claude.env(&ChildEnv::from_vars([]));
        assert_eq!(env.get(HOME_VAR), Some(home.as_os_str()));
        let cmd = claude.command(Path::new("claude"));
        assert!(
            cmd.get_envs()
                .any(|(k, v)| k == HOME_VAR && v == Some(home.as_os_str()))
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn without_a_home_the_environment_is_unchanged() {
        let env = Claude::detect().env(&ChildEnv::from_vars([]));
        assert_eq!(env.get(HOME_VAR), None);
    }
}
