//! Rolling back one part of the map: the files of the part return to how they were before the last
//! task that changed it (all its plan steps), the other parts stay as they are. The rollback is a
//! checkpoint of its own with a journal entry and its "changes" artifact, so it can be undone too.

use vf_core::ProjectPath;
use vf_git::Repo;
use vf_journal::{JournalStore as _, StepReport, StepStart, Vcs as _};
use vf_protocol::BackendError;
use vf_protocol::canvas::Artifact;
use vf_protocol::http::JournalEntry;

use crate::Engine;
use crate::map::{self, ModelMap, OTHER};
use crate::step::now_rfc3339;

/// Provider name of the journal entries of rollbacks (whole project and one part).
pub(crate) const ROLLBACK: &str = "rollback";

/// What a rollback of a part did.
pub(crate) struct PartRolledBack {
    /// Files of the part that were returned, `/`-separated.
    pub(crate) files: Vec<String>,
    /// One sentence for the person.
    pub(crate) summary: String,
    /// The "changes" artifact of the rollback.
    pub(crate) artifacts: Vec<Artifact>,
}

/// `n` of a plan step's journal prompt `[step n/m] title`.
fn step_number(prompt: &str) -> Option<u32> {
    let rest = prompt.strip_prefix("[step ")?;
    let (n, _) = rest.split_once('/')?;
    n.parse().ok()
}

/// The name of part `id` of `map` (`p1`, `p2`…, a piece `p1.1`… or [`OTHER`]); `None` when the map
/// has no such part.
fn part_name(map: &ModelMap, id: &str) -> Option<String> {
    if id == OTHER {
        return Some("other files".into());
    }
    map::node_name(map, id).map(str::to_owned)
}

impl Engine {
    /// Returns the files of part `id` to how they were before the last task that changed them and
    /// journals it. The caller holds the project exclusively (no task is running).
    pub(crate) fn roll_back_part(
        &self,
        project: &str,
        id: &str,
    ) -> Result<PartRolledBack, BackendError> {
        let dir = self
            .projects
            .resolve(project)
            .map_err(|e| BackendError::NotFound(e.to_string()))?;
        let map = self
            .load_map(project)
            .ok_or_else(|| BackendError::NotFound("the project has no map of parts".into()))?;
        let name = part_name(&map, id)
            .ok_or_else(|| BackendError::NotFound("no such part on the map".into()))?;
        // A part owns the files of its pieces too.
        let owned =
            |file: &str| map::is_within(map::part_of(&map, file).as_deref().unwrap_or(OTHER), id);
        let journal = self
            .journal
            .read(project)
            .map_err(|e| BackendError::Internal(e.0))?;
        if journal.iter().all(|e| e.commits.is_empty()) {
            return Err(BackendError::BadRequest(
                "the agent has not changed this project".into(),
            ));
        }
        let repo = Repo::open(&dir);
        let head = repo.head().map_err(|e| BackendError::Internal(e.0))?;
        let base = last_task_base(&repo, &journal, &head, &owned)?.ok_or_else(|| {
            BackendError::BadRequest("the agent has not changed this part".into())
        })?;
        let files: Vec<String> = repo
            .changed_paths(&base, &head)
            .map_err(|e| BackendError::Internal(e.0))?
            .into_iter()
            .filter(|f| owned(f))
            .collect();
        if files.is_empty() {
            return Err(BackendError::BadRequest(
                "this part is already as it was before the last task changed it".into(),
            ));
        }

        let start = StepStart::begin(&repo).map_err(|e| BackendError::Internal(e.0))?;
        for f in &files {
            let path = ProjectPath::parse(f)
                .map_err(|e| BackendError::Internal(format!("{f}: {e}")))?
                .under(&dir);
            let before = repo
                .file_at(&base, f, u64::MAX)
                .map_err(|e| BackendError::Internal(e.0))?;
            let written = match before {
                Some((content, _)) => path
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&path, content)),
                None => remove_with_empty_folders(&path, &dir),
            };
            written.map_err(|e| BackendError::Internal(format!("{f}: {e}")))?;
        }
        let diff = repo
            .diff_from(start.base())
            .map_err(|e| BackendError::Internal(e.0))?;
        let part = |file: &str| map::part_of(&map, file);
        let artifacts: Vec<Artifact> = crate::changes::changes_artifact(&diff, &part)
            .into_iter()
            .collect();
        let summary = format!(
            "The part \"{name}\" was returned to how it was before the last task changed it; the \
             other parts were kept."
        );
        let report = StepReport {
            provider: ROLLBACK.into(),
            prompt: format!("roll back part {id} ({name})"),
            label: format!("roll back part {name}"),
            summary: summary.clone(),
            artifacts: artifacts.clone(),
            ..StepReport::default()
        };
        start
            .finish(&repo, &self.journal, project, now_rfc3339(), report)
            .map_err(|e| BackendError::Internal(e.0))?;
        Ok(PartRolledBack {
            files,
            summary,
            artifacts,
        })
    }
}

/// The commit before the last task that changed a file `owned` says is of the part: the parent of
/// the oldest commit of the task's steps (a plan's steps go back to its first one). Rollbacks are not
/// tasks, and a step a reset dropped from the history is skipped. `None`: no task changed the part.
fn last_task_base(
    repo: &Repo,
    journal: &[JournalEntry],
    head: &str,
    owned: &dyn Fn(&str) -> bool,
) -> Result<Option<String>, BackendError> {
    let internal = |e: vf_journal::JournalError| BackendError::Internal(e.0);
    let kept = |e: &JournalEntry| -> Result<bool, BackendError> {
        match e.commits.first() {
            Some(newest) if e.provider != ROLLBACK => {
                repo.in_history(newest, head).map_err(internal)
            }
            _ => Ok(false),
        }
    };
    for (i, e) in journal.iter().enumerate().rev() {
        if !kept(e)? {
            continue;
        }
        let (Some(newest), Some(oldest)) = (e.commits.first(), e.commits.last()) else {
            continue;
        };
        let changed = repo
            .changed_paths(&format!("{oldest}^"), newest)
            .map_err(internal)?;
        if !changed.iter().any(|f| owned(f)) {
            continue;
        }
        // Back to the first step of its plan: the oldest commit of the steps before it.
        let mut first = oldest.clone();
        let mut n = step_number(&e.prompt);
        for earlier in journal.get(..i).unwrap_or_default().iter().rev() {
            if !n.is_some_and(|n| n > 1) {
                break;
            }
            let Some(m) = step_number(&earlier.prompt) else {
                break;
            };
            if kept(earlier)?
                && let Some(c) = earlier.commits.last()
            {
                first = c.clone();
            }
            n = Some(m);
        }
        return Ok(Some(format!("{first}^")));
    }
    Ok(None)
}

/// Removes a file and the folders it leaves empty, up to `root`.
fn remove_with_empty_folders(
    path: &std::path::Path,
    root: &std::path::Path,
) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    }
    let mut parent = path.parent();
    while let Some(p) = parent.filter(|p| *p != root) {
        if std::fs::remove_dir(p).is_err() {
            break;
        }
        parent = p.parent();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_steps_are_numbered_from_their_prompt() {
        assert_eq!(step_number("[step 2/3] second"), Some(2));
        assert_eq!(step_number("do it"), None);
        assert_eq!(step_number("[step x/3] bad"), None);
    }

    #[test]
    fn parts_are_named_by_their_map_id() {
        let map: ModelMap = serde_json::from_str(
            r#"{"revision":"r","by":"t","parts":[{"name":"Notes","summary":"","paths":["notes"],
              "parts":[{"name":"Search","summary":"","paths":["notes/search"]}]}],"links":[]}"#,
        )
        .unwrap();
        assert_eq!(part_name(&map, "p1").as_deref(), Some("Notes"));
        assert_eq!(part_name(&map, "p1.1").as_deref(), Some("Search"));
        assert!(part_name(&map, "p1.2").is_none());
        assert!(part_name(&map, "p1.").is_none());
        assert!(part_name(&map, "p2").is_none());
        assert!(part_name(&map, "p0").is_none());
        assert!(part_name(&map, "notes").is_none());
        assert!(part_name(&map, OTHER).is_some());
    }
}
