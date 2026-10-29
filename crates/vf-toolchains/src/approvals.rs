//! Which tool versions the person allowed the agent to install: one spec per line in
//! `<tools>/allowed-tools.txt`. A version allowed once is not asked about again; a new version is.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::{Need, Progress};

/// The file of allowed specs.
fn file(tools: &Path) -> PathBuf {
    tools.join("allowed-tools.txt")
}

/// The specs allowed so far.
pub fn allowed(tools: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(file(tools))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Records `needs` as allowed. A failure to record only means the person is asked again.
pub fn remember(tools: &Path, needs: &[Need]) {
    let mut all = allowed(tools);
    all.extend(needs.iter().map(|n| n.spec.clone()));
    let text: String = all.iter().map(|s| format!("{s}\n")).collect();
    let _ = std::fs::create_dir_all(tools);
    let _ = std::fs::write(file(tools), text);
}

/// The `needs` that may be installed: those allowed before, and the new ones when `allow` says so
/// (asked once for all new ones). New ones not allowed are reported as `not_allowed`.
pub fn ask(
    tools: &Path,
    needs: Vec<Need>,
    allow: &mut dyn FnMut(&[Need]) -> bool,
    progress: &mut dyn FnMut(Progress),
) -> Vec<Need> {
    let known = allowed(tools);
    let (old, new): (Vec<Need>, Vec<Need>) =
        needs.iter().cloned().partition(|n| known.contains(&n.spec));
    if new.is_empty() {
        return needs;
    }
    if allow(&new) {
        remember(tools, &new);
        return needs;
    }
    for n in new {
        progress(Progress::Failed {
            tool: n.spec,
            error: "the person did not allow installing it".into(),
            code: "not_allowed",
        });
    }
    old
}

#[cfg(test)]
mod tests {
    use super::*;

    fn need(name: &str, spec: &str) -> Need {
        Need {
            name: name.into(),
            spec: spec.into(),
        }
    }

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(
            format!(
                "vf-approvals-{tag}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
            )
            .replace([':', ' '], "_"),
        );
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn new_tools_are_asked_about_once_together() {
        let tools = temp("once");
        let needs = vec![need("node", "node@20"), need("java", "java@temurin-21")];
        let mut asked = Vec::new();
        let got = ask(
            &tools,
            needs.clone(),
            &mut |n| {
                asked.push(n.len());
                true
            },
            &mut |_| {},
        );
        assert_eq!(got, needs);
        assert_eq!(asked, [2]);
        // The same versions again: no question.
        let got = ask(
            &tools,
            needs.clone(),
            &mut |_| panic!("asked again"),
            &mut |_| {},
        );
        assert_eq!(got, needs);
        let _ = std::fs::remove_dir_all(&tools);
    }

    #[test]
    fn a_new_version_is_asked_about() {
        let tools = temp("version");
        remember(&tools, &[need("node", "node@20")]);
        let mut asked = Vec::new();
        ask(
            &tools,
            vec![need("node", "node@22")],
            &mut |n| {
                asked.extend(n.iter().map(|x| x.spec.clone()));
                true
            },
            &mut |_| {},
        );
        assert_eq!(asked, ["node@22"]);
        let _ = std::fs::remove_dir_all(&tools);
    }

    #[test]
    fn tools_not_allowed_are_not_installed_and_say_why() {
        let tools = temp("denied");
        remember(&tools, &[need("node", "node@20")]);
        let mut failed = Vec::new();
        let got = ask(
            &tools,
            vec![need("node", "node@20"), need("python", "python@3.12")],
            &mut |_| false,
            &mut |p| {
                if let Progress::Failed { tool, code, .. } = p {
                    failed.push((tool, code));
                }
            },
        );
        assert_eq!(got, [need("node", "node@20")]);
        assert_eq!(failed, [("python@3.12".to_owned(), "not_allowed")]);
        // Not remembered: asked again next time.
        assert!(!allowed(&tools).contains("python@3.12"));
        let _ = std::fs::remove_dir_all(&tools);
    }

    #[test]
    fn nothing_needed_asks_nothing() {
        let tools = temp("none");
        let got = ask(&tools, Vec::new(), &mut |_| panic!("asked"), &mut |_| {});
        assert!(got.is_empty());
    }
}
