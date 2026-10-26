//! Deterministic mock provider for offline evaluation tests.

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use vf_protocol::{Cancel, CostClass, ModelInfo};
use vf_providers::{Answer, AskRequest, Output, Provider, RunRequest, RunResult};

/// In-memory scripted provider for deterministic evals and test suites.
#[derive(Debug, Default)]
pub(crate) struct MockProvider {
    /// Count of queries processed.
    pub(crate) calls: AtomicUsize,
}

impl MockProvider {
    /// Creates a new mock provider.
    pub(crate) fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
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
        ]
    }

    fn can_resume(&self) -> bool {
        true
    }

    fn ask(&self, req: &AskRequest<'_>, _: &Cancel) -> Result<Answer, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let sys = req.system.unwrap_or_default();
        let prompt = req.prompt;

        let text = if sys.contains("planning work")
            || sys.contains("steps")
            || prompt.contains("Plan:")
        {
            // Plan stage
            r#"{"steps":[{"title":"Update source logic","prompt":"Add required function to source file","complexity":"simple"},{"title":"Document changes","prompt":"Update README with usage example","complexity":"simple"}]}"#
        } else if sys.contains("rate coding tasks") || sys.contains("for routing") {
            // Classify stage
            r#"{"complexity":"simple","decompose":false,"reason":"Small modification in one file"}"#
        } else if sys.contains("names and summaries") || prompt.contains("PROJECT DATA") {
            // Map stage
            r#"{"parts":[{"name":"Documentation","summary":"Explains the library and usage.","paths":["README.md"]},{"name":"Source Code","summary":"Core module implementation.","paths":["src","index.js","Cargo.toml","package.json","build.gradle","pyproject.toml"]}],"links":[{"from":"Documentation","to":"Source Code"}]}"#
        } else if sys.contains("не программист")
            || sys.contains("non-programmer")
            || prompt.contains("non-programmer")
        {
            // Audience stage (plain text, no code blocks, matching user language)
            "Я проверил проект и обновил нужный модуль. Теперь новая функция доступна для использования, все тесты проходят успешно."
        } else {
            // Default explain / general read-only response
            "This project is a modular library organized into source files and documentation. No file changes are required."
        };

        Ok(Answer {
            text: text.into(),
            cost_usd: Some(0.0),
        })
    }

    fn run(&self, req: &RunRequest<'_>, _: &mut dyn FnMut(Output), _: &Cancel) -> RunResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        // In explain stage, we MUST NOT modify any files
        if req.prompt.contains("Explain")
            || req.prompt.contains("Объясни")
            || req.prompt.contains("without changing any files")
            || req.prompt.contains("без изменения")
        {
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
