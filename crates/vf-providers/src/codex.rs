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
    /// `CODEX_HOME`: sign-in, settings, sessions and the models cache; `None` — `~/.codex`.
    home: Option<PathBuf>,
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
            home: None,
        }
    }

    /// Keeps the CLI's sign-in, settings and sessions in `home` instead of `~/.codex`.
    pub fn with_home(mut self, home: PathBuf) -> Self {
        self.home = Some(home);
        self
    }

    /// The folder the CLI keeps its data in.
    fn data_dir(&self) -> PathBuf {
        self.home.clone().unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".codex")
        })
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

    fn logged_in(&self, bin: &Path) -> bool {
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
        exit.is_ok_and(|e| e.code == 0) && lines.to_ascii_lowercase().contains("logged in")
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
pub fn args(dir: &Path, model: &str, resume: Option<&str>) -> Vec<String> {
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
        crate::Status {
            installed: self.bin().is_some(),
            signed_in: Some(self.bin().is_some() && !self.models().is_empty()),
        }
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
        args.push("-".into());
        let stdout = collect(self.bin().as_deref(), "codex", &args, req, cancel)?;
        let text = stdout
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|e| e["item"]["type"] == "agent_message")
            .filter_map(|e| e["item"]["text"].as_str().map(str::to_owned))
            .next_back()
            .ok_or("codex gave no answer")?;
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
            args(req.dir, req.model, req.resume),
            req,
            out,
            cancel,
            normalize,
        )
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
}
