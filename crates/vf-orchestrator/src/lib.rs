//! Auto / Plan / Continue — model orchestration. **Layer 1: no IO**: prompts, parsing of model
//! answers and the choice of a model; the engine makes the calls.
//!
//! - **Auto** rates the task (a free classifier model, else a heuristic) and picks the cheapest
//!   suitable model ([`pick_model`]); a task that splits well goes to Plan.
//! - **Plan**: a strong model reads the project read-only and splits the task into 2–5 steps
//!   ([`plan_prompt`], [`parse_plan`]); each step goes to the cheapest model for its complexity, free
//!   models first for simple steps.
//! - **Continue** (native resume or a handoff from the journal) is decided per step by the engine.

use vf_protocol::ModelInfo;
use vf_protocol::ws::Rating;

/// Free classifier models tried first, if available.
pub const CLASSIFIER_PREFERENCE: &[&str] = &[
    "nvidia/nemotron-3.5-lightning:free",
    "google/gemma-4-26b-a4b-it:free",
    "minimax/minimax-m3.1-flash:free",
];
/// How many classifier models are tried.
pub const CLASSIFIER_TRIES: usize = 3;
/// Most steps of a plan.
pub const MAX_STEPS: usize = 5;

/// How hard a task or step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Complexity {
    /// Small local change in one or two files.
    Simple,
    /// Several files or some design.
    Medium,
    /// Architecture, tricky debugging, large refactor.
    Hard,
}

impl Complexity {
    /// Parses `simple` / `medium` / `hard`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "simple" => Some(Self::Simple),
            "medium" => Some(Self::Medium),
            "hard" => Some(Self::Hard),
            _ => None,
        }
    }

    /// The wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Medium => "medium",
            Self::Hard => "hard",
        }
    }

    /// Model tier for this complexity (1 light … 3 strong).
    pub fn tier(self) -> u8 {
        match self {
            Self::Simple => 1,
            Self::Medium => 2,
            Self::Hard => 3,
        }
    }
}

// ---------- Auto ----------

/// System prompt of the classifier.
pub fn classifier_system() -> String {
    vf_core::prompts::get(vf_core::prompts::Stage::Classify).template
}

/// Constant fallback system prompt of the classifier.
pub const CLASSIFIER_SYSTEM: &str = "You rate coding tasks for routing. Reply with JSON only: \
{\"complexity\":\"simple\"|\"medium\"|\"hard\",\"decompose\":true|false,\"reason\":\"<one short sentence>\"}. \
simple = small local change in one or two files; medium = several files or some design; \
hard = architecture, tricky debugging, large refactor.";

/// Classifier models to try, in order, from the free models available.
pub fn classifier_candidates(free_models: &[String]) -> Vec<String> {
    let mut out: Vec<String> = CLASSIFIER_PREFERENCE
        .iter()
        .filter(|m| free_models.iter().any(|f| f == *m))
        .map(|m| (*m).to_owned())
        .collect();
    for m in free_models {
        if !out.contains(m) {
            out.push(m.clone());
        }
    }
    out.truncate(CLASSIFIER_TRIES);
    out
}

/// The JSON object inside a model answer (models add prose or code fences around it).
fn json_object(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

/// Parses a classifier answer.
pub fn parse_rating(answer: &str, classifier: &str) -> Option<Rating> {
    let v = json_object(answer)?;
    let complexity = Complexity::parse(v["complexity"].as_str()?)?;
    Some(Rating {
        complexity: complexity.as_str().into(),
        decompose: v["decompose"].as_bool().unwrap_or(false),
        reason: v["reason"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect(),
        classifier: classifier.into(),
    })
}

/// A rating without a classifier: by prompt length.
pub fn heuristic_rating(prompt: &str) -> Rating {
    let n = prompt.chars().count();
    let complexity = if n < 300 {
        Complexity::Simple
    } else if n < 1200 {
        Complexity::Medium
    } else {
        Complexity::Hard
    };
    Rating {
        complexity: complexity.as_str().into(),
        decompose: false,
        reason: "heuristic by prompt length (no classifier available)".into(),
        classifier: "heuristic".into(),
    }
}

/// Whether Auto should plan instead of running one model.
pub fn wants_plan(rating: &Rating) -> bool {
    rating.decompose && rating.complexity != "simple"
}

/// Preference inside a tier: subscription CLIs first (stronger agents), then free models.
fn provider_rank(provider: &str, prefer: &[&str]) -> i32 {
    if prefer.contains(&provider) {
        return -1;
    }
    match provider {
        "claude" => 0,
        "codex" => 1,
        "openrouter" => 2,
        _ => 3,
    }
}

/// The cheapest suitable model: the tier of the complexity, else one above, else one below, else
/// anything. `prefer` puts providers first (free models for simple steps).
pub fn pick_model(
    inventory: &[ModelInfo],
    complexity: Complexity,
    exclude: &[&str],
    prefer: &[&str],
) -> Option<ModelInfo> {
    let usable: Vec<&ModelInfo> = inventory
        .iter()
        .filter(|m| !exclude.contains(&m.provider.as_str()))
        .collect();
    let pick = |tier: u8| {
        usable
            .iter()
            .filter(|m| m.tier == tier)
            .min_by_key(|m| provider_rank(&m.provider, prefer))
            .map(|m| (*m).clone())
    };
    let want = complexity.tier();
    pick(want)
        .or_else(|| pick(want + 1))
        .or_else(|| want.checked_sub(1).and_then(pick))
        .or_else(|| usable.first().map(|m| (*m).clone()))
}

// ---------- Plan ----------

/// One planned step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    /// Short title.
    pub title: String,
    /// Full instruction for the model doing the step.
    pub prompt: String,
    /// Complexity.
    pub complexity: Complexity,
}

/// The planning prompt.
pub fn plan_prompt(task: &str) -> String {
    vf_core::prompts::get(vf_core::prompts::Stage::Plan).render_plan(task)
}

/// Parses a planner answer: at most [`MAX_STEPS`] steps with a prompt.
pub fn parse_plan(answer: &str) -> Result<Vec<PlanStep>, String> {
    let v = json_object(answer).ok_or("no JSON in the plan")?;
    let steps: Vec<PlanStep> = v["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let prompt = s["prompt"].as_str().filter(|p| !p.trim().is_empty())?;
            let title = s["title"]
                .as_str()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or(prompt);
            Some(PlanStep {
                title: title.chars().take(120).collect(),
                prompt: prompt.to_owned(),
                complexity: s["complexity"]
                    .as_str()
                    .and_then(Complexity::parse)
                    .unwrap_or(Complexity::Medium),
            })
        })
        .take(MAX_STEPS)
        .collect();
    if steps.is_empty() {
        Err("empty plan".into())
    } else {
        Ok(steps)
    }
}

/// The model for a plan step: free models first for simple steps.
pub fn model_for_step(inventory: &[ModelInfo], step: &PlanStep) -> Option<ModelInfo> {
    let prefer: &[&str] = if step.complexity == Complexity::Simple {
        &["openrouter"]
    } else {
        &[]
    };
    pick_model(inventory, step.complexity, &[], prefer)
}

/// Context put before the prompt of step `i`.
pub fn plan_preamble(task: &str, steps: &[PlanStep], i: usize) -> String {
    let list: Vec<String> = steps
        .iter()
        .enumerate()
        .map(|(n, s)| {
            let mark = if n < i {
                " — done"
            } else if n == i {
                " — YOUR STEP"
            } else {
                ""
            };
            format!("{}. {}{mark}", n + 1, s.title)
        })
        .collect();
    format!(
        "Overall task (split into {} steps by a planner):\n{task}\n\nPlan:\n{}\n\nDo only step {} now:\n",
        steps.len(),
        list.join("\n"),
        i + 1
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use vf_protocol::CostClass;

    fn m(provider: &str, model: &str, tier: u8) -> ModelInfo {
        ModelInfo {
            provider: provider.into(),
            model: model.into(),
            tier,
            cost: CostClass::Subscription,
            label: format!("{provider} {model}"),
        }
    }

    fn inv() -> Vec<ModelInfo> {
        vec![
            m("claude", "haiku", 1),
            m("claude", "sonnet", 2),
            m("claude", "opus", 3),
            m("codex", "luna", 1),
            m("codex", "", 3),
            m("openrouter", "x:free", 1),
        ]
    }

    #[test]
    fn picks_cheapest_suitable_model() {
        let pick = |c, ex: &[&str], pref: &[&str]| {
            pick_model(&inv(), c, ex, pref).map(|m| format!("{}/{}", m.provider, m.model))
        };
        assert_eq!(
            pick(Complexity::Simple, &[], &[]).as_deref(),
            Some("claude/haiku")
        );
        assert_eq!(
            pick(Complexity::Simple, &[], &["openrouter"]).as_deref(),
            Some("openrouter/x:free")
        );
        assert_eq!(
            pick(Complexity::Hard, &["claude"], &[]).as_deref(),
            Some("codex/")
        );
        assert_eq!(
            pick(Complexity::Medium, &["claude"], &[]).as_deref(),
            Some("codex/"),
            "one tier up"
        );
        let only_free = vec![m("openrouter", "x:free", 1)];
        assert_eq!(
            pick_model(&only_free, Complexity::Hard, &[], &[])
                .map(|m| m.model)
                .as_deref(),
            Some("x:free"),
            "anything"
        );
        assert!(pick_model(&[], Complexity::Simple, &[], &[]).is_none());
    }

    #[test]
    fn ratings() {
        let r = parse_rating(
            "Sure! ```json\n{\"complexity\":\"medium\",\"decompose\":true,\"reason\":\"two parts\"}\n```",
            "c1",
        );
        assert_eq!(
            r.as_ref().map(|r| (r.complexity.as_str(), r.decompose)),
            Some(("medium", true))
        );
        assert!(r.is_some_and(|r| wants_plan(&r)));
        assert!(parse_rating("{\"complexity\":\"tiny\"}", "c1").is_none());
        assert!(parse_rating("no json", "c1").is_none());
        assert_eq!(heuristic_rating("short").complexity, "simple");
        assert_eq!(heuristic_rating(&"x".repeat(2000)).complexity, "hard");
        let free = vec![
            "a:free".to_owned(),
            "google/gemma-4-26b-a4b-it:free".to_owned(),
            "b:free".to_owned(),
            "c:free".to_owned(),
        ];
        assert_eq!(
            classifier_candidates(&free),
            ["google/gemma-4-26b-a4b-it:free", "a:free", "b:free"]
        );
    }

    #[test]
    fn plans() {
        let text = r#"Plan: {"steps":[{"title":"A","prompt":"do a","complexity":"simple"},{"prompt":"do b","complexity":"weird"},{"title":"C","prompt":""}]}"#;
        let steps = parse_plan(text).unwrap_or_default();
        assert_eq!(steps.len(), 2);
        assert_eq!(
            (steps[1].title.as_str(), steps[1].complexity),
            ("do b", Complexity::Medium)
        );
        assert!(parse_plan(r#"{"steps":[]}"#).is_err());
        let many = format!("{{\"steps\":[{}]}}", [r#"{"prompt":"p"}"#; 9].join(","));
        assert_eq!(parse_plan(&many).map(|s| s.len()), Ok(MAX_STEPS));
        assert_eq!(
            model_for_step(&inv(), &steps[0])
                .map(|m| m.provider)
                .as_deref(),
            Some("openrouter")
        );
        let pre = plan_preamble("task", &steps, 1);
        assert!(
            pre.contains("1. A — done")
                && pre.contains("2. do b — YOUR STEP")
                && pre.ends_with("Do only step 2 now:\n")
        );
        assert!(plan_prompt("T").ends_with("Task:\nT"));
    }
}
