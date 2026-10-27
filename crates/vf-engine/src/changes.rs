//! What a task changed, by part: the "changes" artifact of a step (each changed file with the part of
//! the map it belongs to and its changed lines) and the canvas events made from it.

use std::collections::BTreeMap;

use vf_protocol::canvas::{Artifact, ArtifactFile, ArtifactKind, CanvasEvent};

/// Most files listed in one artifact.
const MAX_FILES: usize = 50;
/// Most changed lines kept per file.
const MAX_LINES: usize = 60;
/// Longest kept line, characters.
const LINE_CHARS: usize = 200;

/// Files and their changed lines (`-`/`+`) in a unified git diff, in the order they appear.
fn files_of(diff: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut deleted_from: Option<String> = None;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            deleted_from = None;
            continue;
        }
        if let Some(path) = line.strip_prefix("--- a/") {
            deleted_from = Some(path.to_owned());
            continue;
        }
        if line.starts_with("--- ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            let path = rest
                .strip_prefix("b/")
                .map(str::to_owned)
                .or_else(|| deleted_from.take());
            if let Some(path) = path {
                out.push((path, Vec::new()));
            }
            continue;
        }
        if (line.starts_with('+') || line.starts_with('-'))
            && let Some((_, lines)) = out.last_mut()
            && lines.len() < MAX_LINES
        {
            lines.push(line.chars().take(LINE_CHARS).collect());
        }
    }
    out
}

/// The "changes" artifact of a diff; `part` gives the part of a file (`None`: no part on the canvas).
/// `None` when nothing changed.
pub(crate) fn changes_artifact(
    diff: &str,
    part: &dyn Fn(&str) -> Option<String>,
) -> Option<Artifact> {
    let files: Vec<ArtifactFile> = files_of(diff)
        .into_iter()
        .take(MAX_FILES)
        .map(|(path, diff)| ArtifactFile {
            node: part(&path),
            path,
            note: None,
            diff,
        })
        .collect();
    if files.is_empty() {
        return None;
    }
    let mut nodes: Vec<String> = files.iter().filter_map(|f| f.node.clone()).collect();
    nodes.dedup();
    nodes.sort_unstable();
    nodes.dedup();
    Some(Artifact {
        name: "Changes".into(),
        kind: ArtifactKind::Changes,
        size: None,
        path: None,
        about: None,
        nodes,
        files,
        checks: Vec::new(),
        lines: Vec::new(),
    })
}

/// One `change` event per part the artifacts touched, with its files.
pub(crate) fn change_events(artifacts: &[Artifact]) -> Vec<CanvasEvent> {
    let mut by_part: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for a in artifacts.iter().filter(|a| a.kind == ArtifactKind::Changes) {
        for f in &a.files {
            if let Some(node) = &f.node {
                let files = by_part.entry(node.clone()).or_default();
                if !files.contains(&f.path) {
                    files.push(f.path.clone());
                }
            }
        }
    }
    by_part
        .into_iter()
        .map(|(id, files)| CanvasEvent::Change { id, files })
        .collect()
}

/// The changes of several steps as one artifact; a file changed again keeps its latest lines.
pub(crate) fn merge(artifacts: Vec<Artifact>) -> Vec<Artifact> {
    let mut changes: Option<Artifact> = None;
    let mut others = Vec::new();
    for a in artifacts {
        if a.kind != ArtifactKind::Changes {
            others.push(a);
            continue;
        }
        match changes.as_mut() {
            None => changes = Some(a),
            Some(all) => {
                for f in a.files {
                    all.files.retain(|g| g.path != f.path);
                    all.files.push(f);
                }
                for n in a.nodes {
                    if !all.nodes.contains(&n) {
                        all.nodes.push(n);
                    }
                }
            }
        }
    }
    changes.into_iter().chain(others).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/src/ui/theme.ts b/src/ui/theme.ts\n\
index 1..2 100644\n\
--- a/src/ui/theme.ts\n\
+++ b/src/ui/theme.ts\n\
@@ -1 +1 @@\n\
-light\n\
+dark\n\
diff --git a/notes.md b/notes.md\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/notes.md\n\
@@ -0,0 +1 @@\n\
+hello\n\
diff --git a/src/old.ts b/src/old.ts\n\
deleted file mode 100644\n\
--- a/src/old.ts\n\
+++ /dev/null\n\
@@ -1 +0,0 @@\n\
-gone\n";

    fn part(path: &str) -> Option<String> {
        path.starts_with("src/ui").then(|| "p1".to_owned())
    }

    #[test]
    fn a_diff_becomes_files_with_their_parts_and_lines() {
        let a = changes_artifact(DIFF, &part).unwrap();
        let files: Vec<(&str, Option<&str>, Vec<&str>)> = a
            .files
            .iter()
            .map(|f| {
                (
                    f.path.as_str(),
                    f.node.as_deref(),
                    f.diff.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            files,
            [
                ("src/ui/theme.ts", Some("p1"), vec!["-light", "+dark"]),
                ("notes.md", None, vec!["+hello"]),
                ("src/old.ts", None, vec!["-gone"]),
            ]
        );
        assert_eq!(a.nodes, ["p1"]);
        assert!(changes_artifact("", &part).is_none());
    }

    #[test]
    fn changes_become_one_event_per_part() {
        let a = changes_artifact(DIFF, &part).unwrap();
        assert_eq!(
            change_events(&[a.clone(), a]),
            [CanvasEvent::Change {
                id: "p1".into(),
                files: vec!["src/ui/theme.ts".into()]
            }]
        );
    }

    #[test]
    fn steps_merge_into_one_changes_artifact_with_the_latest_lines() {
        let first = changes_artifact(DIFF, &part).unwrap();
        let second = changes_artifact(
            "diff --git a/src/ui/theme.ts b/src/ui/theme.ts\n--- a/src/ui/theme.ts\n+++ b/src/ui/theme.ts\n-dark\n+darker\n",
            &part,
        )
        .unwrap();
        let merged = merge(vec![first, second]);
        assert_eq!(merged.len(), 1);
        let theme = merged[0]
            .files
            .iter()
            .find(|f| f.path == "src/ui/theme.ts")
            .unwrap();
        assert_eq!(theme.diff, ["-dark", "+darker"]);
        assert_eq!(merged[0].files.len(), 3);
    }
}
