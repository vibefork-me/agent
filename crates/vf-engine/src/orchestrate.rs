//! Auto and Plan on top of [`vf_orchestrator`]: the calls to the classifier and the planner, and the
//! steps of a plan.

use std::path::Path;
use std::time::Duration;

use vf_core::catalog::{self, Language};
use vf_git::Repo;
use vf_journal::{JournalStore as _, Vcs as _};
use vf_orchestrator::{
    Complexity, PlanStep, classifier_candidates, heuristic_rating, model_for_step, parse_plan,
    parse_rating, pick_model, plan_preamble, plan_prompt, wants_plan,
};
use vf_protocol::canvas::CanvasEvent;
use vf_protocol::http::JournalEntry;
use vf_protocol::ws::{PlanStepInfo, Rating, ServerMessage, StepInfo};
use vf_protocol::{Cancel, ModelInfo};
use vf_providers::{AskRequest, ChildEnv};

use crate::notes::Inbox;
use crate::step::now_rfc3339;
use crate::{Engine, MAX_DIFF, StepRequest};

const CLASSIFIER_TIMEOUT: Duration = Duration::from_secs(20);
const PLANNER_TIMEOUT: Duration = Duration::from_secs(600);
const API_PLANNER_TIMEOUT: Duration = Duration::from_secs(90);

/// A validated task.
pub(crate) struct Task<'a> {
    pub(crate) project: &'a str,
    pub(crate) dir: &'a Path,
    pub(crate) repo: &'a Repo,
    pub(crate) prompt: &'a str,
    pub(crate) continue_: bool,
    pub(crate) lang: &'a str,
    /// Messages of the person taken during the task.
    pub(crate) inbox: &'a Inbox,
}

impl Engine {
    /// Auto: rate the task, then one model or a plan.
    pub(crate) fn auto(
        &self,
        t: &Task<'_>,
        inventory: &[ModelInfo],
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) {
        let rating = self.classify(t, inventory, out, cancel);
        if wants_plan(&rating) {
            return self.plan(t, inventory, Some(rating), out, cancel);
        }
        let complexity = Complexity::parse(&rating.complexity).unwrap_or(Complexity::Medium);
        let Some(choice) = pick_model(inventory, complexity, &[], &[]) else {
            return out(ServerMessage::Error {
                error: "no models available".into(),
                code: Some("no_models".into()),
            });
        };
        out(ServerMessage::Routing {
            rating,
            choice: choice.clone(),
        });
        self.single(t, &choice.provider, &choice.model, out, cancel);
    }

    /// The rating by a free classifier model; a heuristic when none answers.
    fn classify(
        &self,
        t: &Task<'_>,
        inventory: &[ModelInfo],
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) -> Rating {
        let free: Vec<String> = inventory
            .iter()
            .filter(|m| m.provider == "openrouter")
            .map(|m| m.model.clone())
            .collect();
        if let Some(p) = self.provider("openrouter") {
            let env = ChildEnv::from_current();
            let prompt: String = t.prompt.chars().take(4000).collect();
            let classifier_prompt = vf_platform::prompts::get(vf_core::prompts::Stage::Classify);
            let candidates = classifier_candidates(&free);
            if !candidates.is_empty() {
                think(out, Thought::Rating, t.lang);
            }
            for model in candidates {
                let req = AskRequest {
                    dir: t.dir,
                    system: Some(&classifier_prompt.template),
                    prompt: &prompt,
                    model: &model,
                    env: &env,
                    max_tokens: Some(200),
                    timeout: CLASSIFIER_TIMEOUT,
                };
                match p.ask(&req, cancel) {
                    Ok(a) => match parse_rating(&a.text, &model) {
                        Some(r) => return r,
                        None => tracing::info!(%model, "classifier answer not understood"),
                    },
                    Err(e) => tracing::info!(%model, error = %e, "classifier failed"),
                }
            }
        }
        heuristic_rating(t.prompt)
    }

    /// Plan: a strong model splits the task, each step runs on the cheapest suitable model.
    pub(crate) fn plan(
        &self,
        t: &Task<'_>,
        inventory: &[ModelInfo],
        rating: Option<Rating>,
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) {
        let start_head = match t.repo.head() {
            Ok(h) => h,
            Err(e) => {
                return out(ServerMessage::Error {
                    error: format!("git: {e}"),
                    code: Some("git_failed".into()),
                });
            }
        };
        out(ServerMessage::Planning { rating });
        // Paused while planning: plan again once resumed.
        let planned = loop {
            t.inbox.pick_up(cancel, out);
            think(out, Thought::Planning, t.lang);
            let planned = self.make_plan(t, inventory, cancel);
            if planned.is_ok() || !cancel.is_paused() {
                break planned;
            }
            out(ServerMessage::Paused);
            if !cancel.wait_resumed() {
                return out(ServerMessage::Cancelled);
            }
            out(ServerMessage::Resumed);
        };
        let (planner, cost_usd, steps) = match planned {
            Ok(p) => p,
            Err(e) => {
                out(ServerMessage::Error {
                    error: format!("planning failed: {e}"),
                    code: Some("planning_failed".into()),
                });
                return done(out, 1, None, String::new(), String::new(), true);
            }
        };
        let models: Vec<Option<ModelInfo>> =
            steps.iter().map(|s| model_for_step(inventory, s)).collect();
        out(ServerMessage::Plan {
            planner: planner.clone(),
            cost_usd,
            steps: steps
                .iter()
                .zip(&models)
                .map(|(s, m)| PlanStepInfo {
                    title: s.title.clone(),
                    complexity: s.complexity.as_str().into(),
                    label: m
                        .as_ref()
                        .map(|m| m.label.clone())
                        .unwrap_or_else(|| "no model".into()),
                })
                .collect(),
        });
        let summary = steps
            .iter()
            .zip(&models)
            .enumerate()
            .map(|(i, (s, m))| {
                let label = m.as_ref().map(|m| m.label.as_str()).unwrap_or("no model");
                format!(
                    "{}. [{} → {label}] {}",
                    i + 1,
                    s.complexity.as_str(),
                    s.title
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let entry = JournalEntry {
            at: now_rfc3339(),
            provider: "planner".into(),
            model: planner,
            prompt: t.prompt.to_owned(),
            session: None,
            handoff: false,
            summary,
            commit: None,
            commits: Vec::new(),
            files: String::new(),
            ms: None,
            cost_usd,
            usage: None,
            prompt_version: Some(vf_platform::prompts::get(vf_core::prompts::Stage::Plan).tag()),
            artifacts: Vec::new(),
        };
        if let Err(e) = self.journal.append(t.project, &entry) {
            tracing::warn!(error = %e, "plan not journaled");
        }

        let of = steps.len() as u32;
        let mut last = None;
        let mut code = 0;
        for (i, (s, m)) in steps.iter().zip(&models).enumerate() {
            if cancel.is_cancelled() {
                break;
            }
            let n = i as u32 + 1;
            let Some(m) = m else {
                out(ServerMessage::Error {
                    error: format!("no model for step {n}"),
                    code: Some("no_model_for_step".into()),
                });
                code = 1;
                break;
            };
            let Some(p) = self.provider(&m.provider).cloned() else {
                code = 1;
                break;
            };
            let info = StepInfo {
                n,
                of,
                title: s.title.clone(),
            };
            think(out, Thought::Step(&info), t.lang);
            let prefix = plan_preamble(t.prompt, &steps, i);
            let system_instruction = vf_core::catalog::system_instruction(t.lang);
            let req = StepRequest {
                project: t.project,
                dir: t.dir,
                prompt: &s.prompt,
                prefix: &prefix,
                notes: "",
                model: &m.model,
                continue_: t.continue_ || i > 0,
                step: Some(&info),
                system: Some(&system_instruction),
            };
            let r = self.step_noted(t.repo, p.as_ref(), &req, Some(t.inbox), out, cancel);
            out(ServerMessage::StepDone {
                n,
                of,
                ok: r.ok,
                commit: r.commit.clone(),
            });
            code = r.code;
            let failed = !r.ok;
            last = Some((m.clone(), r));
            if failed {
                if !cancel.is_cancelled() {
                    out(ServerMessage::Error {
                        error: format!("step {n} failed — stopping the plan"),
                        code: Some("plan_stopped".into()),
                    });
                    code = if code == 0 { 1 } else { code };
                }
                break;
            }
        }
        if cancel.is_cancelled() {
            return out(ServerMessage::Cancelled);
        }
        // The diff of the whole plan, from where it started.
        let diff = t.repo.diff_from(&start_head).unwrap_or_default();
        let status = t.repo.diff_stat(&start_head, "HEAD").unwrap_or_default();
        let (model, session, commit) = match last {
            Some((m, r)) => (Some(m), r.session, r.commit),
            None => (None, None, None),
        };
        out(ServerMessage::Done {
            code,
            provider: model.as_ref().map(|m| m.provider.clone()),
            model: model.map(|m| m.model),
            session_id: session,
            commit,
            diff: crate::step::clip_bytes(diff, MAX_DIFF),
            status,
            plan: true,
        });
    }

    /// Planner preference (subscriptions and free only): Claude opus → Codex default → a free model.
    fn make_plan(
        &self,
        t: &Task<'_>,
        inventory: &[ModelInfo],
        cancel: &Cancel,
    ) -> Result<(String, Option<f64>, Vec<PlanStep>), String> {
        let has = |p: &str, m: Option<&str>| {
            inventory
                .iter()
                .any(|x| x.provider == p && m.is_none_or(|m| x.model == m))
        };
        let mut candidates: Vec<(&str, String, Duration)> = Vec::new();
        if has("claude", Some("opus")) {
            candidates.push(("claude", "opus".into(), PLANNER_TIMEOUT));
        }
        if has("codex", None) {
            candidates.push(("codex", String::new(), PLANNER_TIMEOUT));
        }
        if let Some(custom) = inventory.iter().find(|m| m.provider == "custom") {
            candidates.push(("custom", custom.model.clone(), API_PLANNER_TIMEOUT));
        }
        if let Some(gemini) = inventory.iter().find(|m| m.provider == "gemini") {
            candidates.push(("gemini", gemini.model.clone(), API_PLANNER_TIMEOUT));
        }
        if let Some(free) = inventory.iter().find(|m| m.provider == "openrouter") {
            candidates.push(("openrouter", free.model.clone(), API_PLANNER_TIMEOUT));
        }
        let env = ChildEnv::from_current();
        let prompt = plan_prompt(&format!("{}{}", t.prompt, t.inbox.addendum()));
        let mut tried = Vec::new();
        for (provider, model, timeout) in candidates {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            let Some(p) = self.provider(provider) else {
                continue;
            };
            let req = AskRequest {
                dir: t.dir,
                system: None,
                prompt: &prompt,
                model: &model,
                env: &env,
                max_tokens: None,
                timeout,
            };
            let name = format!(
                "{provider}/{}",
                if model.is_empty() { "default" } else { &model }
            );
            match p
                .ask(&req, cancel)
                .and_then(|a| parse_plan(&a.text).map(|s| (a.cost_usd, s)))
            {
                Ok((cost, steps)) => return Ok((name, cost, steps)),
                Err(e) => tried.push(format!("{name}: {e}")),
            }
        }
        Err(if tried.is_empty() {
            "no planner available".into()
        } else {
            tried.join("; ")
        })
    }
}

/// What the agent waits for a model to do during a task.
pub(crate) enum Thought<'a> {
    /// Rate how big the task is.
    Rating,
    /// Split the task into steps.
    Planning,
    /// Do the whole task in one go.
    Task,
    /// Do one step of a plan.
    Step(&'a StepInfo),
}

/// What the agent is doing while it waits for a model, in the person's language (no call to a model).
pub(crate) fn thought(what: &Thought<'_>, lang: &str) -> String {
    let lang = Language::parse(lang);
    match what {
        Thought::Rating => catalog::thought_rating(lang),
        Thought::Planning => catalog::thought_planning(lang),
        Thought::Task => catalog::thought_task(lang),
        Thought::Step(s) => catalog::thought_step(lang, s.n, s.of, &s.title),
    }
}

/// Sends a `think` event before a call to a model (a canvas event raised by the work; the session
/// stream numbers it). It is about the whole project: a task's steps are not tied to a part.
pub(crate) fn think(out: &mut dyn FnMut(ServerMessage), what: Thought<'_>, lang: &str) {
    out(ServerMessage::Canvas {
        project: String::new(),
        seq: 0,
        at: 0,
        session: String::new(),
        ev: CanvasEvent::Think {
            id: crate::map::ROOT.into(),
            text: thought(&what, lang),
        },
    });
}

fn done(
    out: &mut dyn FnMut(ServerMessage),
    code: i32,
    commit: Option<String>,
    diff: String,
    status: String,
    plan: bool,
) {
    out(ServerMessage::Done {
        code,
        provider: None,
        model: None,
        session_id: None,
        commit,
        diff,
        status,
        plan,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thoughts_are_in_the_persons_language() {
        let step = StepInfo {
            n: 2,
            of: 3,
            title: "Add tags".into(),
        };
        assert_eq!(
            thought(&Thought::Step(&step), "ru-RU"),
            "Шаг 2 из 3: Add tags"
        );
        assert_eq!(
            thought(&Thought::Step(&step), "en"),
            "Step 2 of 3: Add tags"
        );
        assert_eq!(thought(&Thought::Planning, "ru"), "Разбиваю задачу на шаги");
        assert_eq!(
            thought(&Thought::Rating, "??"),
            "Working out how big the task is"
        );
        assert_eq!(thought(&Thought::Task, "en"), "Working on the task");
    }
}
