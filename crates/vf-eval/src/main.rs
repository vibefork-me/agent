//! `vibefork-eval`: automated prompt evaluation and model benchmark tool (AI-EVAL.md).

#![allow(clippy::print_stdout, clippy::print_stderr)]

mod fixtures;
mod mock;
mod report;
mod stages;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::SystemTime;

use fixtures::{ALL_FIXTURES, FixtureProject, find_fixture};
use report::EvalReport;
use stages::{EvalStage, StageRunResult, run_stage};
use vf_core::prompts::Stage;
use vf_providers::Provider;

struct CliConfig {
    provider: String,
    model: String,
    stages: Vec<EvalStage>,
    projects: Vec<&'static FixtureProject>,
    repeat: usize,
    out_dir: PathBuf,
    compare: Option<(PathBuf, PathBuf)>,
}

fn print_usage() {
    println!(
        r#"vibefork-eval — model and stage prompt evaluation tool (AI-EVAL.md)

Usage:
  vibefork-eval [OPTIONS]
  vibefork-eval --compare <report_a.json> <report_b.json>

Options:
  --provider <NAME>   Provider to evaluate (mock, claude, codex, gemini, openrouter, custom) [default: mock]
  --model <NAME>      Model identifier to test (e.g. mock-fast, gemini-2.0-flash) [default: provider default]
  --stages <LIST>     Comma-separated list of stages: map,classify,plan,loop,audience,explain [default: all]
  --projects <LIST>   Comma-separated list of projects: itoa,is-sorted,fabric,python,ru-readme,adversarial [default: all]
  --repeat <N>        Number of repetitions per project and stage [default: 5]
  --out-dir <PATH>    Directory to store Markdown and JSON reports [default: .runs]
  -h, --help          Show this help message
"#
    );
}

fn parse_args() -> Result<Option<CliConfig>, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return Ok(None);
    }

    let mut provider = "mock".to_string();
    let mut model = String::new();
    let mut stages = EvalStage::ALL.to_vec();
    let mut projects: Vec<&'static FixtureProject> = ALL_FIXTURES.iter().collect();
    let mut repeat = 5;
    let mut out_dir = PathBuf::from(".runs");
    let mut compare = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--provider" => {
                i += 1;
                provider = args.get(i).ok_or("Missing value for --provider")?.clone();
            }
            "--model" => {
                i += 1;
                model = args.get(i).ok_or("Missing value for --model")?.clone();
            }
            "--stages" => {
                i += 1;
                let raw = args.get(i).ok_or("Missing value for --stages")?;
                stages = raw
                    .split(',')
                    .map(|s| {
                        EvalStage::parse(s).ok_or_else(|| format!("Unknown stage: '{}'", s.trim()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
            }
            "--projects" => {
                i += 1;
                let raw = args.get(i).ok_or("Missing value for --projects")?;
                projects = raw
                    .split(',')
                    .map(|s| {
                        find_fixture(s.trim())
                            .ok_or_else(|| format!("Unknown project fixture: '{}'", s.trim()))
                    })
                    .collect::<Result<Vec<&'static FixtureProject>, _>>()?;
            }
            "--repeat" => {
                i += 1;
                repeat = args
                    .get(i)
                    .ok_or("Missing value for --repeat")?
                    .parse::<usize>()
                    .map_err(|e| format!("Invalid --repeat: {e}"))?;
            }
            "--out-dir" => {
                i += 1;
                out_dir = PathBuf::from(args.get(i).ok_or("Missing value for --out-dir")?);
            }
            "--compare" => {
                i += 1;
                let file_a = PathBuf::from(args.get(i).ok_or("Missing first file for --compare")?);
                i += 1;
                let file_b = PathBuf::from(args.get(i).ok_or("Missing second file for --compare")?);
                compare = Some((file_a, file_b));
            }
            unknown => return Err(format!("Unknown argument: {unknown}")),
        }
        i += 1;
    }

    if model.is_empty() && provider == "mock" {
        model = "mock-fast".into();
    }

    Ok(Some(CliConfig {
        provider,
        model,
        stages,
        projects,
        repeat,
        out_dir,
        compare,
    }))
}

fn resolve_provider(name: &str) -> Result<Box<dyn Provider>, String> {
    match name {
        "mock" | "scripted" => Ok(Box::new(mock::MockProvider::new())),
        "claude" => Ok(Box::new(vf_providers::claude::Claude::detect())),
        "codex" => Ok(Box::new(vf_providers::codex::Codex::detect())),
        "gemini" => {
            if vf_platform::secrets::gemini_key().is_none() {
                return Err(
                    "Provider 'gemini' requires an API key in OS secret store (or GEMINI_API_KEY)"
                        .into(),
                );
            }
            Ok(Box::new(
                vf_providers::openai::OpenAiCompatible::gemini_dynamic(
                    vf_platform::secrets::gemini_key,
                ),
            ))
        }
        "openrouter" => {
            if vf_platform::secrets::openrouter_key().is_none() {
                return Err("Provider 'openrouter' requires an API key in OS secret store (or OPENROUTER_API_KEY)".into());
            }
            Ok(Box::new(
                vf_providers::openai::OpenAiCompatible::openrouter_dynamic(
                    vf_platform::secrets::openrouter_key,
                ),
            ))
        }
        "custom" => {
            let s = vf_platform::Settings::from_env();
            if s.custom_openai_config().is_none() {
                return Err(
                    "Provider 'custom' requires configuration in settings (or VF_CUSTOM_API_*)"
                        .into(),
                );
            }
            Ok(Box::new(
                vf_providers::openai::OpenAiCompatible::custom_dynamic(
                    move || s.custom_openai_config().map(|c| (c.endpoint, c.model)),
                    vf_platform::secrets::custom_openai_key,
                ),
            ))
        }
        other => Err(format!(
            "Unknown provider: '{other}'. Supported: mock, claude, codex, gemini, openrouter, custom"
        )),
    }
}

fn get_prompt_versions() -> BTreeMap<String, u32> {
    let mut map = BTreeMap::new();
    for stage in Stage::ALL {
        let p = vf_core::prompts::get(stage);
        map.insert(stage.as_str().into(), p.header.version);
    }
    map
}

fn create_temp_workdir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("vf-eval-{prefix}-{nonce}"));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn handle_compare(path_a: &Path, path_b: &Path) -> ExitCode {
    let text_a = match fs::read_to_string(path_a) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to read {}: {e}", path_a.display());
            return ExitCode::FAILURE;
        }
    };
    let text_b = match fs::read_to_string(path_b) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to read {}: {e}", path_b.display());
            return ExitCode::FAILURE;
        }
    };

    let rep_a: EvalReport = match serde_json::from_str(&text_a) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to parse {}: {e}", path_a.display());
            return ExitCode::FAILURE;
        }
    };
    let rep_b: EvalReport = match serde_json::from_str(&text_b) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to parse {}: {e}", path_b.display());
            return ExitCode::FAILURE;
        }
    };

    println!("{}", EvalReport::compare(&rep_a, &rep_b));
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let config = match parse_args() {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}\n");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    if let Some((ref file_a, ref file_b)) = config.compare {
        return handle_compare(file_a, file_b);
    }

    let provider = match resolve_provider(&config.provider) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "Running evaluation on provider: {}, model: '{}', repeat: {}, stages: {}, projects: {}",
        config.provider,
        config.model,
        config.repeat,
        config.stages.len(),
        config.projects.len()
    );

    let mut all_runs: Vec<StageRunResult> = Vec::new();
    let total_steps = config.repeat * config.projects.len() * config.stages.len();
    let mut step_count = 0;

    for r in 1..=config.repeat {
        for project in &config.projects {
            let work_dir = create_temp_workdir(&format!("{}-r{}", project.id, r));
            if let Err(e) = project.materialize(&work_dir) {
                eprintln!("Failed to materialize fixture '{}': {e}", project.id);
                let _ = fs::remove_dir_all(&work_dir);
                continue;
            }

            for &stage in &config.stages {
                step_count += 1;
                print!(
                    "[{step_count}/{total_steps}] rep {r}/{} · {} · {} ... ",
                    config.repeat,
                    project.id,
                    stage.as_str()
                );

                let res = run_stage(stage, &*provider, &config.model, project, &work_dir);
                if res.success {
                    println!("OK ({}ms)", res.latency_ms);
                } else if res.format_ok {
                    println!(
                        "FORMAT OK / CHECK FAILED: {}",
                        res.error.as_deref().unwrap_or("")
                    );
                } else {
                    println!("FAIL: {}", res.error.as_deref().unwrap_or(""));
                }
                all_runs.push(res);
            }

            let _ = fs::remove_dir_all(&work_dir);
        }
    }

    let report = EvalReport::new(
        &config.provider,
        &config.model,
        get_prompt_versions(),
        all_runs,
    );

    let md = report.to_markdown();
    println!("\n=== EVALUATION REPORT ===\n\n{md}");

    if let Err(e) = fs::create_dir_all(&config.out_dir) {
        eprintln!(
            "Warning: failed to create out_dir {}: {e}",
            config.out_dir.display()
        );
    } else {
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let safe_model = config.model.replace(['/', ':', '\\'], "-");
        let base_name = format!("ai-eval-{}-{}-{}", config.provider, safe_model, ts);
        let md_path = config.out_dir.join(format!("{base_name}.md"));
        let json_path = config.out_dir.join(format!("{base_name}.json"));

        if let Ok(json_str) = report.to_json() {
            let _ = fs::write(&json_path, json_str);
            println!("Saved JSON report: {}", json_path.display());
        }
        let _ = fs::write(&md_path, &md);
        println!("Saved Markdown report: {}", md_path.display());
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_evaluation_full_pass() {
        let provider = mock::MockProvider::new();
        let project = &fixtures::FIXTURE_ITOA;
        let temp = create_temp_workdir("test-mock");
        project.materialize(&temp).unwrap();

        for stage in EvalStage::ALL {
            let res = run_stage(stage, &provider, "mock-fast", project, &temp);
            assert!(res.format_ok, "Stage {:?} should pass format check", stage);
            assert!(
                res.success,
                "Stage {:?} should pass success check: {:?}",
                stage, res.error
            );
        }

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_report_summary_and_compare() {
        let provider = mock::MockProvider::new();
        let project = &fixtures::FIXTURE_IS_SORTED;
        let temp = create_temp_workdir("test-report");
        project.materialize(&temp).unwrap();

        let mut runs = Vec::new();
        for stage in [EvalStage::Map, EvalStage::Classify] {
            runs.push(run_stage(stage, &provider, "mock-fast", project, &temp));
        }
        let report = EvalReport::new("mock", "mock-fast", BTreeMap::new(), runs.clone());
        assert_eq!(report.summaries.len(), 2);
        assert_eq!(report.summaries[&EvalStage::Map].format_passed, 1);

        let report_b = EvalReport::new("mock", "mock-v2", BTreeMap::new(), runs);
        let diff = EvalReport::compare(&report, &report_b);
        assert!(diff.contains("Сравнение: mock / mock-fast"));

        let _ = fs::remove_dir_all(temp);
    }
}
