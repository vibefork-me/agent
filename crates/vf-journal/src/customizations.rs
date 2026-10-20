//! `CUSTOMIZATIONS.md` (format v1): one section per customization with a YAML block and an
//! `Intent` section. Keys are English; text is in the project's language. The agent keeps the list
//! and renders the whole file.

use serde::{Deserialize, Serialize};

/// First line of the file: format marker and version.
pub const MARKER: &str = "<!-- vibefork:customizations v1 -->";

/// Lifecycle of a customization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// In the build.
    Active,
    /// Turned off by the user.
    Disabled,
    /// Taken into upstream; dissolves on the next sync.
    Upstreamed,
    /// Abandoned.
    Dropped,
}

/// One customization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Customization {
    /// Stable id: `[a-z0-9-]`.
    pub id: String,
    /// Short title.
    pub title: String,
    /// Status.
    pub status: Status,
    /// Upstream repository (`owner/name`), if known.
    pub upstream_repo: Option<String>,
    /// Upstream version or commit the customization is based on.
    pub upstream_base: String,
    /// Creation date (`YYYY-MM-DD`).
    pub created: String,
    /// Last change date (`YYYY-MM-DD`).
    pub updated: String,
    /// Models that worked on it: `provider/model`.
    pub by: Vec<String>,
    /// Touched files.
    pub files: Vec<String>,
    /// Commands that check it works.
    pub verify_commands: Vec<String>,
    /// How to check it by hand.
    pub verify_manual: String,
    /// What and why — the base for redoing the customization on a changed upstream.
    pub intent: String,
}

/// An id from a title: lowercase ASCII letters, digits and dashes, at most 40 chars.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() {
        "customization".into()
    } else {
        out
    }
}

/// Renders the whole file.
pub fn render(items: &[Customization]) -> String {
    let mut s = format!(
        "{MARKER}\n# Customizations\n\nPersonal changes on top of upstream, maintained by vibefork.\n"
    );
    for c in items {
        s.push_str(&format!(
            "\n## {} — {}\n\n```yaml\n",
            c.id,
            one_line(&c.title)
        ));
        s.push_str(&format!(
            "id: {}\ntitle: {}\nstatus: {}\n",
            c.id,
            yaml_str(&c.title),
            status(c.status)
        ));
        s.push_str("upstream:\n");
        s.push_str(&format!(
            "  repo: {}\n  base: {}\n",
            c.upstream_repo
                .as_deref()
                .map(yaml_str)
                .unwrap_or_else(|| "null".into()),
            yaml_str(&c.upstream_base)
        ));
        s.push_str(&format!("created: {}\nupdated: {}\n", c.created, c.updated));
        s.push_str(&list("by", &c.by));
        s.push_str(&list("files", &c.files));
        s.push_str("verify:\n");
        s.push_str(&list("  commands", &c.verify_commands).replace("\n  - ", "\n    - "));
        s.push_str(&format!("  manual: {}\n", yaml_str(&c.verify_manual)));
        s.push_str("```\n\n### Intent\n");
        s.push_str(c.intent.trim());
        s.push('\n');
    }
    s
}

fn status(s: Status) -> &'static str {
    match s {
        Status::Active => "active",
        Status::Disabled => "disabled",
        Status::Upstreamed => "upstreamed",
        Status::Dropped => "dropped",
    }
}

fn list(key: &str, items: &[String]) -> String {
    if items.is_empty() {
        return format!("{key}: []\n");
    }
    let mut s = format!("{key}:\n");
    for i in items {
        s.push_str(&format!("  - {}\n", yaml_str(i)));
    }
    s
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A YAML double-quoted scalar.
fn yaml_str(s: &str) -> String {
    format!(
        "\"{}\"",
        one_line(s).replace('\\', "\\\\").replace('"', "\\\"")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_safe() {
        assert_eq!(slug("Add --name option!"), "add-name-option");
        assert_eq!(slug("Тёмная тема"), "customization");
        assert!(slug(&"x".repeat(100)).len() <= 40);
    }

    #[test]
    fn render_has_marker_yaml_and_intent() {
        let c = Customization {
            id: "dark-mode".into(),
            title: "Dark \"mode\"".into(),
            status: Status::Active,
            upstream_repo: Some("owner/project".into()),
            upstream_base: "v2.3.1".into(),
            created: "2026-10-20".into(),
            updated: "2026-10-20".into(),
            by: vec!["claude/sonnet".into()],
            files: vec!["src/theme.ts".into()],
            verify_commands: vec!["npm test".into()],
            verify_manual: "Switch to dark".into(),
            intent: "Work at night without a white screen.".into(),
        };
        let md = render(&[c]);
        assert!(md.starts_with(MARKER));
        assert!(md.contains("## dark-mode — Dark \"mode\""));
        assert!(md.contains("title: \"Dark \\\"mode\\\"\""));
        assert!(md.contains("status: active"));
        assert!(md.contains("  commands:\n    - \"npm test\""));
        assert!(md.contains("### Intent\nWork at night"));
    }
}
