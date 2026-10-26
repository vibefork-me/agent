//! Deterministic mock provider for offline evaluation tests.

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use vf_protocol::{Cancel, CostClass, ModelInfo};
use vf_providers::{Answer, AskRequest, Output, Provider, RunRequest, RunResult};

/// Simulated failure mode for testing evaluation robustness (AI-EVAL.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MockMode {
    /// Ideal responses passing all criteria.
    #[default]
    Normal,
    /// Explain stage creates or modifies a file, violating read-only invariant.
    FaultyExplainModifiesFile,
    /// Map stage returns non-JSON or invalid JSON text.
    FaultyMapInvalidJson,
    /// Map stage returns paths that do not exist in the project.
    FaultyMapInvalidPaths,
    /// Classify stage returns invalid JSON structure.
    FaultyClassifyInvalidJson,
    /// Classify stage returns an unsupported complexity value.
    FaultyClassifyInvalidComplexity,
    /// Plan stage returns invalid JSON structure.
    FaultyPlanInvalidJson,
    /// Plan stage returns invalid number of steps (e.g. 1 step).
    FaultyPlanInvalidStepCount,
    /// Audience stage returns markdown code fences.
    FaultyAudienceCodeFence,
    /// Audience stage returns English text when Russian was requested.
    FaultyAudienceWrongLanguage,
    /// Audience stage returns empty response.
    FaultyAudienceEmpty,
    /// Loop stage fails with non-zero exit code.
    FaultyLoopFailure,
}

impl MockMode {
    /// Maps a model identifier string to a specific mock fault mode.
    pub(crate) fn from_model_str(model: &str) -> Option<Self> {
        match model {
            "mock-faulty-explain" => Some(Self::FaultyExplainModifiesFile),
            "mock-faulty-map-json" => Some(Self::FaultyMapInvalidJson),
            "mock-faulty-map-paths" => Some(Self::FaultyMapInvalidPaths),
            "mock-faulty-classify" => Some(Self::FaultyClassifyInvalidJson),
            "mock-faulty-classify-complexity" => Some(Self::FaultyClassifyInvalidComplexity),
            "mock-faulty-plan" => Some(Self::FaultyPlanInvalidJson),
            "mock-faulty-plan-steps" => Some(Self::FaultyPlanInvalidStepCount),
            "mock-faulty-audience-code" => Some(Self::FaultyAudienceCodeFence),
            "mock-faulty-audience-lang" => Some(Self::FaultyAudienceWrongLanguage),
            "mock-faulty-audience-empty" => Some(Self::FaultyAudienceEmpty),
            "mock-faulty-loop" => Some(Self::FaultyLoopFailure),
            _ => None,
        }
    }
}

/// In-memory scripted provider for deterministic evals and test suites.
#[derive(Debug, Default)]
pub(crate) struct MockProvider {
    /// Configured simulation mode.
    pub(crate) mode: MockMode,
    /// Count of queries processed.
    pub(crate) calls: AtomicUsize,
}

impl MockProvider {
    /// Creates a new normal mock provider.
    pub(crate) fn new() -> Self {
        Self {
            mode: MockMode::Normal,
            calls: AtomicUsize::new(0),
        }
    }

    /// Creates a mock provider configured with a specific fault mode.
    #[allow(dead_code)]
    pub(crate) fn with_mode(mode: MockMode) -> Self {
        Self {
            mode,
            calls: AtomicUsize::new(0),
        }
    }

    fn effective_mode(&self, model: &str) -> MockMode {
        if let Some(m) = MockMode::from_model_str(model) {
            m
        } else {
            self.mode
        }
    }
}

impl Provider for MockProvider {
    fn id(&self) -> &'static str {
        "mock"
    }

    fn models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                provider: "mock".into(),
                model: "mock-fast".into(),
                tier: 1,
                cost: CostClass::Free,
                label: "Mock Fast".into(),
            },
            ModelInfo {
                provider: "mock".into(),
                model: "mock-pro".into(),
                tier: 3,
                cost: CostClass::Paid,
                label: "Mock Pro".into(),
            },
            ModelInfo {
                provider: "mock".into(),
                model: "mock-faulty-explain".into(),
                tier: 1,
                cost: CostClass::Free,
                label: "Mock Faulty Explain (Modifies File)".into(),
            },
            ModelInfo {
                provider: "mock".into(),
                model: "mock-faulty-map-json".into(),
                tier: 1,
                cost: CostClass::Free,
                label: "Mock Faulty Map (Invalid JSON)".into(),
            },
            ModelInfo {
                provider: "mock".into(),
                model: "mock-faulty-map-paths".into(),
                tier: 1,
                cost: CostClass::Free,
                label: "Mock Faulty Map (Non-existent Paths)".into(),
            },
            ModelInfo {
                provider: "mock".into(),
                model: "mock-faulty-classify".into(),
                tier: 1,
                cost: CostClass::Free,
                label: "Mock Faulty Classify (Invalid JSON)".into(),
            },
            ModelInfo {
                provider: "mock".into(),
                model: "mock-faulty-plan".into(),
                tier: 1,
                cost: CostClass::Free,
                label: "Mock Faulty Plan (Invalid JSON)".into(),
            },
            ModelInfo {
                provider: "mock".into(),
                model: "mock-faulty-audience-code".into(),
                tier: 1,
                cost: CostClass::Free,
                label: "Mock Faulty Audience (Markdown Code Fence)".into(),
            },
            ModelInfo {
                provider: "mock".into(),
                model: "mock-faulty-audience-lang".into(),
                tier: 1,
                cost: CostClass::Free,
                label: "Mock Faulty Audience (Language Mismatch)".into(),
            },
        ]
    }

    fn can_resume(&self) -> bool {
        true
    }

    fn ask(&self, req: &AskRequest<'_>, _: &Cancel) -> Result<Answer, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mode = self.effective_mode(req.model);
        let sys = req.system.unwrap_or_default();
        let prompt = req.prompt;

        let text = if sys.contains("planning work")
            || sys.contains("steps")
            || prompt.contains("Plan:")
        {
            // Plan stage
            match mode {
                MockMode::FaultyPlanInvalidJson => "not a valid json plan".to_string(),
                MockMode::FaultyPlanInvalidStepCount => {
                    r#"{"steps":[{"title":"Only step","prompt":"Do everything","complexity":"simple"}]}"#.to_string()
                }
                _ => {
                    r#"{"steps":[{"title":"Update source logic","prompt":"Add required function to source file","complexity":"simple"},{"title":"Document changes","prompt":"Update README with usage example","complexity":"simple"}]}"#.to_string()
                }
            }
        } else if sys.contains("rate coding tasks") || sys.contains("for routing") {
            // Classify stage
            match mode {
                MockMode::FaultyClassifyInvalidJson => "invalid classification text".to_string(),
                MockMode::FaultyClassifyInvalidComplexity => {
                    r#"{"complexity":"ultra-hard","decompose":false,"reason":"Non-standard complexity"}"#.to_string()
                }
                _ => {
                    r#"{"complexity":"simple","decompose":false,"reason":"Small modification in one file"}"#.to_string()
                }
            }
        } else if sys.contains("names and summaries") || prompt.contains("PROJECT DATA") {
            // Map stage
            match mode {
                MockMode::FaultyMapInvalidJson => "not valid json map".to_string(),
                MockMode::FaultyMapInvalidPaths => {
                    r#"{"parts":[{"name":"Документация","summary":"Описание","paths":["README.md"]},{"name":"Несуществующий код","summary":"Файлы не из проекта","paths":["src/ghost_file.rs","nonexistent/module.py"]}],"links":[{"from":"Документация","to":"Несуществующий код"}]}"#.to_string()
                }
                _ => {
                    let mut doc_paths = Vec::new();
                    let mut code_paths = Vec::new();
                    if let Some(files_section) = prompt.split("Files:\n").nth(1) {
                        let files_block = files_section.split("\n\nREADME:").next().unwrap_or("");
                        for line in files_block.lines() {
                            let p = line.trim();
                            if p.is_empty() {
                                continue;
                            }
                            if p.eq_ignore_ascii_case("readme.md") {
                                doc_paths.push(p.to_string());
                            } else {
                                code_paths.push(p.to_string());
                            }
                        }
                    }
                    if doc_paths.is_empty() {
                        doc_paths.push(code_paths.first().cloned().unwrap_or_else(|| "README.md".into()));
                    }
                    if code_paths.is_empty() {
                        code_paths.push(doc_paths.first().cloned().unwrap_or_else(|| "README.md".into()));
                    }

                    serde_json::json!({
                        "parts": [
                            {
                                "name": "Документация",
                                "summary": "Описание проекта и инструкции по использованию.",
                                "paths": doc_paths,
                            },
                            {
                                "name": "Исходный код",
                                "summary": "Основные модули и конфигурация проекта.",
                                "paths": code_paths,
                            }
                        ],
                        "links": [
                            {
                                "from": "Документация",
                                "to": "Исходный код"
                            }
                        ]
                    }).to_string()
                }
            }
        } else if sys.contains("not a programmer")
            || sys.contains("не программист")
            || sys.contains("non-programmer")
            || sys.contains("without code")
            || prompt.contains("non-programmer")
            || prompt.contains("does not code")
            || prompt.contains("summarize the outcome")
        {
            // Audience stage
            match mode {
                MockMode::FaultyAudienceCodeFence => {
                    "Я обновил проект. Вот изменённый код:\n```rust\npub fn write() {}\n```".to_string()
                }
                MockMode::FaultyAudienceWrongLanguage => {
                    "The task was successfully finished and all tests pass with zero errors.".to_string()
                }
                MockMode::FaultyAudienceEmpty => "   ".to_string(),
                _ => {
                    "Я проверил проект и обновил нужный модуль. Теперь новая функция доступна для использования, все тесты проходят успешно.".to_string()
                }
            }
        } else {
            "This project is a modular library organized into source files and documentation. No file changes are required.".to_string()
        };

        Ok(Answer {
            text,
            cost_usd: Some(0.0),
        })
    }

    fn run(&self, req: &RunRequest<'_>, _: &mut dyn FnMut(Output), _: &Cancel) -> RunResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mode = self.effective_mode(req.model);

        if mode == MockMode::FaultyLoopFailure {
            return RunResult {
                code: 1,
                ok: false,
                ..Default::default()
            };
        }

        // In explain stage, we MUST NOT modify any files unless testing failure mode
        let is_explain = req.prompt.contains("Explain")
            || req.prompt.contains("Объясни")
            || req.prompt.contains("without changing any files")
            || req.prompt.contains("без изменения");

        if is_explain {
            if mode == MockMode::FaultyExplainModifiesFile {
                let leaked_file = req.dir.join("leaked_explain_file.txt");
                let _ = fs::write(leaked_file, "eval-explain-violation");
            }
            return RunResult {
                code: 0,
                ok: true,
                ..Default::default()
            };
        }

        // In normal loop tasks, apply a benign file modification to simulate progress
        let test_file = req.dir.join("eval_output.txt");
        let _ = fs::write(test_file, "eval-verified");

        RunResult {
            code: 0,
            ok: true,
            ..Default::default()
        }
    }
}
