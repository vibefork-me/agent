//! Report generation and formatting for evaluation runs (AI-EVAL.md §7).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::stages::{EvalStage, StageRunResult};

/// Aggregated metrics for a single stage across repetitions and projects.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct StageSummary {
    /// Number of runs.
    pub(crate) total: usize,
    /// Number of runs that passed schema format check.
    pub(crate) format_passed: usize,
    /// Number of runs that passed functional success check.
    pub(crate) success_passed: usize,
    /// Number of errors or timeouts.
    pub(crate) refusals: usize,
    /// Median latency in milliseconds.
    pub(crate) median_latency_ms: u64,
    /// 90th percentile latency in milliseconds.
    pub(crate) p90_latency_ms: u64,
    /// Accumulated cost in USD.
    pub(crate) total_cost_usd: f64,
}

impl StageSummary {
    /// Format pass rate percentage (0–100).
    pub(crate) fn format_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.format_passed as f64 / self.total as f64) * 100.0
        }
    }

    /// Functional success pass rate percentage (0–100).
    pub(crate) fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.success_passed as f64 / self.total as f64) * 100.0
        }
    }

    /// Refusal / error rate percentage (0–100).
    pub(crate) fn refusal_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.refusals as f64 / self.total as f64) * 100.0
        }
    }
}

/// A complete evaluation report across stages, projects, and repetitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EvalReport {
    /// Provider ID (`claude`, `codex`, `gemini`, `openrouter`, `mock`).
    pub(crate) provider: String,
    /// Model name.
    pub(crate) model: String,
    /// Timestamp (ISO 8601).
    pub(crate) timestamp: String,
    /// Embedded prompt versions at evaluation time.
    pub(crate) prompt_versions: BTreeMap<String, u32>,
    /// Summary table by stage.
    pub(crate) summaries: BTreeMap<EvalStage, StageSummary>,
    /// Raw run results.
    pub(crate) runs: Vec<StageRunResult>,
}

impl EvalReport {
    /// Computes summary statistics from a list of runs.
    pub(crate) fn new(
        provider: &str,
        model: &str,
        prompt_versions: BTreeMap<String, u32>,
        runs: Vec<StageRunResult>,
    ) -> Self {
        let mut summaries = BTreeMap::new();
        for stage in EvalStage::ALL {
            let stage_runs: Vec<&StageRunResult> =
                runs.iter().filter(|r| r.stage == stage).collect();
            if stage_runs.is_empty() {
                continue;
            }
            let total = stage_runs.len();
            let format_passed = stage_runs.iter().filter(|r| r.format_ok).count();
            let success_passed = stage_runs.iter().filter(|r| r.success).count();
            let refusals = stage_runs.iter().filter(|r| r.error.is_some()).count();

            let mut latencies: Vec<u64> = stage_runs.iter().map(|r| r.latency_ms).collect();
            latencies.sort_unstable();

            let median_latency_ms = if latencies.is_empty() {
                0
            } else {
                latencies[latencies.len() / 2]
            };
            let p90_idx = ((latencies.len() as f64) * 0.9)
                .min(latencies.len().saturating_sub(1) as f64) as usize;
            let p90_latency_ms = latencies.get(p90_idx).copied().unwrap_or(0);

            let total_cost_usd: f64 = stage_runs.iter().filter_map(|r| r.cost_usd).sum();

            summaries.insert(
                stage,
                StageSummary {
                    total,
                    format_passed,
                    success_passed,
                    refusals,
                    median_latency_ms,
                    p90_latency_ms,
                    total_cost_usd,
                },
            );
        }

        Self {
            provider: provider.into(),
            model: model.into(),
            timestamp: chrono_iso8601_now(),
            prompt_versions,
            summaries,
            runs,
        }
    }

    /// Formats the report as Markdown according to AI-EVAL.md §7.
    pub(crate) fn to_markdown(&self) -> String {
        let versions: Vec<String> = self
            .prompt_versions
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect();
        let versions_str = if versions.is_empty() {
            "default".into()
        } else {
            versions.join(", ")
        };

        let mut md = String::new();
        md.push_str(&format!(
            "Кандидат: {} / {}, {}, промпты: {}\n\n",
            self.provider, self.model, self.timestamp, versions_str
        ));
        md.push_str("| Этап      | Формат | Успех | Медиана, с | P90, с | Отказы |\n");
        md.push_str("|---|---|---|---|---|---|\n");

        for stage in EvalStage::ALL {
            if let Some(s) = self.summaries.get(&stage) {
                let stage_name = stage.as_str();
                let format_col = format!("{:.0} %", s.format_rate());
                let success_col = if stage == EvalStage::Explain {
                    format!("{:.0} % (0 правок)", s.success_rate())
                } else {
                    format!("{:.0} %", s.success_rate())
                };
                let med_sec = (s.median_latency_ms as f64) / 1000.0;
                let p90_sec = (s.p90_latency_ms as f64) / 1000.0;
                let refusal_col = format!("{:.0} %", s.refusal_rate());

                md.push_str(&format!(
                    "| {:<9} | {:<6} | {:<5} | {:<10.2} | {:<6.2} | {:<6} |\n",
                    stage_name, format_col, success_col, med_sec, p90_sec, refusal_col
                ));
            }
        }

        let total_cost: f64 = self.summaries.values().map(|s| s.total_cost_usd).sum();
        md.push_str(&format!("\nСтоимость на прогон: ${total_cost:.4}\n"));
        md
    }

    /// Serializes report as pretty-printed JSON.
    pub(crate) fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Compares two evaluation reports (e.g. two prompt versions or models).
    pub(crate) fn compare(a: &EvalReport, b: &EvalReport) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "### Сравнение: {} / {} против {} / {}\n\n",
            a.provider, a.model, b.provider, b.model
        ));
        out.push_str("| Этап | Успех A | Успех B | Дельта | Медиана A, с | Медиана B, с |\n");
        out.push_str("|---|---|---|---|---|---|\n");

        for stage in EvalStage::ALL {
            let sa = a.summaries.get(&stage);
            let sb = b.summaries.get(&stage);
            let rate_a = sa.map_or(0.0, |s| s.success_rate());
            let rate_b = sb.map_or(0.0, |s| s.success_rate());
            let delta = rate_b - rate_a;
            let med_a = sa.map_or(0.0, |s| s.median_latency_ms as f64 / 1000.0);
            let med_b = sb.map_or(0.0, |s| s.median_latency_ms as f64 / 1000.0);

            out.push_str(&format!(
                "| {:<7} | {:.0} % | {:.0} % | {:+.0} % | {:.2}s | {:.2}s |\n",
                stage.as_str(),
                rate_a,
                rate_b,
                delta,
                med_a,
                med_b
            ));
        }
        out
    }
}

fn chrono_iso8601_now() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    format!("epoch-{}", secs)
}
