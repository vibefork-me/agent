//! Journal storage: one JSON line per entry in `<state>/journal/<project>.jsonl`.

use std::io::Write as _;
use std::path::PathBuf;

use vf_journal::{JournalError, JournalStore};
use vf_protocol::http::JournalEntry;

use crate::projects::valid_name;

/// JSONL files in a folder.
#[derive(Debug, Clone)]
pub struct JournalFiles {
    dir: PathBuf,
}

impl JournalFiles {
    /// Journals in `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self, project: &str) -> Result<PathBuf, JournalError> {
        if !valid_name(project) {
            return Err(JournalError("invalid project name".into()));
        }
        Ok(self.dir.join(format!("{project}.jsonl")))
    }
}

impl JournalStore for JournalFiles {
    fn read(&self, project: &str) -> Result<Vec<JournalEntry>, JournalError> {
        let text = match std::fs::read(self.file(project)?) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(JournalError(e.to_string())),
        };
        // A broken line (e.g. cut by a crash) is skipped, the rest stays readable.
        Ok(text
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect())
    }

    fn append(&self, project: &str, entry: &JournalEntry) -> Result<(), JournalError> {
        let path = self.file(project)?;
        std::fs::create_dir_all(&self.dir).map_err(|e| JournalError(e.to_string()))?;
        let mut line = serde_json::to_string(entry).map_err(|e| JournalError(e.to_string()))?;
        line.push('\n');
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| JournalError(e.to_string()))?;
        f.write_all(line.as_bytes())
            .map_err(|e| JournalError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_read_and_broken_lines() {
        let dir = std::env::temp_dir().join(format!("vf-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let j = JournalFiles::new(&dir);
        assert_eq!(j.read("p"), Ok(Vec::new()));
        let e = JournalEntry {
            at: "t".into(),
            provider: "codex".into(),
            model: String::new(),
            prompt: "x".into(),
            session: None,
            handoff: false,
            summary: String::new(),
            commit: None,
            commits: Vec::new(),
            files: String::new(),
            ms: None,
            cost_usd: None,
            usage: None,
            prompt_version: None,
            artifacts: Vec::new(),
        };
        assert!(j.append("p", &e).is_ok());
        let path = dir.join("p.jsonl");
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).ok();
        assert!(
            f.as_mut()
                .is_some_and(|f| f.write_all(b"{broken\n").is_ok())
        );
        assert!(j.append("p", &e).is_ok());
        assert_eq!(j.read("p").map(|v| v.len()), Ok(2));
        assert!(j.read("../p").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
