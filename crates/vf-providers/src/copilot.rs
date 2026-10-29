//! GitHub Copilot CLI (the user's Copilot plan, Copilot Free included): `copilot --output-format json`.
//!
//! None of the user's personal environment: no built-in GitHub MCP server, no automatic updates of
//! the CLI, no account history sync, no client telemetry; project instructions are read, as with
//! Claude Code. Full access to the project folder (`--allow-all-tools`); the CLI still checks file
//! paths against the folder, and isolation is the agent's job. The prompt goes through stdin; the
//! CLI has no system prompt flag, so a system instruction is put before the prompt.
//!
//! The sign-in is checked over the CLI's own JSON-RPC server (`auth.getStatus`), which spends no
//! model request. In agent mode only the CLI's own sign-in in `COPILOT_HOME` counts: the CLI also
//! falls back to tokens in the environment and to the GitHub CLI (`gh`) sign-in, and the agent
//! does not use either for the user without asking.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use vf_protocol::{Cancel, CostClass, Event, ModelInfo};

use crate::cli::{collect, run_cli};
use crate::process::{self, Stream};
use crate::{Answer, AskRequest, ChildEnv, Output, Provider, RunRequest, RunResult, clip};

/// Where the CLI keeps its sign-in, settings and sessions.
const HOME_VAR: &str = "COPILOT_HOME";

/// Tokens the CLI takes over its stored sign-in; never given to it in agent mode.
const TOKEN_VARS: &[&str] = &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"];

/// How long the sign-in status is cached.
const CACHE_TTL: Duration = Duration::from_secs(60);

/// Longest answer of the CLI's JSON-RPC server that is read.
const STATUS_MAX_BYTES: usize = 64 * 1024;

/// The GitHub Copilot CLI.
#[derive(Debug)]
pub struct Copilot {
    bin: std::sync::RwLock<Option<PathBuf>>,
    signed_in_cache: Mutex<Option<(Instant, bool)>>,
    user_cache: Mutex<Option<(Instant, bool)>>,
    /// `COPILOT_HOME`: sign-in, settings and sessions in agent mode.
    agent_home: Option<PathBuf>,
    /// Mode: agent or user.
    mode: std::sync::RwLock<vf_protocol::CliHomeMode>,
}

/// What `auth.getStatus` says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthStatus {
    /// The CLI has a credential.
    pub authenticated: bool,
    /// Where it comes from: `user` (its own sign-in), `env`, `gh-cli`, …
    pub kind: String,
}

impl AuthStatus {
    /// Whether this sign-in may be used: in agent mode only the CLI's own sign-in, in user mode
    /// also the user's GitHub CLI sign-in.
    pub fn usable(&self, mode: vf_protocol::CliHomeMode) -> bool {
        self.authenticated
            && match mode {
                vf_protocol::CliHomeMode::Agent => self.kind == "user",
                vf_protocol::CliHomeMode::User => matches!(self.kind.as_str(), "user" | "gh-cli"),
            }
    }
}

impl Copilot {
    fn bin(&self) -> Option<PathBuf> {
        self.bin.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Finds the CLI: `VF_COPILOT_BIN`, else `copilot` on `PATH`.
    pub fn detect() -> Self {
        let bin = std::env::var_os("VF_COPILOT_BIN")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| process::find_program("copilot"));
        Self {
            bin: std::sync::RwLock::new(bin),
            signed_in_cache: Mutex::new(None),
            user_cache: Mutex::new(None),
            agent_home: None,
            mode: std::sync::RwLock::new(vf_protocol::CliHomeMode::Agent),
        }
    }

    /// Keeps the CLI's sign-in, settings and sessions in `home` in agent mode instead of
    /// `~/.copilot`.
    pub fn with_home(mut self, home: PathBuf) -> Self {
        self.agent_home = Some(home);
        self
    }

    /// Sets the CLI home mode (agent or user).
    pub fn with_mode(self, mode: vf_protocol::CliHomeMode) -> Self {
        *self.mode.write().unwrap_or_else(|p| p.into_inner()) = mode;
        self
    }

    fn mode(&self) -> vf_protocol::CliHomeMode {
        *self.mode.read().unwrap_or_else(|p| p.into_inner())
    }

    fn active_home(&self) -> Option<PathBuf> {
        match self.mode() {
            vf_protocol::CliHomeMode::Agent => self.agent_home.clone(),
            vf_protocol::CliHomeMode::User => None,
        }
    }

    /// A command of the CLI in the current mode (for sign-in and status).
    fn command(&self, bin: &Path) -> std::process::Command {
        let mut cmd = process::command(bin);
        match self.active_home() {
            Some(home) => {
                let _ = std::fs::create_dir_all(&home);
                cmd.env(HOME_VAR, home);
                for v in TOKEN_VARS {
                    cmd.env_remove(v);
                }
            }
            None => {
                cmd.env_remove(HOME_VAR);
            }
        }
        cmd
    }

    fn env(&self, env: &ChildEnv) -> ChildEnv {
        let mut env = env.clone();
        match self.active_home() {
            Some(home) => {
                let _ = std::fs::create_dir_all(&home);
                env.set(HOME_VAR, home.as_os_str());
                for v in TOKEN_VARS {
                    env.remove(std::ffi::OsStr::new(v));
                }
            }
            None => env.remove(std::ffi::OsStr::new(HOME_VAR)),
        }
        env
    }

    fn logged_in(&self, bin: &Path) -> bool {
        let mut cache = self
            .signed_in_cache
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some((at, ok)) = *cache
            && at.elapsed() < CACHE_TTL
        {
            return ok;
        }
        let ok = auth_status(self.command(bin)).is_some_and(|s| s.usable(self.mode()));
        *cache = Some((Instant::now(), ok));
        ok
    }

    /// Whether the user is signed in to their own CLI (`~/.copilot`, without `COPILOT_HOME`).
    fn user_logged_in(&self, bin: &Path) -> bool {
        let mut cache = self.user_cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, ok)) = *cache
            && at.elapsed() < CACHE_TTL
        {
            return ok;
        }
        let mut cmd = process::command(bin);
        cmd.env_remove(HOME_VAR);
        let ok = auth_status(cmd).is_some_and(|s| s.usable(vf_protocol::CliHomeMode::User));
        *cache = Some((Instant::now(), ok));
        ok
    }
}

/// One JSON-RPC request to the CLI's server, framed as the server reads it.
pub fn rpc_request(method: &str) -> String {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": {}}).to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len())
}

/// The `result` of the answer to [`rpc_request`] in the server's output (a `Content-Length`
/// header, a blank line, the JSON body).
pub fn rpc_result(output: &str) -> Option<Value> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('{'))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["id"] == 1)
        .map(|v| v["result"].clone())
        .filter(|r| r.is_object())
}

/// The answer to `auth.getStatus`.
pub fn status_from_result(result: &Value) -> AuthStatus {
    AuthStatus {
        authenticated: result["isAuthenticated"].as_bool().unwrap_or(false),
        kind: result["authType"].as_str().unwrap_or_default().to_owned(),
    }
}

/// Runs one JSON-RPC call on the CLI's server (`--server --stdio`): the request goes to stdin, the
/// server answers and exits when stdin closes.
fn rpc(mut cmd: std::process::Command, method: &str) -> Option<Value> {
    cmd.args(["--server", "--stdio", "--no-auto-update"]);
    let mut out = String::new();
    let exit = process::run_streaming(
        cmd,
        Some(&rpc_request(method)),
        &Cancel::new(),
        Some(Duration::from_secs(20)),
        &mut |stream, line| {
            if stream == Stream::Stdout && out.len() + line.len() < STATUS_MAX_BYTES {
                out.push_str(&line);
                out.push('\n');
            }
        },
    )
    .ok()?;
    if exit.timed_out || exit.cancelled {
        return None;
    }
    rpc_result(&out)
}

fn auth_status(cmd: std::process::Command) -> Option<AuthStatus> {
    rpc(cmd, "auth.getStatus").map(|r| status_from_result(&r))
}

/// The prompt with a system instruction before it (the CLI has no flag for one).
pub fn with_system(prompt: &str, system: Option<&str>) -> String {
    match system.filter(|s| !s.trim().is_empty()) {
        Some(sys) => format!("{sys}\n\n---\n\n{prompt}"),
        None => prompt.to_owned(),
    }
}

/// Flags of every run: JSON lines, no questions back, none of the user's personal environment.
fn common_args() -> Vec<String> {
    [
        "--output-format",
        "json",
        "--no-ask-user",
        "--disable-builtin-mcps",
        "--no-auto-update",
        "--disable-client-telemetry",
        "--sync-session=off",
    ]
    .map(str::to_owned)
    .to_vec()
}

/// Command-line arguments of a run (the prompt goes through stdin).
pub fn args(model: &str, resume: Option<&str>) -> Vec<String> {
    let mut a = common_args();
    a.push("--allow-all-tools".into());
    if let Some(r) = resume {
        a.extend(["--resume".into(), r.into()]);
    }
    if !model.is_empty() {
        a.extend(["--model".into(), model.into()]);
    }
    a
}

/// Arguments of a read-only question: shell commands and file changes are denied (a denial wins
/// over every allow rule).
pub fn ask_args(model: &str) -> Vec<String> {
    let mut a = common_args();
    a.extend(["--deny-tool=shell", "--deny-tool=write"].map(str::to_owned));
    if !model.is_empty() {
        a.extend(["--model".into(), model.into()]);
    }
    a
}

/// The files a patch of `apply_patch` changes: `add notes.txt, update app.py`.
fn patch_files(patch: &str) -> String {
    patch
        .lines()
        .filter_map(|l| {
            let l = l.strip_prefix("*** ")?;
            [
                ("Add File: ", "add"),
                ("Update File: ", "update"),
                ("Delete File: ", "delete"),
            ]
            .iter()
            .find_map(|(p, kind)| l.strip_prefix(p).map(|f| format!("{kind} {}", f.trim())))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a tool call shows: shell tools by their command, file edits as `edit`.
fn tool(name: &str, args: &Value, title: &str) -> Event {
    let command = args["command"].as_str();
    match (name, command) {
        ("powershell" | "bash" | "shell", Some(c)) => Event::Tool {
            tool: "shell".into(),
            detail: c.to_owned(),
        },
        ("apply_patch" | "edit" | "create" | "write", _) => Event::Tool {
            tool: "edit".into(),
            detail: args["path"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| args.as_str().map(patch_files))
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| title.to_owned()),
        },
        _ => Event::Tool {
            tool: name.to_owned(),
            detail: title.to_owned(),
        },
    }
}

/// Normalizes one `--output-format json` line.
pub fn normalize(e: &Value) -> Vec<Event> {
    let d = &e["data"];
    let s = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    match e["type"].as_str() {
        Some("session.auto_mode_resolved") => vec![Event::Init {
            session: None,
            model: d["chosenModel"].as_str().map(str::to_owned),
            cli: "GitHub Copilot CLI".into(),
            mcp: None,
            skills: None,
        }],
        Some("assistant.message") if !s(&d["content"]).trim().is_empty() => {
            vec![Event::Text {
                text: s(&d["content"]),
            }]
        }
        Some("tool.execution_start") => vec![tool(
            d["toolName"].as_str().unwrap_or_default(),
            &d["arguments"],
            d["toolTitle"].as_str().unwrap_or_default(),
        )],
        Some("tool.execution_complete") => {
            let ok = d["success"].as_bool().unwrap_or(false);
            let text = d["result"]["content"]
                .as_str()
                .or_else(|| d["error"]["message"].as_str())
                .unwrap_or_default();
            vec![Event::ToolResult {
                error: !ok,
                text: clip(text, 2000),
            }]
        }
        Some("session.error") => vec![Event::Error {
            text: s(&d["message"]),
        }],
        Some("result") => {
            let session = e["sessionId"].as_str().map(str::to_owned);
            let mut out = Vec::new();
            if session.is_some() {
                out.push(Event::Init {
                    session,
                    model: None,
                    cli: "GitHub Copilot CLI".into(),
                    mcp: None,
                    skills: None,
                });
            }
            out.push(Event::Result {
                ok: e["exitCode"].as_i64() == Some(0),
                text: String::new(),
                turns: None,
                ms: e["usage"]["sessionDurationMs"].as_u64(),
                cost_usd: None,
                usage: None,
            });
            out
        }
        _ => Vec::new(),
    }
}

/// What the user is told when the CLI stops with `Error: …` on stderr: no access to Copilot (no
/// plan, a policy, the country) or the plan's limit is used up.
pub fn explain_error(line: &str) -> Option<String> {
    let msg = line.trim().strip_prefix("Error:")?.trim();
    let lower = msg.to_ascii_lowercase();
    let hint = if lower.contains("access denied") || lower.contains("not authorized") {
        "GitHub Copilot is not available for this GitHub account (check github.com/settings/copilot); choose another AI"
    } else if lower.contains("quota")
        || lower.contains("limit")
        || lower.contains("credits")
        || lower.contains("429")
    {
        "the GitHub Copilot limit is used up; choose another AI or wait until it renews"
    } else {
        return Some(format!("GitHub Copilot: {}", clip(msg, 300)));
    };
    Some(format!("{hint} ({})", clip(msg, 200)))
}

/// Passes a run's output on, turning the CLI's final `Error: …` line into an error event.
fn forward(out: &mut dyn FnMut(Output)) -> impl FnMut(Output) + '_ {
    move |o| match o {
        Output::Stderr(line) => match explain_error(&line) {
            Some(text) => out(Output::Event(Event::Error { text })),
            None => out(Output::Stderr(line)),
        },
        other => out(other),
    }
}

impl Provider for Copilot {
    fn id(&self) -> &'static str {
        "copilot"
    }

    fn logout(&self) -> Result<(), String> {
        // Only the agent's own folder: without it the user would be signed out of their CLI.
        if self.active_home().is_none() {
            return Err("the user's own sign-in is not signed out by the agent".into());
        }
        let bin = self.bin().ok_or("GitHub Copilot CLI is not installed")?;
        let done = rpc(self.command(&bin), "account.logout");
        self.reload();
        match done {
            Some(_) if !self.logged_in(&bin) => Ok(()),
            _ => Err("sign-out did not finish".into()),
        }
    }

    fn connect(&self, timeout: Duration) -> Result<(), String> {
        let bin = self.bin().ok_or("GitHub Copilot CLI is not installed")?;
        let mut cmd = self.command(&bin);
        cmd.args(["login", "--no-auto-update"]);
        let exit = process::run_streaming(cmd, None, &Cancel::new(), Some(timeout), &mut |_, _| {})
            .map_err(|e| e.to_string())?;
        self.reload();
        if exit.code == 0 && self.logged_in(&bin) {
            Ok(())
        } else {
            Err(format!("sign-in did not finish (code {})", exit.code))
        }
    }

    fn use_cli(&self, bin: PathBuf) {
        *self.bin.write().unwrap_or_else(|p| p.into_inner()) = Some(bin);
    }

    fn status(&self) -> crate::Status {
        let bin = self.bin();
        crate::Status {
            installed: bin.is_some(),
            signed_in: Some(bin.as_deref().is_some_and(|b| self.logged_in(b))),
            user_signed_in: Some(bin.as_deref().is_some_and(|b| self.user_logged_in(b))),
            cli_home_mode: Some(self.mode()),
        }
    }

    fn reload(&self) {
        *self
            .signed_in_cache
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        *self.user_cache.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    fn set_cli_home_mode(&self, mode: vf_protocol::CliHomeMode) {
        *self.mode.write().unwrap_or_else(|p| p.into_inner()) = mode;
        self.reload();
    }

    fn cli_home_mode(&self) -> Option<vf_protocol::CliHomeMode> {
        Some(self.mode())
    }

    /// One model: Copilot picks it itself (Auto). Copilot Free always uses Auto, whatever is asked.
    fn models(&self) -> Vec<ModelInfo> {
        match self.bin() {
            Some(bin) if self.logged_in(&bin) => vec![ModelInfo {
                provider: "copilot".into(),
                model: String::new(),
                tier: 2,
                cost: CostClass::Subscription,
                label: "GitHub Copilot (Auto)".into(),
            }],
            _ => Vec::new(),
        }
    }

    fn can_resume(&self) -> bool {
        true
    }

    fn ask(&self, req: &AskRequest<'_>, cancel: &Cancel) -> Result<Answer, String> {
        let env = self.env(req.env);
        let prompt = with_system(req.prompt, req.system);
        let req = &AskRequest {
            env: &env,
            prompt: &prompt,
            ..*req
        };
        let stdout = collect(
            self.bin().as_deref(),
            "copilot",
            &ask_args(req.model),
            req,
            cancel,
        )
        .map_err(|e| {
            e.split_once("Error:")
                .and_then(|(_, rest)| explain_error(&format!("Error:{rest}")))
                .unwrap_or(e)
        })?;
        let events: Vec<Value> = stdout
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .collect();
        let text = events
            .iter()
            .filter(|e| e["type"] == "assistant.message")
            .filter_map(|e| e["data"]["content"].as_str())
            .filter(|t| !t.trim().is_empty())
            .map(str::to_owned)
            .next_back()
            .ok_or_else(|| no_answer(&events))?;
        Ok(Answer {
            text,
            cost_usd: None,
        })
    }

    fn run(&self, req: &RunRequest<'_>, out: &mut dyn FnMut(Output), cancel: &Cancel) -> RunResult {
        let env = self.env(req.env);
        let prompt = with_system(req.prompt, req.system);
        let req = &RunRequest {
            env: &env,
            prompt: &prompt,
            ..*req
        };
        run_cli(
            self.bin().as_deref(),
            "copilot",
            args(req.model, req.resume),
            req,
            &mut forward(out),
            cancel,
            normalize,
        )
    }
}

/// Why the CLI gave no answer: the last error it reported, if any.
fn no_answer(events: &[Value]) -> String {
    events
        .iter()
        .rev()
        .find(|e| e["type"] == "session.error")
        .and_then(|e| e["data"]["message"].as_str())
        .map_or_else(
            || "copilot gave no answer".to_owned(),
            |m| format!("copilot gave no answer: {}", clip(m, 300)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use vf_protocol::CliHomeMode;

    #[test]
    fn only_the_clis_own_sign_in_counts_in_agent_mode() {
        let st = |kind: &str| AuthStatus {
            authenticated: true,
            kind: kind.into(),
        };
        assert!(st("user").usable(CliHomeMode::Agent));
        assert!(!st("gh-cli").usable(CliHomeMode::Agent));
        assert!(!st("env").usable(CliHomeMode::Agent));
        assert!(st("gh-cli").usable(CliHomeMode::User));
        assert!(!st("env").usable(CliHomeMode::User));
        let none = AuthStatus {
            authenticated: false,
            kind: "user".into(),
        };
        assert!(!none.usable(CliHomeMode::Agent));
    }

    #[test]
    fn the_status_answer_is_read_from_the_framed_output() {
        let req = rpc_request("auth.getStatus");
        assert!(req.starts_with("Content-Length: "));
        let body = req.split("\r\n\r\n").nth(1).unwrap();
        assert_eq!(req, format!("Content-Length: {}\r\n\r\n{body}", body.len()));

        let out = "Content-Length: 120\n\n{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"isAuthenticated\":true,\"authType\":\"user\",\"login\":\"someone\"}}\n";
        let r = rpc_result(out).unwrap();
        assert_eq!(
            status_from_result(&r),
            AuthStatus {
                authenticated: true,
                kind: "user".into()
            }
        );
        assert!(rpc_result("Content-Length: 2\n\n{}").is_none());
        assert!(rpc_result("not json at all").is_none());
        let err = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"no"}}"#;
        assert!(rpc_result(err).is_none());
    }

    #[test]
    fn agent_mode_hides_the_users_tokens_and_user_mode_keeps_its_home() {
        let home = std::env::temp_dir().join(format!("vf-copilot-home-{}", std::process::id()));
        let copilot = Copilot::detect().with_home(home.clone());
        let base = ChildEnv::from_vars([
            ("GH_TOKEN".into(), "x".into()),
            ("COPILOT_GITHUB_TOKEN".into(), "y".into()),
        ]);
        let env = copilot.env(&base);
        assert_eq!(env.get(HOME_VAR), Some(home.as_os_str()));
        assert_eq!(env.get("GH_TOKEN"), None);
        assert_eq!(env.get("COPILOT_GITHUB_TOKEN"), None);

        copilot.set_cli_home_mode(CliHomeMode::User);
        let env = copilot.env(&ChildEnv::from_vars([(
            HOME_VAR.into(),
            "elsewhere".into(),
        )]));
        assert_eq!(env.get(HOME_VAR), None);
        let cmd = copilot.command(Path::new("copilot"));
        assert!(
            cmd.get_envs().any(|(k, v)| k == HOME_VAR && v.is_none()),
            "COPILOT_HOME must be explicitly removed in user mode"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn runs_allow_tools_and_questions_deny_changes() {
        let run = args("", Some("abc"));
        assert!(run.contains(&"--allow-all-tools".to_owned()));
        assert!(run.windows(2).any(|w| w == ["--resume", "abc"]));
        assert!(!run.contains(&"--model".to_owned()));
        assert!(!run.iter().any(|a| a == "-p" || a == "--prompt"));
        let ask = ask_args("some-model");
        assert!(!ask.contains(&"--allow-all-tools".to_owned()));
        assert!(ask.contains(&"--deny-tool=shell".to_owned()));
        assert!(ask.contains(&"--deny-tool=write".to_owned()));
        assert!(ask.windows(2).any(|w| w == ["--model", "some-model"]));
        for a in [&run, &ask] {
            assert!(a.contains(&"--disable-builtin-mcps".to_owned()));
            assert!(a.contains(&"--no-auto-update".to_owned()));
        }
    }

    #[test]
    fn a_system_instruction_goes_before_the_prompt() {
        assert_eq!(with_system("do it", None), "do it");
        assert_eq!(with_system("do it", Some("  ")), "do it");
        assert_eq!(with_system("do it", Some("rules")), "rules\n\n---\n\ndo it");
    }

    #[test]
    fn cli_errors_are_explained() {
        let denied =
            explain_error("Error: Access denied by policy settings (Request ID: 1)").unwrap();
        assert!(
            denied.contains("not available for this GitHub account"),
            "{denied}"
        );
        let limit = explain_error("Error: You have exceeded your monthly quota").unwrap();
        assert!(limit.contains("limit is used up"), "{limit}");
        let other = explain_error("Error: something else").unwrap();
        assert_eq!(other, "GitHub Copilot: something else");
        assert!(explain_error("a warning line").is_none());
    }
}
