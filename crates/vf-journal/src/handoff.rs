//! Context for a model that did not see the previous work (another provider or model).

use vf_protocol::http::JournalEntry;

/// How many recent entries go into a handoff.
pub const HANDOFF_ENTRIES: usize = 8;

/// The preamble put before the new task; empty when there is no history.
pub fn handoff_preamble(entries: &[JournalEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let start = entries.len().saturating_sub(HANDOFF_ENTRIES);
    let lines: Vec<String> = entries[start..]
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let model = if e.model.is_empty() {
                String::new()
            } else {
                format!(" / {}", e.model)
            };
            let mut s = format!("{}. Task ({}{model}): {}", i + 1, e.provider, e.prompt);
            if !e.summary.is_empty() {
                s.push_str(&format!(
                    "\n   Outcome: {}",
                    e.summary.chars().take(600).collect::<String>()
                ));
            }
            match &e.commit {
                Some(c) => s.push_str(&format!(
                    "\n   Checkpoint commit: {c}{}",
                    if e.files.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", e.files)
                    }
                )),
                None => s.push_str("\n   No file changes."),
            }
            s
        })
        .collect();
    format!(
        "You are continuing work on this project that was started in another session (possibly another AI model).\n\
         History of earlier tasks (most recent last):\n{}\n\n\
         The current state of the code is in this folder (see `git log` / `git show` for the checkpoints above). \
         Do not redo finished work.\n\nNew task:\n",
        lines.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(prompt: &str, commit: Option<&str>) -> JournalEntry {
        JournalEntry {
            at: "2026-10-20T00:00:00Z".into(),
            provider: "claude".into(),
            model: "haiku".into(),
            prompt: prompt.into(),
            session: None,
            handoff: false,
            summary: "done".into(),
            commit: commit.map(str::to_owned),
            commits: commit.map(|c| vec![c.to_owned()]).unwrap_or_default(),
            files: if commit.is_some() {
                "2 files".into()
            } else {
                String::new()
            },
            ms: None,
            cost_usd: None,
            usage: None,
        }
    }

    #[test]
    fn empty_history_gives_no_preamble() {
        assert_eq!(handoff_preamble(&[]), "");
    }

    #[test]
    fn preamble_lists_recent_tasks_and_commits() {
        let entries: Vec<JournalEntry> = (0..10)
            .map(|i| entry(&format!("task {i}"), (i % 2 == 0).then_some("abc1234")))
            .collect();
        let p = handoff_preamble(&entries);
        assert!(
            p.contains("task 9") && p.contains("task 2") && !p.contains("task 1\n"),
            "only the last 8"
        );
        assert!(p.contains("Checkpoint commit: abc1234 (2 files)"));
        assert!(p.contains("No file changes."));
        assert!(p.ends_with("New task:\n"));
    }
}
