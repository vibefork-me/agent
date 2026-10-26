//! Automated evaluation runners and validators for stages (AI-EVAL.md §2, §4).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use vf_protocol::Cancel;
use vf_providers::{AskRequest, ChildEnv, Output, Provider, RunRequest};

use crate::fixtures::FixtureProject;

/// Stages evaluated by vibefork-eval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EvalStage {
    /// Overview project map.
    Map,
    /// Task difficulty classification.
    Classify,
    /// Task plan decomposition.
    Plan,
    /// Active coding loop with tool execution.
    Loop,
    /// Plain non-technical explanation to user.
    Audience,
    /// Read-only explanation with strictly zero file changes.
    Explain,
}

impl EvalStage {
    /// All stages in order.
    pub(crate) const ALL: [EvalStage; 6] = [
        EvalStage::Map,
        EvalStage::Classify,
        EvalStage::Plan,
        EvalStage::Loop,
        EvalStage::Audience,
        EvalStage::Explain,
    ];

    /// Lowercase stage identifier.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Classify => "classify",
            Self::Plan => "plan",
            Self::Loop => "loop",
            Self::Audience => "audience",
            Self::Explain => "explain",
        }
    }

    /// Parses stage from string.
    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "map" => Some(Self::Map),
            "classify" => Some(Self::Classify),
            "plan" => Some(Self::Plan),
            "loop" => Some(Self::Loop),
            "audience" => Some(Self::Audience),
            "explain" => Some(Self::Explain),
            _ => None,
        }
    }
}

/// Result of a single evaluation run on a stage and project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StageRunResult {
    /// Stage name.
    pub(crate) stage: EvalStage,
    /// Project identifier.
    pub(crate) project: String,
    /// Whether output conformed to required format/schema.
    pub(crate) format_ok: bool,
    /// Whether the stage succeeded its automated checks.
    pub(crate) success: bool,
    /// Execution duration in milliseconds.
    pub(crate) latency_ms: u64,
    /// Cost reported by model in USD, if available.
    pub(crate) cost_usd: Option<f64>,
    /// Raw answer text (clipped to 1000 characters).
    pub(crate) raw_response: String,
    /// Error message, if failed.
    pub(crate) error: Option<String>,
}

/// Takes a snapshot of all file contents in `dir` to detect modifications.
fn snapshot_dir(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut map = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.is_file()
                    && let Ok(rel) = path.strip_prefix(dir)
                    && let Ok(bytes) = fs::read(&path)
                {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    map.insert(rel_str, bytes);
                }
            }
        }
    }
    map
}

/// Runs evaluation of a single stage on a materialized project folder.
pub(crate) fn run_stage(
    stage: EvalStage,
    provider: &dyn Provider,
    model: &str,
    project: &FixtureProject,
    work_dir: &Path,
) -> StageRunResult {
    let env = ChildEnv::from_current();
    let cancel = Cancel::new();
    let files = project.file_paths();
    let readme = project.readme();
    let t0 = Instant::now();

    match stage {
        EvalStage::Map => {
            let prompt_text = format!(
                "Write the names and summaries in the language with code `ru`.\n\nPROJECT DATA\n\nFiles:\n{}\n\nREADME:\n{}",
                files.join("\n"),
                readme
            );
            let sys = vf_core::prompts::get(vf_core::prompts::Stage::Map);
            let req = AskRequest {
                dir: work_dir,
                system: Some(&sys.template),
                prompt: &prompt_text,
                model,
                env: &env,
                max_tokens: Some(3000),
                timeout: std::time::Duration::from_secs(60),
            };

            match provider.ask(&req, &cancel) {
                Ok(ans) => {
                    let elapsed = t0.elapsed().as_millis() as u64;
                    let (format_ok, success, err) =
                        match serde_json::from_str::<serde_json::Value>(&ans.text) {
                            Ok(v) => {
                                let parts = v["parts"].as_array();
                                let links = v["links"].as_array();
                                if let (Some(p), Some(_)) = (parts, links) {
                                    if p.len() >= 2 && p.len() <= 12 {
                                        (true, true, None)
                                    } else {
                                        (
                                            true,
                                            false,
                                            Some(format!("Invalid parts count: {}", p.len())),
                                        )
                                    }
                                } else {
                                    (
                                        false,
                                        false,
                                        Some("Missing 'parts' or 'links' JSON arrays".into()),
                                    )
                                }
                            }
                            Err(e) => (false, false, Some(format!("JSON parse error: {e}"))),
                        };
                    StageRunResult {
                        stage,
                        project: project.id.into(),
                        format_ok,
                        success,
                        latency_ms: elapsed,
                        cost_usd: ans.cost_usd,
                        raw_response: ans.text.chars().take(1000).collect(),
                        error: err,
                    }
                }
                Err(e) => StageRunResult {
                    stage,
                    project: project.id.into(),
                    format_ok: false,
                    success: false,
                    latency_ms: t0.elapsed().as_millis() as u64,
                    cost_usd: None,
                    raw_response: String::new(),
                    error: Some(e),
                },
            }
        }
        EvalStage::Classify => {
            let sys = vf_core::prompts::get(vf_core::prompts::Stage::Classify);
            let req = AskRequest {
                dir: work_dir,
                system: Some(&sys.template),
                prompt: project.task_prompt,
                model,
                env: &env,
                max_tokens: Some(500),
                timeout: std::time::Duration::from_secs(30),
            };

            match provider.ask(&req, &cancel) {
                Ok(ans) => {
                    let elapsed = t0.elapsed().as_millis() as u64;
                    let (format_ok, success, err) =
                        match vf_orchestrator::parse_rating(&ans.text, "eval") {
                            Some(rating) => (
                                true,
                                matches!(rating.complexity.as_str(), "simple" | "medium" | "hard"),
                                None,
                            ),
                            None => (
                                false,
                                false,
                                Some("Invalid classification JSON output".into()),
                            ),
                        };
                    StageRunResult {
                        stage,
                        project: project.id.into(),
                        format_ok,
                        success,
                        latency_ms: elapsed,
                        cost_usd: ans.cost_usd,
                        raw_response: ans.text.chars().take(1000).collect(),
                        error: err,
                    }
                }
                Err(e) => StageRunResult {
                    stage,
                    project: project.id.into(),
                    format_ok: false,
                    success: false,
                    latency_ms: t0.elapsed().as_millis() as u64,
                    cost_usd: None,
                    raw_response: String::new(),
                    error: Some(e),
                },
            }
        }
        EvalStage::Plan => {
            let sys = vf_core::prompts::get(vf_core::prompts::Stage::Plan);
            let prompt_text = format!(
                "Task: {}\n\nFiles:\n{}\n\nREADME:\n{}",
                project.task_prompt,
                files.join("\n"),
                readme
            );
            let req = AskRequest {
                dir: work_dir,
                system: Some(&sys.template),
                prompt: &prompt_text,
                model,
                env: &env,
                max_tokens: Some(2000),
                timeout: std::time::Duration::from_secs(60),
            };

            match provider.ask(&req, &cancel) {
                Ok(ans) => {
                    let elapsed = t0.elapsed().as_millis() as u64;
                    let (format_ok, success, err) = match vf_orchestrator::parse_plan(&ans.text) {
                        Ok(steps) => {
                            if steps.len() >= 2 && steps.len() <= 5 {
                                (true, true, None)
                            } else {
                                (
                                    true,
                                    false,
                                    Some(format!("Step count {} not in 2..=5", steps.len())),
                                )
                            }
                        }
                        Err(e) => (false, false, Some(format!("Plan parse error: {e}"))),
                    };
                    StageRunResult {
                        stage,
                        project: project.id.into(),
                        format_ok,
                        success,
                        latency_ms: elapsed,
                        cost_usd: ans.cost_usd,
                        raw_response: ans.text.chars().take(1000).collect(),
                        error: err,
                    }
                }
                Err(e) => StageRunResult {
                    stage,
                    project: project.id.into(),
                    format_ok: false,
                    success: false,
                    latency_ms: t0.elapsed().as_millis() as u64,
                    cost_usd: None,
                    raw_response: String::new(),
                    error: Some(e),
                },
            }
        }
        EvalStage::Loop => {
            let sys = vf_core::prompts::get(vf_core::prompts::Stage::Loop);
            let mut events = Vec::new();
            let mut sink = |out: Output| {
                if let Output::Event(ev) = out {
                    events.push(ev);
                }
            };
            let req = RunRequest {
                dir: work_dir,
                prompt: project.task_prompt,
                model,
                resume: None,
                env: &env,
                system: Some(&sys.template),
            };

            let res = provider.run(&req, &mut sink, &cancel);
            let elapsed = t0.elapsed().as_millis() as u64;
            let success = res.code == 0 && res.ok;
            StageRunResult {
                stage,
                project: project.id.into(),
                format_ok: true,
                success,
                latency_ms: elapsed,
                cost_usd: res.cost_usd,
                raw_response: format!("exit_code={}, events={}", res.code, events.len()),
                error: if success {
                    None
                } else {
                    Some(format!("Loop failed with exit code {}", res.code))
                },
            }
        }
        EvalStage::Audience => {
            let sys = vf_core::catalog::system_instruction("ru");
            let prompt_text = "The task was completed. Please summarize the outcome in 2-3 clear sentences for someone who does not code.";
            let req = AskRequest {
                dir: work_dir,
                system: Some(&sys),
                prompt: prompt_text,
                model,
                env: &env,
                max_tokens: Some(1000),
                timeout: std::time::Duration::from_secs(30),
            };

            match provider.ask(&req, &cancel) {
                Ok(ans) => {
                    let elapsed = t0.elapsed().as_millis() as u64;
                    let has_code_fence = ans.text.contains("```");
                    let non_empty = !ans.text.trim().is_empty();
                    let success = non_empty && !has_code_fence;
                    StageRunResult {
                        stage,
                        project: project.id.into(),
                        format_ok: true,
                        success,
                        latency_ms: elapsed,
                        cost_usd: ans.cost_usd,
                        raw_response: ans.text.chars().take(1000).collect(),
                        error: if has_code_fence {
                            Some("Contains markdown code fence in audience response".into())
                        } else if !non_empty {
                            Some("Empty response".into())
                        } else {
                            None
                        },
                    }
                }
                Err(e) => StageRunResult {
                    stage,
                    project: project.id.into(),
                    format_ok: false,
                    success: false,
                    latency_ms: t0.elapsed().as_millis() as u64,
                    cost_usd: None,
                    raw_response: String::new(),
                    error: Some(e),
                },
            }
        }
        EvalStage::Explain => {
            // Crucial: 0 files modified
            let before = snapshot_dir(work_dir);
            let mut sink = |_| {};
            let req = RunRequest {
                dir: work_dir,
                prompt: project.explain_prompt,
                model,
                resume: None,
                env: &env,
                system: None,
            };

            let res = provider.run(&req, &mut sink, &cancel);
            let after = snapshot_dir(work_dir);
            let elapsed = t0.elapsed().as_millis() as u64;

            let modified = before != after;
            let success = res.code == 0 && res.ok && !modified;
            StageRunResult {
                stage,
                project: project.id.into(),
                format_ok: true,
                success,
                latency_ms: elapsed,
                cost_usd: res.cost_usd,
                raw_response: format!("exit_code={}, files_modified={}", res.code, modified),
                error: if modified {
                    Some("FATAL: Files were modified during read-only explain stage".into())
                } else if res.code != 0 || !res.ok {
                    Some(format!("Run failed with exit code {}", res.code))
                } else {
                    None
                },
            }
        }
    }
}
