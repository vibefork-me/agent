//! Building a project and rolling it back to a checkpoint.

use std::path::Path;
use std::time::{Duration, Instant};

use vf_builds::estimate::{agents_memory_hint, profile};
use vf_builds::{Machine, Verdict, artifacts, detect_recipe, estimate};
use vf_git::Repo;
use vf_journal::{JournalStore as _, Vcs as _};
use vf_protocol::http::{JournalEntry, RollbackResponse};
use vf_protocol::ws::ServerMessage;
use vf_protocol::{BackendError, Cancel};
use vf_providers::ChildEnv;
use vf_providers::process::{self, Stream};

use crate::Engine;
use crate::step::now_rfc3339;

/// Longest build.
const BUILD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// A step as one line, the way the person reads it: the program, then its arguments.
pub(crate) fn command_line(step: &vf_builds::Step) -> String {
    std::iter::once(step.program.as_str())
        .chain(step.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Engine {
    pub(crate) fn run_build(
        &self,
        project: &str,
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) {
        let error = |out: &mut dyn FnMut(ServerMessage), e: String, code: &str| {
            out(ServerMessage::Error {
                error: e,
                code: Some(code.into()),
            })
        };
        let Ok(dir) = self.projects.resolve(project) else {
            return error(out, "unknown project".into(), "not_found");
        };
        let Some(recipe) = detect_recipe(&dir) else {
            return error(
                out,
                "no known way to build this project".into(),
                "no_build_recipe",
            );
        };

        // Resources: ecosystem profile, raised by the maintainer's hint.
        let mut needs = profile(recipe.ecosystem);
        if let Some(mem) = std::fs::read_to_string(dir.join("AGENTS.md"))
            .ok()
            .as_deref()
            .and_then(agents_memory_hint)
        {
            needs.memory = needs.memory.max(mem);
        }
        let _ = std::fs::create_dir_all(&self.artifacts);
        let verdict = estimate(needs, Machine::measure(&self.artifacts));
        let (name, text) = match &verdict {
            Verdict::Ok => (
                "ok",
                format!(
                    "{} build, about {} min",
                    recipe.ecosystem.name(),
                    needs.minutes
                ),
            ),
            Verdict::Tight(r) => ("tight", r.clone()),
            Verdict::NotEnough(r) => ("not_enough", r.clone()),
        };
        out(ServerMessage::Estimate {
            verdict: name.into(),
            text: text.clone(),
        });
        if matches!(verdict, Verdict::NotEnough(_)) {
            return error(
                out,
                format!("not enough resources: {text}"),
                "not_enough_resources",
            );
        }

        let run_id = crate::step::new_run_id(&format!("build-{project}"));
        let _tmp_guard = crate::step::TmpGuard::new(&self.caches, &run_id);
        let (env, _, _) = self.child_env(&dir, true, false, Some(&run_id), out, cancel);
        let mut code = 0;
        let started = Instant::now();
        for step in &recipe.steps {
            let mut cmd = shell(&dir, &step.program, &step.args, &env);
            cmd.current_dir(&dir);
            let exit = process::run_streaming(
                cmd,
                None,
                cancel,
                Some(BUILD_TIMEOUT),
                &mut |stream, line| {
                    if stream == Stream::Stdout || !line.trim().is_empty() {
                        out(ServerMessage::BuildLog { text: line });
                    }
                },
            );
            code = match exit {
                Ok(e) if e.cancelled => return out(ServerMessage::Cancelled),
                Ok(e) => e.code,
                Err(e) => {
                    return error(
                        out,
                        format!("cannot start the build: {e}"),
                        "build_start_failed",
                    );
                }
            };
            if code != 0 {
                break;
            }
        }
        let seconds = started.elapsed().as_secs_f64();
        let (mut folder, mut files) = (None, Vec::new());
        if code == 0 {
            let head = Repo::open(&dir).head().unwrap_or_default();
            let id = format!(
                "{}-{}",
                now_rfc3339().replace([':', '-'], ""),
                head.get(..7).unwrap_or("nogit")
            );
            match artifacts::collect(&dir, &recipe.outputs, &self.artifacts, project, &id) {
                Ok((dest, f)) if !f.is_empty() => {
                    folder = Some(dest.display().to_string());
                    files = f;
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "artifacts not kept"),
            }
        }
        out(ServerMessage::BuildDone {
            code,
            artifacts: folder,
            files,
            seconds,
        });
    }

    pub(crate) fn reset_to(
        &self,
        project: &str,
        commit: &str,
    ) -> Result<RollbackResponse, BackendError> {
        let dir = self
            .projects
            .resolve(project)
            .map_err(|e| BackendError::NotFound(e.to_string()))?;
        if commit.len() < 7 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(BackendError::BadRequest("commit must be a hash".into()));
        }
        let journal = self
            .journal
            .read(project)
            .map_err(|e| BackendError::Internal(e.0))?;
        let known = journal
            .iter()
            .flat_map(|e| e.commits.iter())
            .any(|c| c.starts_with(commit) || commit.starts_with(c.as_str()));
        if !known {
            return Err(BackendError::NotFound(
                "not a checkpoint of this project".into(),
            ));
        }
        let repo = Repo::open(&dir);
        repo.reset_hard(commit)
            .map_err(|e| BackendError::Internal(e.0))?;
        let head = repo.head().map_err(|e| BackendError::Internal(e.0))?;
        let entry = JournalEntry {
            at: now_rfc3339(),
            provider: crate::rollback::ROLLBACK.into(),
            model: String::new(),
            prompt: format!("roll back to {commit}"),
            session: None,
            handoff: false,
            summary: format!(
                "The project was reset to checkpoint {commit}; later changes were dropped."
            ),
            commit: None,
            commits: Vec::new(),
            files: String::new(),
            ms: None,
            cost_usd: None,
            usage: None,
            prompt_version: None,
            artifacts: Vec::new(),
            cli_config: Vec::new(),
        };
        self.journal
            .append(project, &entry)
            .map_err(|e| BackendError::Internal(e.0))?;
        Ok(RollbackResponse {
            project: project.to_owned(),
            commit: head,
        })
    }
}

/// The build command through the system shell: `cmd` on Windows (wrappers and `.cmd` tools; a
/// wrapper in the project is called by full path, the current folder may not be searched),
/// `/bin/sh` elsewhere.
#[cfg(windows)]
pub(crate) fn shell(
    dir: &Path,
    program: &str,
    args: &[String],
    env: &ChildEnv,
) -> std::process::Command {
    use std::os::windows::process::CommandExt as _;
    let first = if dir.join(program).is_file() {
        format!("\"{}\"", dir.join(program).display())
    } else {
        program.to_owned()
    };
    let line = std::iter::once(first)
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    let mut c = env.command(Path::new("cmd.exe"));
    c.arg("/d")
        .arg("/s")
        .arg("/c")
        .raw_arg(format!("\"{line}\""));
    c
}

#[cfg(not(windows))]
pub(crate) fn shell(
    _dir: &Path,
    program: &str,
    args: &[String],
    env: &ChildEnv,
) -> std::process::Command {
    let line = std::iter::once(program.to_owned())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    let mut c = env.command(Path::new("/bin/sh"));
    c.args(["-c", &line]);
    c
}
