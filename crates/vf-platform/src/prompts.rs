//! Prompt resolution with optional dev-only directory override (`VF_PROMPTS_DIR`).

use vf_core::prompts::{Prompt, Stage};

/// Retrieves the prompt for a given stage.
///
/// In builds without the `dev-prompts-dir` feature (release builds), returns the built-in embedded prompt.
/// When the `dev-prompts-dir` feature is enabled, checks `VF_PROMPTS_DIR` for `<stage>.md`.
pub fn get(stage: Stage) -> Prompt {
    #[cfg(feature = "dev-prompts-dir")]
    {
        if let Some(p) = load_from_env(stage) {
            return p;
        }
    }
    vf_core::prompts::get(stage)
}

/// Loads a prompt from an explicit directory path (for development and tests).
#[cfg(feature = "dev-prompts-dir")]
pub fn load_from_dir(dir: &std::path::Path, stage: Stage) -> Option<Prompt> {
    let path = dir.join(format!("{}.md", stage.as_str()));
    let text = std::fs::read_to_string(path).ok()?;
    vf_core::prompts::parse_prompt(&text).ok()
}

#[cfg(feature = "dev-prompts-dir")]
fn load_from_env(stage: Stage) -> Option<Prompt> {
    let dir = std::env::var_os("VF_PROMPTS_DIR")?;
    if dir.is_empty() {
        return None;
    }
    load_from_dir(std::path::Path::new(&dir), stage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_prompts_returned_by_default() {
        for stage in Stage::ALL {
            let p = get(stage);
            assert_eq!(p.header.stage, stage);
            assert_eq!(p, vf_core::prompts::get(stage));
        }
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
