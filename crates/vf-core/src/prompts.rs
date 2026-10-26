//! Prompt registry: versioned stage prompts embedded at build time,
//! with an optional dev-only directory override (`VF_PROMPTS_DIR`).

use std::fmt;

/// Stage of the agent workflow that uses a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Overview project map for non-programmers.
    Map,
    /// Fast task complexity rating for Auto mode routing.
    Classify,
    /// Planning step decomposition for complex tasks.
    Plan,
    /// Main execution loop for OpenAI-compatible models.
    Loop,
    /// Handoff preamble for cross-session/cross-model context continuation.
    Handoff,
}

impl Stage {
    /// All registered stages.
    pub const ALL: [Stage; 5] = [
        Stage::Map,
        Stage::Classify,
        Stage::Plan,
        Stage::Loop,
        Stage::Handoff,
    ];

    /// Lowercase stage identifier matching the file name in `prompts/<stage>.md`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Classify => "classify",
            Self::Plan => "plan",
            Self::Loop => "loop",
            Self::Handoff => "handoff",
        }
    }

    /// Parses a stage name.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "map" => Some(Self::Map),
            "classify" => Some(Self::Classify),
            "plan" => Some(Self::Plan),
            "loop" | "openai-loop" => Some(Self::Loop),
            "handoff" => Some(Self::Handoff),
            _ => None,
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Metadata header in the frontmatter of a prompt file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptHeader {
    /// The stage this prompt is used in.
    pub stage: Stage,
    /// Version number of the prompt.
    pub version: u32,
    /// Short description of when this prompt is invoked.
    pub when: String,
    /// Expected model tier or type for this prompt.
    pub model: String,
    /// What input data is provided to this prompt.
    pub input: String,
    /// Expected output format from the model.
    pub output: String,
}

/// A parsed prompt with its metadata header and template body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// Metadata header.
    pub header: PromptHeader,
    /// Template body.
    pub template: String,
}

impl Prompt {
    /// Tag identifying the prompt and version, e.g. `plan:1`.
    pub fn tag(&self) -> String {
        format!("{}:{}", self.header.stage.as_str(), self.header.version)
    }

    /// Version as string.
    pub fn version_str(&self) -> String {
        self.header.version.to_string()
    }

    /// Renders the plan template by replacing `{task}` with `task`.
    pub fn render_plan(&self, task: &str) -> String {
        self.template.replace("{task}", task)
    }

    /// Renders the OpenAI loop system prompt by replacing `{os}` with the current OS,
    /// and appending optional extra instructions.
    pub fn render_loop(&self, extra_instructions: Option<&str>) -> String {
        let base = self.template.replace("{os}", std::env::consts::OS);
        match extra_instructions {
            Some(extra) if !extra.trim().is_empty() => format!("{base}\n{extra}"),
            _ => base,
        }
    }

    /// Renders the handoff preamble by replacing `{history}` with `history`.
    pub fn render_handoff(&self, history: &str) -> String {
        self.template.replace("{history}", history)
    }
}

/// Error during prompt file parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PromptParseError {
    /// Frontmatter was not delimited by `---`.
    #[error("frontmatter not delimited by '---'")]
    InvalidFrontmatter,
    /// A mandatory frontmatter field was absent.
    #[error("missing frontmatter field: {0}")]
    MissingField(&'static str),
    /// Version field could not be parsed as an unsigned integer.
    #[error("invalid version number: {0}")]
    InvalidVersion(String),
    /// Stage field contained an unrecognized identifier.
    #[error("unknown stage in frontmatter: {0}")]
    UnknownStage(String),
}

/// Embedded stage prompts.
const EMBEDDED_MAP: &str = include_str!("../../../prompts/map.md");
const EMBEDDED_CLASSIFY: &str = include_str!("../../../prompts/classify.md");
const EMBEDDED_PLAN: &str = include_str!("../../../prompts/plan.md");
const EMBEDDED_LOOP: &str = include_str!("../../../prompts/loop.md");
const EMBEDDED_HANDOFF: &str = include_str!("../../../prompts/handoff.md");

/// Raw embedded prompt text for a stage.
pub fn embedded_raw(stage: Stage) -> &'static str {
    match stage {
        Stage::Map => EMBEDDED_MAP,
        Stage::Classify => EMBEDDED_CLASSIFY,
        Stage::Plan => EMBEDDED_PLAN,
        Stage::Loop => EMBEDDED_LOOP,
        Stage::Handoff => EMBEDDED_HANDOFF,
    }
}

/// Parses markdown frontmatter and template body.
pub fn parse_prompt(raw: &str) -> Result<Prompt, PromptParseError> {
    let normalized = raw.replace("\r\n", "\n");
    let trimmed = normalized.trim_start();
    if !trimmed.starts_with("---") {
        return Err(PromptParseError::InvalidFrontmatter);
    }
    let after_open = &trimmed[3..];
    let after_open = after_open.strip_prefix('\n').unwrap_or(after_open);
    let Some(closing_pos) = after_open.find("\n---") else {
        return Err(PromptParseError::InvalidFrontmatter);
    };

    let header_str = &after_open[..closing_pos];
    let mut stage = None;
    let mut version = None;
    let mut when = None;
    let mut model = None;
    let mut input = None;
    let mut output = None;

    for line in header_str.lines() {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            continue;
        }
        if let Some((k, v)) = trimmed_line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_string();
            match key.as_str() {
                "stage" => {
                    stage = Some(Stage::parse(&val).ok_or(PromptParseError::UnknownStage(val))?);
                }
                "version" => {
                    let v_num = val
                        .parse::<u32>()
                        .map_err(|_| PromptParseError::InvalidVersion(val))?;
                    version = Some(v_num);
                }
                "when" => when = Some(val),
                "model" => model = Some(val),
                "input" => input = Some(val),
                "output" => output = Some(val),
                _ => {}
            }
        }
    }

    let stage = stage.ok_or(PromptParseError::MissingField("stage"))?;
    let version = version.ok_or(PromptParseError::MissingField("version"))?;
    let when = when.unwrap_or_default();
    let model = model.unwrap_or_default();
    let input = input.unwrap_or_default();
    let output = output.unwrap_or_default();

    // Body is everything after \n---\n
    let rest = &after_open[closing_pos + 4..];
    let body = rest.strip_prefix('\n').unwrap_or(rest);
    let body = body.strip_suffix('\n').unwrap_or(body);

    Ok(Prompt {
        header: PromptHeader {
            stage,
            version,
            when,
            model,
            input,
            output,
        },
        template: body.to_string(),
    })
}

/// Retrieves the prompt for a given stage.
///
/// In release builds, this strictly returns the built-in embedded prompt.
/// In dev builds with the `dev-prompts-dir` feature, `VF_PROMPTS_DIR` can override it.
pub fn get(stage: Stage) -> Prompt {
    #[cfg(feature = "dev-prompts-dir")]
    {
        if let Some(p) = load_from_env_dir(stage) {
            return p;
        }
    }
    match parse_prompt(embedded_raw(stage)) {
        Ok(p) => p,
        Err(_) => unreachable!("embedded prompt is validated by tests"),
    }
}

/// Looks up a prompt by stage name.
pub fn by_name(name: &str) -> Option<Prompt> {
    Stage::parse(name).map(get)
}

/// Returns all registered prompts.
pub fn all() -> Vec<Prompt> {
    Stage::ALL.iter().map(|&s| get(s)).collect()
}

#[cfg(feature = "dev-prompts-dir")]
fn load_from_env_dir(stage: Stage) -> Option<Prompt> {
    let dir = std::env::var_os("VF_PROMPTS_DIR")?;
    if dir.is_empty() {
        return None;
    }
    load_from_dir(std::path::Path::new(&dir), stage)
}

/// Loads a prompt from a directory for development.
#[cfg(feature = "dev-prompts-dir")]
pub fn load_from_dir(dir: &std::path::Path, stage: Stage) -> Option<Prompt> {
    let path = dir.join(format!("{}.md", stage.as_str()));
    let text = std::fs::read_to_string(path).ok()?;
    parse_prompt(&text).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn all_embedded_prompts_are_valid() {
        for stage in Stage::ALL {
            let p = get(stage);
            assert_eq!(p.header.stage, stage);
            assert!(p.header.version >= 1);
            assert!(!p.header.when.is_empty(), "stage {stage} has empty 'when'");
            assert!(
                !p.header.model.is_empty(),
                "stage {stage} has empty 'model'"
            );
            assert!(
                !p.header.input.is_empty(),
                "stage {stage} has empty 'input'"
            );
            assert!(
                !p.header.output.is_empty(),
                "stage {stage} has empty 'output'"
            );
            assert!(!p.template.is_empty(), "stage {stage} has empty template");
            assert_eq!(p.tag(), format!("{}:{}", stage.as_str(), p.header.version));
        }
    }

    #[test]
    fn parse_custom_prompt() {
        let raw = "\
---
stage: plan
version: 42
when: testing custom prompt
model: mock-model
input: dummy input
output: dummy output
---
Custom plan for {task}
";
        let p = parse_prompt(raw).unwrap();
        assert_eq!(p.header.stage, Stage::Plan);
        assert_eq!(p.header.version, 42);
        assert_eq!(p.header.when, "testing custom prompt");
        assert_eq!(p.render_plan("my-task"), "Custom plan for my-task");
    }

    #[test]
    fn parse_rejects_missing_stage_or_version() {
        let no_stage = "---\nversion: 1\n---\nbody";
        assert_eq!(
            parse_prompt(no_stage),
            Err(PromptParseError::MissingField("stage"))
        );

        let no_version = "---\nstage: map\n---\nbody";
        assert_eq!(
            parse_prompt(no_version),
            Err(PromptParseError::MissingField("version"))
        );
    }

    #[test]
    #[cfg(feature = "dev-prompts-dir")]
    fn dev_prompts_dir_overrides_when_feature_enabled() {
        let temp = std::env::temp_dir().join(format!("vf-dev-prompts-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp);
        let custom_file = temp.join("map.md");
        std::fs::write(
            &custom_file,
            "---\nstage: map\nversion: 99\nwhen: dev\nmodel: dev\ninput: dev\noutput: dev\n---\nDev override map",
        )
        .unwrap();

        let p = load_from_dir(&temp, Stage::Map).unwrap();
        assert_eq!(p.header.version, 99);
        assert_eq!(p.template, "Dev override map");
        let _ = std::fs::remove_dir_all(&temp);
    }
}
