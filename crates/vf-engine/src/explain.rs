//! Explaining a part of the project, or answering a question about it, in words only: a model reads
//! the project through a read-only question ([`vf_providers::Provider::ask`]) and its answer goes
//! into the session stream. Nothing of the project changes, no tools are prepared, nothing is built
//! or run, and nothing is journaled.

use vf_core::catalog::{self, Language};
use vf_protocol::canvas::CanvasEvent;
use vf_protocol::ws::ServerMessage;
use vf_protocol::{Cancel, ExplainRequest};
use vf_providers::{AskRequest, ChildEnv};

use crate::map::{self, OTHER, ROOT};
use crate::{Engine, MAX_PROMPT, canvas};

/// Most characters of an answer kept and shown.
pub(crate) const MAX_ANSWER: usize = 8000;
/// Most files of a part named to the model; it may read the others itself.
const MAX_PART_PATHS: usize = 200;
/// Longest answer a model may write, tokens (API providers).
const MAX_ANSWER_TOKENS: u32 = 2000;

/// The part a question is about, as the map shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartInfo {
    /// Node id.
    pub(crate) id: String,
    /// Name as the map shows it.
    pub(crate) name: String,
    /// What it does, when the model's map says.
    pub(crate) summary: Option<String>,
    /// Its folders and files, `/`-separated.
    pub(crate) paths: Vec<String>,
}

/// The question as the model gets it: the project data after the line `PROJECT DATA`, the person's
/// question last.
pub(crate) fn prompt(project: &str, part: Option<&PartInfo>, question: &str) -> String {
    let mut p = format!("PROJECT DATA\n\nProject: {project}");
    if let Some(part) = part {
        p.push_str(&format!("\nPart of the map: {}", part.name));
        if let Some(summary) = part.summary.as_deref().filter(|s| !s.is_empty()) {
            p.push_str(&format!("\nWhat it does: {summary}"));
        }
        if !part.paths.is_empty() {
            let shown: Vec<&str> = part
                .paths
                .iter()
                .map(String::as_str)
                .take(MAX_PART_PATHS)
                .collect();
            p.push_str("\nIts folders and files:\n");
            p.push_str(&shown.join("\n"));
            let more = part.paths.len().saturating_sub(MAX_PART_PATHS);
            if more > 0 {
                p.push_str(&format!("\n… and {more} more"));
            }
        }
    }
    p.push_str("\n\nThe person's question:\n");
    p.push_str(question.trim());
    p
}

/// The answer as shown: trimmed, at most [`MAX_ANSWER`] characters.
pub(crate) fn bounded(answer: &str) -> String {
    let answer = answer.trim();
    if answer.chars().count() <= MAX_ANSWER {
        return answer.to_owned();
    }
    let mut cut: String = answer.chars().take(MAX_ANSWER).collect();
    cut.push('…');
    cut
}

impl Engine {
    /// Part `id` of the project's map: its name and, on a model's map, what it does and its paths;
    /// on the folder map, the folder itself. `None` when the map has no such part.
    fn part_info(&self, project: &str, id: &str) -> Option<PartInfo> {
        let graph = self.project_graph(project, false).ok()?;
        let node = graph.nodes.into_iter().find(|n| n.id == id)?;
        let paths = match self.load_map(project) {
            Some(m) if map::node_name(&m, id).is_some() => map::node_paths(&m, id),
            _ if id == OTHER => Vec::new(),
            _ if id == "." => Vec::new(),
            _ => vec![id.to_owned()],
        };
        Some(PartInfo {
            id: id.to_owned(),
            name: if node.label.is_empty() {
                id.to_owned()
            } else {
                node.label
            },
            summary: node.summary,
            paths,
        })
    }

    /// Answers `req` in words; see [`vf_protocol::Backend::explain`].
    pub(crate) fn explain_question(
        &self,
        req: ExplainRequest,
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) {
        let error = |out: &mut dyn FnMut(ServerMessage), e: &str, code: &str| {
            out(ServerMessage::Error {
                error: e.to_owned(),
                code: Some(code.to_owned()),
            })
        };
        let Ok(dir) = self.projects.resolve(&req.project) else {
            return error(out, "unknown project", "not_found");
        };
        let question = req.question.trim().to_owned();
        if question.is_empty() || question.chars().count() > MAX_PROMPT {
            return error(
                out,
                &format!("question required (≤ {MAX_PROMPT} chars)"),
                "bad_prompt",
            );
        }
        let lang = req
            .lang
            .as_deref()
            .and_then(catalog::lang_code)
            .map(str::to_owned)
            .unwrap_or_else(|| vf_platform::system_language().as_str().to_owned());
        let project = req.project.clone();
        self.exclusive(&project, out, |out| {
            let part = match req.part.as_deref().filter(|id| *id != ROOT) {
                Some(id) => match self.part_info(&project, id) {
                    Some(p) => Some(p),
                    None => return error(out, "no such part on the map", "not_found"),
                },
                None => None,
            };
            let at = part.as_ref().map(|p| p.id.clone());
            let mut stream = canvas::Stream::new(&self.canvas, &project, out);
            stream.emit(CanvasEvent::Question {
                text: question.clone(),
                id: at.clone(),
            });
            stream.emit(CanvasEvent::Think {
                id: at.clone().unwrap_or_else(|| ROOT.into()),
                text: catalog::thought_explain(
                    Language::parse(&lang),
                    part.as_ref().map(|p| p.name.as_str()),
                ),
            });
            let answered = |text: String, code: Option<&str>| CanvasEvent::Explained {
                text,
                id: at.clone(),
                code: code.map(str::to_owned),
            };
            let list = self.explain_models(req.provider.as_deref(), req.model.as_deref());
            if list.is_empty() {
                return stream.emit(answered(String::new(), Some("no_models")));
            }
            let system =
                vf_platform::prompts::get(vf_core::prompts::Stage::Explain).render_explain(&lang);
            let text = prompt(&project, part.as_ref(), &question);
            let env = ChildEnv::from_current();
            let mut tried = Vec::new();
            for (provider, model, timeout) in list {
                if cancel.is_cancelled() {
                    return stream.emit(answered(String::new(), Some("cancelled")));
                }
                let Some(p) = self.provider(provider) else {
                    continue;
                };
                let ask = AskRequest {
                    dir: &dir,
                    system: Some(&system),
                    prompt: &text,
                    model: &model,
                    env: &env,
                    max_tokens: Some(MAX_ANSWER_TOKENS),
                    timeout,
                };
                let by = format!(
                    "{provider}/{}",
                    if model.is_empty() { "default" } else { &model }
                );
                match p.ask(&ask, cancel) {
                    Ok(a) if !a.text.trim().is_empty() => {
                        return stream.emit(answered(bounded(&a.text), None));
                    }
                    Ok(_) => tried.push(format!("{by}: empty answer")),
                    Err(e) => tried.push(format!("{by}: {e}")),
                }
            }
            if cancel.is_cancelled() {
                return stream.emit(answered(String::new(), Some("cancelled")));
            }
            tracing::info!(project = %project, tried = %tried.join("; "), "no model explained");
            stream.emit(answered(String::new(), Some("explain_failed")));
        });
    }

    /// Models to ask, in order: the one the person chose (when it can run now), then the ones the
    /// map asks, without repeats.
    fn explain_models(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Vec<(&'static str, String, std::time::Duration)> {
        let inventory = self.inventory();
        let auto = map::candidates(&inventory);
        let model = model.unwrap_or_default();
        let mut list = Vec::new();
        if let Some(chosen) = provider.filter(|p| *p != "auto" && *p != "plan")
            && self.available(&inventory, chosen, model)
            && let Some(id) = self.provider(chosen).map(|p| p.id())
        {
            let timeout = auto
                .iter()
                .find(|(p, ..)| *p == id)
                .map_or(map::API_TIMEOUT, |(_, _, t)| *t);
            list.push((id, model.to_owned(), timeout));
        }
        for c in auto {
            if !list.iter().any(|(p, m, _)| *p == c.0 && *m == c.1) {
                list.push(c);
            }
        }
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_puts_the_part_then_the_question_after_project_data() {
        let part = PartInfo {
            id: "p1".into(),
            name: "Notes".into(),
            summary: Some("Keeps your notes.".into()),
            paths: vec!["src/notes".into(), "src/store.ts".into()],
        };
        let p = prompt("notes-app", Some(&part), "  What does it keep?  ");
        assert!(p.starts_with("PROJECT DATA\n"));
        let at = |s: &str| p.find(s).unwrap();
        assert!(at("Part of the map: Notes") < at("What it does: Keeps your notes."));
        assert!(at("src/store.ts") < at("The person's question:"));
        assert!(p.ends_with("\nWhat does it keep?"));

        let p = prompt("notes-app", None, "What is this project?");
        assert!(!p.contains("Part of the map"));
    }

    #[test]
    fn many_paths_are_cut_and_counted() {
        let part = PartInfo {
            id: "p1".into(),
            name: "Data".into(),
            summary: None,
            paths: (0..MAX_PART_PATHS + 5).map(|i| format!("f{i}")).collect(),
        };
        let p = prompt("p", Some(&part), "q");
        assert!(p.contains("… and 5 more"));
        assert!(!p.contains(&format!("f{}", MAX_PART_PATHS)));
    }

    #[test]
    fn a_long_answer_is_cut_on_a_character() {
        let long = "ж".repeat(MAX_ANSWER + 10);
        let cut = bounded(&long);
        assert_eq!(cut.chars().count(), MAX_ANSWER + 1);
        assert!(cut.ends_with('…'));
        assert_eq!(bounded("  short \n"), "short");
    }
}
