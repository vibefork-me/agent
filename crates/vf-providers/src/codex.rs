//! Codex CLI (the user's ChatGPT subscription): `codex exec --json`.
//!
//! Without the user's Codex configuration, plugins and rules (`--ignore-user-config
//! --ignore-rules`); the sign-in still works. Full access to the project folder: the Codex Windows
//! sandbox needs an administrator setup, and isolation is the agent's job.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use vf_protocol::{Cancel, CostClass, Event, ModelInfo, Usage};

use crate::cli::{collect, run_cli};
use crate::process;
use crate::{Answer, AskRequest, ChildEnv, Output, Provider, RunRequest, RunResult, clip};

/// Where Codex keeps its sign-in, settings, sessions and the models cache.
const HOME_VAR: &str = "CODEX_HOME";

/// How long the sign-in status and model list are cached.
const CACHE_TTL: Duration = Duration::from_secs(60);

/// The Codex CLI.
#[derive(Debug)]
pub struct Codex {
    bin: std::sync::RwLock<Option<PathBuf>>,
    cache: Mutex<Option<(Instant, Vec<ModelInfo>)>>,
    signed_in_cache: Mutex<Option<(Instant, bool)>>,
    user_cache: Mutex<Option<(Instant, bool)>>,
    /// `CODEX_HOME`: sign-in, settings, sessions and the models cache in agent mode.
    agent_home: Option<PathBuf>,
    /// Mode: agent or user.
    mode: std::sync::RwLock<vf_protocol::CliHomeMode>,
}

impl Codex {
    fn bin(&self) -> Option<PathBuf> {
        self.bin.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Finds the CLI: `VF_CODEX_BIN`; on Windows the newest CLI of the Codex app
    /// (`%LOCALAPPDATA%\OpenAI\Codex\bin\<hash>\codex.exe`); else `codex` on `PATH`.
    pub fn detect() -> Self {
        let bin = std::env::var_os("VF_CODEX_BIN")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(app_cli)
            .or_else(|| process::find_program("codex"));
        Self {
            bin: std::sync::RwLock::new(bin),
            cache: Mutex::new(None),
            signed_in_cache: Mutex::new(None),
            user_cache: Mutex::new(None),
            agent_home: None,
            mode: std::sync::RwLock::new(vf_protocol::CliHomeMode::Agent),
        }
    }

    /// Keeps the CLI's sign-in, settings and sessions in `home` in agent mode instead of `~/.codex`.
    pub fn with_home(mut self, home: PathBuf) -> Self {
        self.agent_home = Some(home);
        self
    }

    /// Sets the CLI home mode (agent or user).
    pub fn with_mode(self, mode: vf_protocol::CliHomeMode) -> Self {
        *self.mode.write().unwrap_or_else(|p| p.into_inner()) = mode;
        self
    }

    fn active_home(&self) -> Option<PathBuf> {
        let mode = *self.mode.read().unwrap_or_else(|p| p.into_inner());
        match mode {
            vf_protocol::CliHomeMode::Agent => self.agent_home.clone(),
            vf_protocol::CliHomeMode::User => None,
        }
    }

    /// The folder the CLI keeps its data in.
    fn data_dir(&self) -> PathBuf {
        self.active_home().unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".codex")
        })
    }

    fn command(&self, bin: &Path) -> std::process::Command {
        let mut cmd = process::command(bin);
        if let Some(home) = self.active_home() {
            let _ = std::fs::create_dir_all(&home);
            cmd.env(HOME_VAR, home);
        } else {
            cmd.env_remove(HOME_VAR);
        }
        cmd
    }

    fn env(&self, env: &ChildEnv) -> ChildEnv {
        let mut env = env.clone();
        if let Some(home) = self.active_home() {
            let _ = std::fs::create_dir_all(&home);
            env.set(HOME_VAR, home.as_os_str());
        } else {
            env.remove(std::ffi::OsStr::new(HOME_VAR));
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
        let mut lines = String::new();
        let mut cmd = self.command(bin);
        cmd.args(["login", "status"]);
        // Codex prints the status to stderr.
        let exit = process::run_streaming(
            cmd,
            None,
            &Cancel::new(),
            Some(Duration::from_secs(20)),
            &mut |_, l| {
                lines.push_str(&l);
            },
        );
        let ok =
            exit.is_ok_and(|e| e.code == 0) && lines.to_ascii_lowercase().contains("logged in");
        *cache = Some((Instant::now(), ok));
        ok
    }

    /// Whether the user has an existing login in `~/.codex` (without `CODEX_HOME`).
    fn user_logged_in(&self, bin: &Path) -> bool {
        let mut cache = self.user_cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, ok)) = *cache
            && at.elapsed() < CACHE_TTL
        {
            return ok;
        }
        let mut lines = String::new();
        let mut cmd = process::command(bin);
        cmd.env_remove(HOME_VAR);
        cmd.args(["login", "status"]);
        let exit = process::run_streaming(
            cmd,
            None,
            &Cancel::new(),
            Some(Duration::from_secs(20)),
            &mut |_, l| {
                lines.push_str(&l);
            },
        );
        let ok =
            exit.is_ok_and(|e| e.code == 0) && lines.to_ascii_lowercase().contains("logged in");
        *cache = Some((Instant::now(), ok));
        ok
    }
}

fn app_cli() -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
        .join("OpenAI")
        .join("Codex")
        .join("bin");
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(base)
        .ok()?
        .filter_map(Result::ok)
        .map(|d| d.path().join("codex.exe"))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
        .collect();
    found.sort();
    found.pop().map(|(_, p)| p)
}

/// Models from Codex's own cache (`~/.codex/models_cache.json`), listed ones only, plus the default.
pub fn models_from_cache(json: &str) -> Vec<ModelInfo> {
    let v: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    let list = v["models"]
        .as_array()
        .or_else(|| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out: Vec<ModelInfo> = list
        .iter()
        .filter(|m| m["visibility"].as_str().is_none_or(|vis| vis == "list"))
        .filter_map(|m| {
            let slug = m["slug"].as_str().or_else(|| m["id"].as_str())?;
            if slug.contains("review") {
                return None;
            }
            let desc = m["description"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let tier = if ["fast", "easier", "efficient"]
                .iter()
                .any(|w| desc.contains(w))
            {
                1
            } else {
                2
            };
            Some(ModelInfo {
                provider: "codex".into(),
                model: slug.into(),
                tier,
                cost: CostClass::Subscription,
                label: format!("Codex {slug}"),
            })
        })
        .collect();
    out.push(ModelInfo {
        provider: "codex".into(),
        model: String::new(),
        tier: 3,
        cost: CostClass::Subscription,
        label: "Codex (default model)".into(),
    });
    out
}

/// Command-line arguments (the prompt goes through stdin: `-`).
/// `exec resume` has no `-s` / `-C`: the sandbox mode is set through `-c`, the folder is the working
/// directory of the process.
pub fn args(dir: &Path, model: &str, resume: Option<&str>, system: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec!["exec".into()];
    if resume.is_some() {
        a.extend(["resume", "-c", "sandbox_mode=\"danger-full-access\""].map(str::to_owned));
    } else {
        a.extend([
            "-s".into(),
            "danger-full-access".into(),
            "-C".into(),
            dir.display().to_string(),
        ]);
    }
    a.extend(
        [
            "--json",
            "--skip-git-repo-check",
            "--ignore-user-config",
            "--ignore-rules",
        ]
        .map(str::to_owned),
    );
    if !model.is_empty() {
        a.extend(["-m".into(), model.into()]);
    }
    if let Some(sys) = system.filter(|s| !s.trim().is_empty())
        && let Ok(json_val) = serde_json::to_string(sys)
    {
        a.extend(["-c".into(), format!("developer_instructions={json_val}")]);
    }
    if let Some(r) = resume {
        a.push(r.into());
    }
    a.push("-".into());
    a
}

/// Normalizes one `--json` line.
pub fn normalize(e: &Value) -> Vec<Event> {
    let s = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    let it = &e["item"];
    let ev = match (e["type"].as_str(), it["type"].as_str()) {
        (Some("thread.started"), _) => Some(Event::Init {
            session: e["thread_id"].as_str().map(str::to_owned),
            model: None,
            cli: "Codex CLI".into(),
            mcp: None,
            skills: None,
        }),
        (Some("item.started"), Some("command_execution")) => Some(Event::Tool {
            tool: "shell".into(),
            detail: s(&it["command"]),
        }),
        (Some("item.completed"), Some("agent_message")) => Some(Event::Text {
            text: s(&it["text"]),
        }),
        (Some("item.completed"), Some("command_execution")) => Some(Event::ToolResult {
            error: it["exit_code"].as_i64().is_some_and(|c| c != 0),
            text: clip(&s(&it["aggregated_output"]), 2000),
        }),
        (Some("item.completed"), Some("file_change")) => Some(Event::Tool {
            tool: "edit".into(),
            detail: it["changes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| {
                    format!("{} {}", s(&c["kind"]), s(&c["path"]))
                        .trim()
                        .to_owned()
                })
                .collect::<Vec<_>>()
                .join(", "),
        }),
        (Some("item.completed"), Some("error")) => Some(Event::Error {
            text: s(&it["message"]),
        }),
        (Some("turn.completed"), _) => Some(Event::Result {
            ok: true,
            text: String::new(),
            turns: None,
            ms: None,
            cost_usd: None,
            usage: e["usage"].is_object().then(|| Usage {
                input_tokens: e["usage"]["input_tokens"].as_u64().unwrap_or(0),
                output_tokens: e["usage"]["output_tokens"].as_u64().unwrap_or(0),
            }),
        }),
        (Some("turn.failed" | "error"), _) => Some(Event::Error {
            text: e["error"]["message"]
                .as_str()
                .or_else(|| e["message"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| clip(&e.to_string(), 300)),
        }),
        _ => None,
    };
    ev.into_iter().collect()
}

impl Provider for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn logout(&self) -> Result<(), String> {
        // Only the agent's own folder: without it `logout` would sign the user out of their CLI.
        if self.active_home().is_none() {
            return Err("the user's own sign-in is not signed out by the agent".into());
        }
        let bin = self.bin().ok_or("Codex is not installed")?;
        let mut cmd = self.command(&bin);
        cmd.arg("logout");
        let exit = process::run_streaming(
            cmd,
            None,
            &Cancel::new(),
            Some(Duration::from_secs(30)),
            &mut |_, _| {},
        )
        .map_err(|e| e.to_string())?;
        self.reload();
        if exit.code == 0 {
            Ok(())
        } else {
            Err(format!("sign-out did not finish (code {})", exit.code))
        }
    }

    fn connect(&self, timeout: Duration) -> Result<(), String> {
        let bin = self.bin().ok_or("Codex is not installed")?;
        let mut cmd = self.command(&bin);
        cmd.arg("login");
        let exit = process::run_streaming(cmd, None, &Cancel::new(), Some(timeout), &mut |_, _| {})
            .map_err(|e| e.to_string())?;
        *self.cache.lock().unwrap_or_else(|p| p.into_inner()) = None;
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
            cli_home_mode: Some(*self.mode.read().unwrap_or_else(|p| p.into_inner())),
        }
    }

    fn reload(&self) {
        *self.cache.lock().unwrap_or_else(|p| p.into_inner()) = None;
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
        Some(*self.mode.read().unwrap_or_else(|p| p.into_inner()))
    }

    fn models(&self) -> Vec<ModelInfo> {
        let Some(bin) = self.bin() else {
            return Vec::new();
        };
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, models)) = cache.as_ref()
            && at.elapsed() < CACHE_TTL
        {
            return models.clone();
        }
        let models = if self.logged_in(&bin) {
            let json = std::fs::read_to_string(self.data_dir().join("models_cache.json"))
                .unwrap_or_default();
            models_from_cache(&json)
        } else {
            Vec::new()
        };
        *cache = Some((Instant::now(), models.clone()));
        models
    }

    fn can_resume(&self) -> bool {
        true
    }

    fn ask(&self, req: &AskRequest<'_>, cancel: &Cancel) -> Result<Answer, String> {
        let env = self.env(req.env);
        let req = &AskRequest { env: &env, ..*req };
        let mut args: Vec<String> = [
            "exec",
            "--json",
            "-s",
            "read-only",
            "--skip-git-repo-check",
            "--ignore-user-config",
            "--ignore-rules",
            "-C",
        ]
        .map(str::to_owned)
        .to_vec();
        args.push(req.dir.display().to_string());
        if !req.model.is_empty() {
            args.extend(["-m".into(), req.model.into()]);
        }
        if let Some(sys) = req.system.filter(|s| !s.trim().is_empty())
            && let Ok(json_val) = serde_json::to_string(sys)
        {
            args.extend(["-c".into(), format!("developer_instructions={json_val}")]);
        }
        args.push("-".into());
        let stdout = collect(self.bin().as_deref(), "codex", &args, req, cancel)?;
        let events: Vec<Value> = stdout
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .collect();
        let text = events
            .iter()
            .filter(|e| e["item"]["type"] == "agent_message")
            .filter_map(|e| e["item"]["text"].as_str().map(str::to_owned))
            .next_back()
            .ok_or_else(|| no_answer(&events))?;
        Ok(Answer {
            text,
            cost_usd: None,
        })
    }

    fn run(&self, req: &RunRequest<'_>, out: &mut dyn FnMut(Output), cancel: &Cancel) -> RunResult {
        let env = self.env(req.env);
        let req = &RunRequest { env: &env, ..*req };
        run_cli(
            self.bin().as_deref(),
            "codex",
            args(req.dir, req.model, req.resume, req.system),
            req,
            out,
            cancel,
            normalize,
        )
    }
}

/// Why `codex exec` gave no answer: the last error it reported (`error`, `turn.failed`), if any.
fn no_answer(events: &[Value]) -> String {
    events
        .iter()
        .rev()
        .find_map(|e| {
            e["message"]
                .as_str()
                .or_else(|| e["error"]["message"].as_str())
                .filter(|_| matches!(e["type"].as_str(), Some("error" | "turn.failed")))
        })
        .map_or_else(
            || "codex gave no answer".to_owned(),
            |m| format!("codex gave no answer: {}", clip(m, 300)),
        )
}

#[cfg(test)]
mod no_answer_tests {
    use super::*;

    #[test]
    fn the_reported_error_is_kept() {
        let events: Vec<Value> = [
            r#"{"type":"thread.started"}"#,
            r#"{"type":"turn.failed","error":{"message":"You must sign in"}}"#,
        ]
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
        assert_eq!(no_answer(&events), "codex gave no answer: You must sign in");
        assert_eq!(no_answer(&[]), "codex gave no answer");
    }
}

#[cfg(test)]
mod home_tests {
    use super::*;

    #[test]
    fn the_cli_home_holds_the_models_cache_and_reaches_every_process() {
        let home = std::env::temp_dir().join(format!("vf-codex-home-{}", std::process::id()));
        let codex = Codex::detect().with_home(home.clone());
        assert_eq!(codex.data_dir(), home);
        let env = codex.env(&ChildEnv::from_vars([]));
        assert_eq!(env.get(HOME_VAR), Some(home.as_os_str()));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn user_mode_does_not_set_codex_home() {
        let home = std::env::temp_dir().join(format!("vf-codex-user-{}", std::process::id()));
        let codex = Codex::detect()
            .with_home(home.clone())
            .with_mode(vf_protocol::CliHomeMode::User);
        let env = codex.env(&ChildEnv::from_vars([(HOME_VAR.into(), "existing".into())]));
        assert_eq!(env.get(HOME_VAR), None);
        let cmd = codex.command(Path::new("codex"));
        assert!(
            cmd.get_envs().any(|(k, v)| k == HOME_VAR && v.is_none()),
            "CODEX_HOME must be explicitly removed in user mode"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn switching_cli_home_mode_toggles_environment() {
        let home = std::env::temp_dir().join(format!("vf-codex-toggle-{}", std::process::id()));
        let codex = Codex::detect().with_home(home.clone());
        assert_eq!(codex.cli_home_mode(), Some(vf_protocol::CliHomeMode::Agent));
        let env = codex.env(&ChildEnv::from_vars([]));
        assert_eq!(env.get(HOME_VAR), Some(home.as_os_str()));

        codex.set_cli_home_mode(vf_protocol::CliHomeMode::User);
        assert_eq!(codex.cli_home_mode(), Some(vf_protocol::CliHomeMode::User));
        let env = codex.env(&ChildEnv::from_vars([]));
        assert_eq!(env.get(HOME_VAR), None);

        codex.set_cli_home_mode(vf_protocol::CliHomeMode::Agent);
        assert_eq!(codex.cli_home_mode(), Some(vf_protocol::CliHomeMode::Agent));
        let env = codex.env(&ChildEnv::from_vars([]));
        assert_eq!(env.get(HOME_VAR), Some(home.as_os_str()));
        let _ = std::fs::remove_dir_all(&home);
    }
}
