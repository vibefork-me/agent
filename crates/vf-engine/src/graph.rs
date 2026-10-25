//! The map of a project for the editor: its parts by folder, and which of them changed since the
//! project was taken. Built from the files of the newest commit, so ignored files never show.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use vf_git::Repo;
use vf_journal::Vcs as _;
use vf_protocol::BackendError;
use vf_protocol::http::{DescribeRequest, GraphEdge, GraphNode, GraphResponse, GraphSource};

use crate::Engine;

/// A top folder with more files than this is shown as its subfolders.
const SPLIT_ABOVE: usize = 40;
/// Most parts in a map; smaller ones are dropped past it.
const MAX_NODES: usize = 200;

/// The part a file belongs to: its top folder, or `top/sub` when the top folder is split.
fn part_of(path: &str, split: &BTreeSet<String>) -> String {
    let mut it = path.split('/');
    let first = it.next().unwrap_or_default();
    let rest: Vec<&str> = it.collect();
    if rest.is_empty() {
        return ".".into();
    }
    if split.contains(first) && rest.len() > 1 {
        return format!("{first}/{}", rest[0]);
    }
    first.into()
}

/// Parts and edges from the files of a project and the files changed in it.
pub(crate) fn build(files: &[String], changed: &[String]) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut per_top: BTreeMap<&str, usize> = BTreeMap::new();
    for f in files {
        if let Some((top, _)) = f.split_once('/') {
            *per_top.entry(top).or_default() += 1;
        }
    }
    let split: BTreeSet<String> = per_top
        .iter()
        .filter(|(_, n)| **n > SPLIT_ABOVE)
        .map(|(t, _)| (*t).to_owned())
        .collect();
    let file_set: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    let changed: BTreeSet<&str> = changed.iter().map(String::as_str).collect();
    // (files, changed) per part; a deleted file counts as changed only.
    let mut counts: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for f in file_set.union(&changed) {
        let c = counts.entry(part_of(f, &split)).or_default();
        c.0 += u32::from(file_set.contains(f));
        c.1 += u32::from(changed.contains(f));
    }
    let mut parts: Vec<(String, (u32, u32))> = counts.into_iter().collect();
    if parts.len() > MAX_NODES {
        parts.sort_by_key(|p| std::cmp::Reverse((p.1.1, p.1.0)));
        parts.truncate(MAX_NODES);
    }
    // A split folder sums its subfolders and its own files.
    let mut totals: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut edges = Vec::new();
    for (id, (f, c)) in parts {
        if let Some((top, _)) = id.split_once('/') {
            let t = totals.entry(top.to_owned()).or_default();
            t.0 += f;
            t.1 += c;
            edges.push(GraphEdge {
                from: top.to_owned(),
                to: id.clone(),
            });
        }
        let n = totals.entry(id).or_default();
        n.0 += f;
        n.1 += c;
    }
    edges.sort_by(|a, b| a.to.cmp(&b.to));
    let mut nodes: Vec<GraphNode> = totals
        .into_iter()
        .map(|(id, (files, changed))| GraphNode {
            label: id
                .rsplit_once('/')
                .map_or_else(|| id.clone(), |(_, l)| l.to_owned()),
            id,
            summary: None,
            files,
            changed,
        })
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    (nodes, edges)
}

impl Engine {
    /// The project folder, its newest commit, its files then and the files changed since it was
    /// taken; `None` — no commits yet.
    #[allow(clippy::type_complexity)]
    fn graph_input(
        &self,
        project: &str,
    ) -> Result<Option<(PathBuf, String, Vec<String>, Vec<String>)>, BackendError> {
        let root = self
            .projects
            .resolve(project)
            .map_err(|_| BackendError::NotFound("no such project".into()))?;
        let repo = Repo::open(root.clone());
        let Ok(head) = repo.head() else {
            return Ok(None);
        };
        let files = repo
            .files_at(&head)
            .map_err(|e| BackendError::Internal(e.0))?;
        let changed = match self.load_meta(project).map(|m| m.local_base) {
            Some(base) if !base.is_empty() => repo
                .changed_paths(&base, &head)
                .map_err(|e| BackendError::Internal(e.0))?,
            _ => Vec::new(),
        };
        Ok(Some((root, head, files, changed)))
    }

    /// The model's map when there is one, the folders otherwise.
    pub(crate) fn project_graph(&self, project: &str) -> Result<GraphResponse, BackendError> {
        let Some((_, head, files, changed)) = self.graph_input(project)? else {
            return Ok(GraphResponse {
                source: GraphSource::Folders,
                revision: None,
                nodes: Vec::new(),
                edges: Vec::new(),
            });
        };
        let (source, (nodes, edges)) = match self.load_map(project) {
            Some(m) => (GraphSource::Model, crate::map::build(&m, &files, &changed)),
            None => (GraphSource::Folders, build(&files, &changed)),
        };
        Ok(GraphResponse {
            source,
            revision: Some(head),
            nodes,
            edges,
        })
    }

    pub(crate) fn describe_project_graph(
        &self,
        req: &DescribeRequest,
    ) -> Result<GraphResponse, BackendError> {
        let Some((root, head, files, _)) = self.graph_input(&req.project)? else {
            return Err(BackendError::BadRequest(
                "the project has no files yet".into(),
            ));
        };
        let lang = req.lang.as_deref().unwrap_or("en");
        self.describe_map(&req.project, &root, &head, &files, lang)
            .map_err(|e| {
                tracing::info!(project = %req.project, error = %e, "map not described");
                BackendError::BadRequest(format!("no model could describe the project: {e}"))
            })?;
        self.project_graph(&req.project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn parts_are_top_folders_and_root_files() {
        let (nodes, edges) = build(
            &s(&["README.md", "src/main.rs", "src/lib.rs", "docs/a.md"]),
            &s(&["src/lib.rs"]),
        );
        let got: Vec<(&str, u32, u32)> = nodes
            .iter()
            .map(|n| (n.id.as_str(), n.files, n.changed))
            .collect();
        assert_eq!(got, vec![(".", 1, 0), ("docs", 1, 0), ("src", 2, 1)]);
        assert!(edges.is_empty());
    }

    #[test]
    fn a_large_folder_is_shown_by_its_subfolders() {
        let mut files: Vec<String> = (0..30).map(|i| format!("src/ui/f{i}.ts")).collect();
        files.extend((0..20).map(|i| format!("src/core/f{i}.ts")));
        files.push("src/index.ts".into());
        let (nodes, edges) = build(&files, &s(&["src/core/f1.ts", "src/core/gone.ts"]));
        let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec!["src", "src/core", "src/ui"]);
        let count = |id: &str| {
            nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| (n.files, n.changed))
        };
        assert_eq!(count("src/core"), Some((20, 2)));
        assert_eq!(count("src"), Some((51, 2)));
        assert_eq!(edges.len(), 2);
        assert!(edges.iter().all(|e| e.from == "src"));
    }
}
