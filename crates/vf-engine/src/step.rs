//! One step: one model in the project folder, then a checkpoint and a journal entry.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use vf_git::Repo;
use vf_journal::{JournalStore as _, StepReport, StepStart, handoff_preamble};
use vf_protocol::ws::{ServerMessage, StepInfo, ToolchainStatus};
use vf_protocol::{Cancel, Event};
use vf_providers::{ChildEnv, Output, Provider, RunRequest};
use vf_sandbox::IsolationLevel;
use vf_toolchains::Progress;

use crate::{Engine, MAX_DIFF};

/// `PATH` entries hidden by the clean-machine simulation.
const SYSTEM_TOOLCHAINS: &[&str] = &["java", "jdk", "temurin", "gradle", "maven"];

/// What to run.
#[derive(Debug, Clone, Copy)]
pub struct StepRequest<'a> {
    /// Project name.
    pub project: &'a str,
    /// Project folder.
    pub dir: &'a Path,
    /// Prompt of the step.
    pub prompt: &'a str,
    /// Text put before the prompt (plan context).
    pub prefix: &'a str,
    /// Model.
    pub model: &'a str,
    /// Continue with the project's history.
    pub continue_: bool,
    /// Position in a plan.
    pub step: Option<&'a StepInfo>,
    /// System instruction, if any.
    pub system: Option<&'a str>,
}

/// How a step ended.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepResult {
    /// Exit code.
    pub code: i32,
    /// The model reported success and exited with 0.
    pub ok: bool,
    /// Stopped by cancel.
    pub cancelled: bool,
    /// Provider session.
    pub session: Option<String>,
    /// Newest commit of the step.
    pub commit: Option<String>,
    /// Diff of the step.
    pub diff: String,
    /// `git diff --stat` of the step.
    pub status: String,
}

impl Engine {
    /// The environment of the model's (and the build's) processes: cleaned, the agent's git
    /// identity, the project's toolchain in front of `PATH`. Returns it with the toolchain folders.
    /// Prepares the environment for a child process.
    pub(crate) fn child_env(
        &self,
        dir: &Path,
        sandboxed: bool,
        is_cli: bool,
        run_id: Option<&str>,
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) -> (ChildEnv, Vec<String>, IsolationLevel) {
        let mut env = ChildEnv::from_current();

        // For internal processes (builds and agent loops), redirect HOME, USERPROFILE,
        // APPDATA, LOCALAPPDATA, XDG_*, and TEMP to the agent's caches folder so no files
        // leak into the real user profile. Coding CLIs (claude/codex) keep the real HOME
        // to access their auth sessions, but inherit explicit toolchain cache variables.
        if !is_cli {
            let sandbox_home = self.caches.join("home");
            let sandbox_tmp = match run_id {
                Some(id) => self.caches.join("tmp").join(id),
                None => self.caches.join("tmp"),
            };
            let _ = std::fs::create_dir_all(&sandbox_home);
            let _ = std::fs::create_dir_all(&sandbox_tmp);

            let home_str = sandbox_home.display().to_string();
            let tmp_str = sandbox_tmp.display().to_string();

            env.set("HOME", &home_str);
            env.set("USERPROFILE", &home_str);
            env.set(
                "APPDATA",
                sandbox_home
                    .join("AppData")
                    .join("Roaming")
                    .display()
                    .to_string(),
            );
            env.set(
                "LOCALAPPDATA",
                sandbox_home
                    .join("AppData")
                    .join("Local")
                    .display()
                    .to_string(),
            );
            env.set(
                "XDG_CONFIG_HOME",
                sandbox_home.join(".config").display().to_string(),
            );
            env.set(
                "XDG_CACHE_HOME",
                sandbox_home.join(".cache").display().to_string(),
            );
            env.set(
                "XDG_DATA_HOME",
                sandbox_home
                    .join(".local")
                    .join("share")
                    .display()
                    .to_string(),
            );
            env.set(
                "XDG_STATE_HOME",
                sandbox_home
                    .join(".local")
                    .join("state")
                    .display()
                    .to_string(),
            );
            env.set("TEMP", &tmp_str);
            env.set("TMP", &tmp_str);
            env.set("TMPDIR", &tmp_str);
        }

        // Commits made by the model carry the agent's identity, not the user's git identity.
        for (k, v) in [
            ("GIT_AUTHOR_NAME", vf_git::AGENT_NAME),
            ("GIT_COMMITTER_NAME", vf_git::AGENT_NAME),
            ("GIT_AUTHOR_EMAIL", vf_git::AGENT_EMAIL),
            ("GIT_COMMITTER_EMAIL", vf_git::AGENT_EMAIL),
        ] {
            env.set(k, v);
        }
        if self.simulate_clean_machine {
            env.hide_path_entries(SYSTEM_TOOLCHAINS);
            env.remove("JAVA_HOME".as_ref());
            env.remove("GRADLE_HOME".as_ref());
        }
        // The user should not need Java / Gradle / Node installed.
        let prepared = vf_toolchains::prepare(dir, &self.tools, &self.caches, cancel, &mut |p| {
            out(match p {
                Progress::Installing { tool } => ServerMessage::Toolchain {
                    status: ToolchainStatus::Installing,
                    tool,
                    path: None,
                    seconds: None,
                    error: None,
                },
                Progress::Ready {
                    tool,
                    path,
                    seconds,
                } => ServerMessage::Toolchain {
                    status: ToolchainStatus::Ready,
                    tool,
                    path: Some(path.display().to_string()),
                    seconds: Some(format!("{seconds:.1}")),
                    error: None,
                },
                Progress::Failed { tool, error } => ServerMessage::Toolchain {
                    status: ToolchainStatus::Failed,
                    tool,
                    path: None,
                    seconds: None,
                    error: Some(error),
                },
            })
        });
        env.prepend_path(&prepared.paths);
        for (k, v) in prepared.env {
            env.set(k, v);
        }
        if let Ok(agent) = std::env::current_exe()
            && let Some(shim_dir) = install_git_shim(&self.tools, &agent)
        {
            env.prepend_path(&[shim_dir]);
        }
        // Everything the model and the build start runs in the sandbox (when there is one).
        let level = match self.sandbox_helper.as_ref().filter(|_| sandboxed) {
            Some(helper) => {
                let policy = vf_sandbox::task_policy(dir, &self.tools, &self.caches);
                let (wrapper, level) = vf_sandbox::prepare(&policy, helper);
                env.set_wrapper(wrapper);
                level
            }
            None => IsolationLevel::None,
        };
        let paths = prepared
            .paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        (env, paths, level)
    }

    /// Runs one step and journals it.
    pub fn step(
        &self,
        repo: &Repo,
        provider: &dyn Provider,
        req: &StepRequest<'_>,
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) -> StepResult {
        let id = provider.id();
        // Context: native resume with the same provider + model, otherwise a handoff from the journal.
        let journal = self.journal.read(req.project).unwrap_or_default();
        let mut resume = None;
        let mut handoff = false;
        let mut prompt = format!("{}{}", req.prefix, req.prompt);
        if req.continue_
            && let Some(last) = journal.last()
        {
            match &last.session {
                Some(s)
                    if last.provider == id && last.model == req.model && provider.can_resume() =>
                {
                    resume = Some(s.clone())
                }
                _ => {
                    prompt = format!("{}{}", handoff_preamble(&journal), prompt);
                    handoff = true;
                }
            }
        }

        let cli = matches!(id, "claude" | "codex");
        let sandboxed = !cli || vf_sandbox::supports_coding_clis();
        let run_id = new_run_id(&format!("step-{}", req.project));
        let _tmp_guard = (!cli).then(|| TmpGuard::new(&self.caches, &run_id));
        let (env, toolchain, isolation) =
            self.child_env(req.dir, sandboxed, cli, Some(&run_id), out, cancel);
        out(ServerMessage::Started {
            project: req.project.to_owned(),
            provider: id.to_owned(),
            model: if req.model.is_empty() {
                "default".into()
            } else {
                req.model.to_owned()
            },
            resume: resume.is_some(),
            handoff,
            toolchain,
            step: req.step.cloned(),
            isolation: isolation.as_str().into(),
        });
        tracing::info!(
            project = req.project,
            provider = id,
            model = req.model,
            resume = resume.is_some(),
            handoff,
            "step start"
        );

        let start = match StepStart::begin(repo) {
            Ok(s) => s,
            Err(e) => {
                out(ServerMessage::Error {
                    error: format!("git: {e}"),
                    code: None,
                });
                return StepResult {
                    code: -1,
                    ..StepResult::default()
                };
            }
        };
        let started = std::time::Instant::now();
        let run = RunRequest {
            dir: req.dir,
            prompt: &prompt,
            model: req.model,
            resume: resume.as_deref(),
            env: &env,
            system: req.system,
        };
        let result = provider.run(
            &run,
            &mut |o| match o {
                Output::Event(ev) => {
                    if let Event::Error { text } = &ev {
                        tracing::warn!(provider = id, error = %text, "provider error");
                    }
                    out(ServerMessage::Evt {
                        provider: id.to_owned(),
                        ev,
                    })
                }
                Output::Stderr(text) => out(ServerMessage::Stderr { text }),
            },
            cancel,
        );

        let label = match req.step {
            Some(s) => format!("step {}/{}: {}", s.n, s.of, s.title),
            None => req.prompt.lines().next().unwrap_or_default().to_owned(),
        };
        let report = StepReport {
            provider: id.to_owned(),
            model: req.model.to_owned(),
            prompt: match req.step {
                Some(s) => format!("[step {}/{}] {}", s.n, s.of, s.title),
                None => req.prompt.to_owned(),
            },
            label,
            customization: None,
            session: result.session.clone(),
            handoff,
            summary: result.summary.clone(),
            ms: result.ms.or(Some(started.elapsed().as_millis() as u64)),
            cost_usd: result.cost_usd,
            usage: result.usage.clone(),
        };
        let base = start.base().to_owned();
        let finished = start.finish(repo, &self.journal, req.project, now_rfc3339(), report);
        tracing::info!(
            project = req.project,
            code = result.code,
            ok = result.ok,
            "step end"
        );
        match finished {
            Ok(outcome) => StepResult {
                code: result.code,
                ok: result.ok,
                cancelled: result.cancelled,
                session: result.session,
                status: outcome
                    .entry
                    .commit
                    .as_ref()
                    .and_then(|_| repo.diff_stat(&base, "HEAD").ok())
                    .unwrap_or_default(),
                commit: outcome.entry.commit,
                diff: clip_bytes(outcome.diff, MAX_DIFF),
            },
            Err(e) => {
                out(ServerMessage::Evt {
                    provider: id.to_owned(),
                    ev: Event::Error {
                        text: format!("checkpoint failed: {e}"),
                    },
                });
                StepResult {
                    code: if result.code == 0 { 1 } else { result.code },
                    cancelled: result.cancelled,
                    session: result.session,
                    ..StepResult::default()
                }
            }
        }
    }
}

/// RAII guard that removes a run's temporary folder upon completion.
pub(crate) struct TmpGuard {
    dir: std::path::PathBuf,
}

impl TmpGuard {
    pub(crate) fn new(caches: &Path, run_id: &str) -> Self {
        Self {
            dir: caches.join("tmp").join(run_id),
        }
    }
}

impl Drop for TmpGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Removes stale run folders in `tmp_dir` older than `max_age` (e.g. 24h remnants from crashes).
pub(crate) fn clean_stale_tmp(tmp_dir: &Path, max_age: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(tmp_dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(meta) = entry.metadata() {
            let modified = meta.modified().unwrap_or(UNIX_EPOCH);
            if let Ok(age) = now.duration_since(modified)
                && age >= max_age
            {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}

/// Generates a unique run ID for a task or build.
pub(crate) fn new_run_id(prefix: &str) -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{prefix}-{ts}-{}-{seq}", std::process::id())
}

fn install_git_shim(tools: &Path, agent: &Path) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    if !git_shim_required(&path, cfg!(target_os = "macos"), || {
        std::process::Command::new("xcode-select")
            .arg("-p")
            .output()
            .is_ok_and(|o| o.status.success())
    }) {
        return None;
    }
    let dir = tools.join("shims");
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    #[cfg(windows)]
    let (file, content) = (
        dir.join("git.cmd"),
        format!("@\"{}\" git-shim %*\r\n", agent.display()),
    );
    #[cfg(not(windows))]
    let (file, content) = (
        dir.join("git"),
        format!("#!/bin/sh\nexec \"{}\" git-shim \"$@\"\n", agent.display()),
    );
    if std::fs::write(&file, content).is_err() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).is_err() {
            return None;
        }
    }
    Some(dir)
}

fn git_shim_required(
    path: &std::ffi::OsStr,
    macos: bool,
    xcode_tools_present: impl FnOnce() -> bool,
) -> bool {
    let found = std::env::split_paths(path).any(|dir| {
        let candidate = dir.join(if cfg!(windows) { "git.exe" } else { "git" });
        candidate.is_file()
    });
    !found || (macos && !xcode_tools_present())
}

pub(crate) fn clip_bytes(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut cut = max;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    s
}

/// Current UTC time as `YYYY-MM-DDTHH:MM:SSZ`.
pub(crate) fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn time_format() {
        let t = now_rfc3339();
        assert_eq!(t.len(), 20);
        assert!(t.starts_with("20") && t.ends_with('Z'));
    }

    #[test]
    fn diff_is_clipped_on_char_boundary() {
        assert_eq!(clip_bytes("ааа".into(), 3), "а");
    }

    #[test]
    fn shim_selection_detects_missing_git_and_broken_macos_stub() {
        let root = std::env::temp_dir().join(format!("vf-git-shim-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(std::fs::create_dir_all(&root).is_ok());
        assert!(git_shim_required(root.as_os_str(), false, || true));
        let marker = root.join(if cfg!(windows) { "git.exe" } else { "git" });
        assert!(std::fs::write(&marker, b"").is_ok());
        assert!(git_shim_required(root.as_os_str(), true, || false));
        assert!(!git_shim_required(root.as_os_str(), true, || true));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_runs_do_not_delete_each_others_temp() {
        let base = std::env::temp_dir().join(format!("vf-tmp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let caches = base.join("caches");
        let run1_id = "run-1";
        let run2_id = "run-2";

        let guard1 = TmpGuard::new(&caches, run1_id);
        let guard2 = TmpGuard::new(&caches, run2_id);

        let tmp1 = caches.join("tmp").join(run1_id);
        let tmp2 = caches.join("tmp").join(run2_id);

        std::fs::create_dir_all(&tmp1).unwrap();
        std::fs::create_dir_all(&tmp2).unwrap();

        let file1 = tmp1.join("temp_file1.txt");
        let file2 = tmp2.join("temp_file2.txt");

        std::fs::write(&file1, "run1").unwrap();
        std::fs::write(&file2, "run2").unwrap();

        assert!(file1.exists());
        assert!(file2.exists());

        // Run 1 finishes and cleans up its own folder
        drop(guard1);

        assert!(!file1.exists(), "run1 temp should be deleted");
        assert!(!tmp1.exists(), "run1 dir should be deleted");
        assert!(
            file2.exists(),
            "run2 temp file must still exist while run2 is active"
        );
        assert!(tmp2.exists(), "run2 dir must still exist");

        // Run 2 finishes and cleans up
        drop(guard2);

        assert!(!file2.exists(), "run2 temp should now be deleted");
        assert!(!tmp2.exists(), "run2 dir should now be deleted");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn stale_temp_cleanup_deletes_old_folders_only() {
        let base = std::env::temp_dir().join(format!("vf-stale-tmp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let tmp_root = base.join("tmp");
        let fresh_dir = tmp_root.join("fresh-run");
        let stale_dir = tmp_root.join("stale-run");

        std::fs::create_dir_all(&fresh_dir).unwrap();
        std::fs::create_dir_all(&stale_dir).unwrap();

        // With max_age = 1 day, neither is considered old
        clean_stale_tmp(&tmp_root, std::time::Duration::from_secs(86_400));
        assert!(fresh_dir.exists());
        assert!(stale_dir.exists());

        // With zero duration, both are stale
        clean_stale_tmp(&tmp_root, std::time::Duration::from_secs(0));
        assert!(!fresh_dir.exists());
        assert!(!stale_dir.exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn internal_processes_get_fake_home_and_clis_keep_real_home() {
        let base = std::env::temp_dir().join(format!("vf-engine-step-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let settings = vf_platform::Settings {
            state_dir: base.join("state"),
            port: 1,
            extra_origins: vec![],
            ignored_extra_origins: vec![],
            ui_bundle_url: String::new(),
            storage: vf_platform::settings::Storage {
                projects: base.join("projects"),
                tools: base.join("tools"),
                caches: base.join("caches"),
                artifacts: base.join("artifacts"),
            },
            simulate_clean_machine: false,
            update_url: String::new(),
            auto_update: false,
        };
        let engine = Engine::new(&settings, Vec::new());
        let project_dir = base.join("projects/demo");
        let cancel = Cancel::default();

        // 1. Internal processes / builds (is_cli = false): fake home and tmp in caches/<run_id>
        let run_id = "test-run-123";
        let (env_internal, _, _) = engine.child_env(
            &project_dir,
            false,
            false,
            Some(run_id),
            &mut |_| {},
            &cancel,
        );
        let home = env_internal
            .get("HOME")
            .map(|s| s.to_string_lossy().to_string());
        let userprofile = env_internal
            .get("USERPROFILE")
            .map(|s| s.to_string_lossy().to_string());
        let tmp = env_internal
            .get("TEMP")
            .map(|s| s.to_string_lossy().to_string());

        let expected_home = engine.caches.join("home").display().to_string();
        let expected_tmp = engine.caches.join("tmp").join(run_id).display().to_string();

        assert_eq!(home.as_deref(), Some(expected_home.as_str()));
        assert_eq!(userprofile.as_deref(), Some(expected_home.as_str()));
        assert_eq!(tmp.as_deref(), Some(expected_tmp.as_str()));

        // 2. CLI providers (is_cli = true): real home is preserved
        let (env_cli, _, _) = engine.child_env(
            &project_dir,
            false,
            true,
            Some(run_id),
            &mut |_| {},
            &cancel,
        );
        let cli_home = env_cli.get("HOME").map(|s| s.to_string_lossy().to_string());
        assert_ne!(cli_home.as_deref(), Some(expected_home.as_str()));

        let _ = std::fs::remove_dir_all(&base);
    }
}
