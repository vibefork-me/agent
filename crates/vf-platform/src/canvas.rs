//! Session stream storage: one JSON line per event in `<state>/canvas/<project>.jsonl`.

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::PathBuf;

use vf_journal::{CanvasLog, JournalError};
use vf_protocol::canvas::CanvasRecord;

use crate::projects::valid_name;

/// JSONL files in a folder.
#[derive(Debug, Clone)]
pub struct CanvasFiles {
    dir: PathBuf,
}

impl CanvasFiles {
    /// Event logs in `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self, project: &str) -> Result<PathBuf, JournalError> {
        if !valid_name(project) {
            return Err(JournalError("invalid project name".into()));
        }
        Ok(self.dir.join(format!("{project}.jsonl")))
    }

    /// Calls `f` for every readable record, oldest first, until it returns `false`. A broken line
    /// (e.g. cut by a crash) is skipped.
    fn scan(
        &self,
        project: &str,
        mut f: impl FnMut(CanvasRecord) -> bool,
    ) -> Result<(), JournalError> {
        let file = match std::fs::File::open(self.file(project)?) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(JournalError(e.to_string())),
        };
        for line in BufReader::new(file).split(b'\n') {
            let line = line.map_err(|e| JournalError(e.to_string()))?;
            if let Ok(record) = serde_json::from_slice::<CanvasRecord>(&line)
                && !f(record)
            {
                break;
            }
        }
        Ok(())
    }
}

impl CanvasLog for CanvasFiles {
    fn last_seq(&self, project: &str) -> Result<u64, JournalError> {
        let mut last = 0;
        self.scan(project, |r| {
            last = last.max(r.seq);
            true
        })?;
        Ok(last)
    }

    fn append(&self, project: &str, record: &CanvasRecord) -> Result<(), JournalError> {
        let path = self.file(project)?;
        std::fs::create_dir_all(&self.dir).map_err(|e| JournalError(e.to_string()))?;
        let mut line = serde_json::to_string(record).map_err(|e| JournalError(e.to_string()))?;
        line.push('\n');
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| JournalError(e.to_string()))?;
        f.write_all(line.as_bytes())
            .map_err(|e| JournalError(e.to_string()))
    }

    fn read_from(
        &self,
        project: &str,
        from: u64,
        limit: usize,
    ) -> Result<Vec<CanvasRecord>, JournalError> {
        let mut out = Vec::new();
        if limit == 0 {
            return Ok(out);
        }
        self.scan(project, |r| {
            if r.seq >= from {
                out.push(r);
            }
            out.len() < limit
        })?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use vf_protocol::canvas::{CanvasEvent, Stage};

    use super::*;

    fn record(seq: u64) -> CanvasRecord {
        CanvasRecord {
            seq,
            at: 1000 + seq,
            session: "s1".into(),
            ev: CanvasEvent::Stage { id: Stage::Plan },
        }
    }

    #[test]
    fn append_read_from_and_broken_lines() {
        let dir = std::env::temp_dir().join(format!("vf-canvas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = CanvasFiles::new(&dir);
        assert_eq!(log.last_seq("p"), Ok(0));
        assert_eq!(log.read_from("p", 1, 10), Ok(Vec::new()));
        for seq in 1..=3 {
            log.append("p", &record(seq)).unwrap();
        }
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join("p.jsonl"))
            .unwrap();
        f.write_all(b"{broken\n").unwrap();
        for seq in 4..=5 {
            log.append("p", &record(seq)).unwrap();
        }
        assert_eq!(log.last_seq("p"), Ok(5));
        let seqs = |v: Vec<CanvasRecord>| v.into_iter().map(|r| r.seq).collect::<Vec<_>>();
        assert_eq!(log.read_from("p", 1, 10).map(seqs), Ok(vec![1, 2, 3, 4, 5]));
        assert_eq!(log.read_from("p", 3, 2).map(seqs), Ok(vec![3, 4]));
        assert_eq!(log.read_from("p", 6, 10).map(seqs), Ok(Vec::new()));
        assert_eq!(log.read_from("p", 1, 0).map(seqs), Ok(Vec::new()));
        assert!(log.append("../p", &record(1)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
