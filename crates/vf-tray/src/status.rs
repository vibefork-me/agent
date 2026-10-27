//! What the agent does, read from the session streams of projects (`<state>/canvas/<project>.jsonl`):
//! the same events the editor gets over the WebSocket, kept on disk by the core. The tray only
//! reads them — no session, no API call — and trusts nothing in them beyond display text.

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vf_protocol::canvas::{CanvasEvent, CanvasRecord, Stage};

/// How much of the end of a stream is read: the last events, not the whole history.
const TAIL_BYTES: u64 = 256 * 1024;
/// Projects shown in "Recent projects".
pub(crate) const RECENT: usize = 5;
/// Files looked at in the streams folder (a guard against a folder full of junk).
const MAX_DIR_ENTRIES: usize = 10_000;
/// A waiting choice or a failed build older than this is no longer shown.
const SHOWN_FOR_MS: u64 = 60 * 60 * 1000;
/// A run without a new event for this long is taken as ended (e.g. the agent was killed).
const STALE_RUN_MS: u64 = 30 * 60 * 1000;

/// What the last run of a project is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Activity {
    /// Nothing runs.
    Idle,
    /// A run goes on: its stage and the time left, when known.
    Working {
        /// The current stage.
        stage: Option<Stage>,
        /// Seconds left.
        eta_secs: Option<u64>,
    },
    /// The analysis ended with offers: the person chooses what to do.
    NeedsYou,
    /// The last build or check of the run failed.
    Failed,
    /// The run ended.
    Done,
}

/// The activity of the most recent project, with the event it was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// Project name (a valid folder name).
    pub project: String,
    /// What it does.
    pub activity: Activity,
    /// Number of the last event.
    pub seq: u64,
    /// When the last event happened, Unix ms.
    pub at: u64,
}

/// Reads the activity of the most recently written stream and the names of recent projects
/// (most recent first). Missing folder or unreadable files give `None` / an empty list.
pub(crate) fn read(canvas_dir: &Path, now_ms: u64) -> (Option<Snapshot>, Vec<String>) {
    let recent = recent_streams(canvas_dir);
    let names: Vec<String> = recent.iter().map(|(name, _)| name.clone()).collect();
    let snapshot = recent.first().and_then(|(name, path)| {
        let records = tail(path)?;
        let last = records.last()?;
        Some(Snapshot {
            project: name.clone(),
            activity: derive(&records, now_ms),
            seq: last.seq,
            at: last.at,
        })
    });
    (snapshot, names)
}

/// The activity told by the events of the last run (records oldest first).
pub(crate) fn derive(records: &[CanvasRecord], now_ms: u64) -> Activity {
    let Some(last) = records.last() else {
        return Activity::Idle;
    };
    let age = now_ms.saturating_sub(last.at);
    let run: Vec<&CanvasRecord> = records
        .iter()
        .filter(|r| r.session == last.session)
        .collect();
    let last_check_failed = run
        .iter()
        .rev()
        .find_map(|r| match &r.ev {
            CanvasEvent::Check { ok, .. } => Some(!ok),
            _ => None,
        })
        .unwrap_or(false);
    let ended = match &last.ev {
        CanvasEvent::Result { .. } if last_check_failed => Activity::Failed,
        CanvasEvent::Result { .. } => Activity::Done,
        CanvasEvent::Check { ok: false, .. } => Activity::Failed,
        CanvasEvent::Offers { .. } => Activity::NeedsYou,
        CanvasEvent::Paused => Activity::Idle,
        _ => {
            if age > STALE_RUN_MS {
                return Activity::Idle;
            }
            let stage = run.iter().rev().find_map(|r| match &r.ev {
                CanvasEvent::Stage { id } => Some(*id),
                _ => None,
            });
            let eta_secs = run.iter().rev().find_map(|r| match &r.ev {
                CanvasEvent::Eta { seconds, .. } | CanvasEvent::Progress { eta: seconds, .. } => {
                    Some(seconds.saturating_sub(now_ms.saturating_sub(r.at) / 1000))
                }
                _ => None,
            });
            return Activity::Working { stage, eta_secs };
        }
    };
    match ended {
        Activity::Done => Activity::Done,
        _ if age > SHOWN_FOR_MS => Activity::Idle,
        other => other,
    }
}

/// Protocol id of a stage, as the catalog names it.
pub(crate) fn stage_id(stage: Stage) -> &'static str {
    match stage {
        Stage::Download => "download",
        Stage::Parts => "parts",
        Stage::Links => "links",
        Stage::Tools => "tools",
        Stage::Summary => "summary",
        Stage::Understand => "understand",
        Stage::Plan => "plan",
        Stage::Change => "change",
        Stage::Check => "check",
        Stage::Done => "done",
    }
}

/// Streams of valid project names, most recently written first, at most [`RECENT`].
fn recent_streams(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, String, PathBuf)> = entries
        .take(MAX_DIR_ENTRIES)
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_owned();
            if !vf_platform::projects::valid_name(&name) {
                return None;
            }
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, name, path))
        })
        .collect();
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    found
        .into_iter()
        .take(RECENT)
        .map(|(_, name, path)| (name, path))
        .collect()
}

/// The readable records in the last [`TAIL_BYTES`] of a stream, oldest first; a line cut by the
/// start of the window or by a crash is skipped.
fn tail(path: &Path) -> Option<Vec<CanvasRecord>> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes).ok()?;
    Some(parse_tail(&bytes, start > 0))
}

/// Records of a window of JSON lines; `cut` drops the first line (it may be a part of one).
fn parse_tail(bytes: &[u8], cut: bool) -> Vec<CanvasRecord> {
    bytes
        .split(|b| *b == b'\n')
        .skip(usize::from(cut))
        .filter_map(|line| serde_json::from_slice::<CanvasRecord>(line).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use vf_protocol::canvas::EtaScope;

    use super::*;

    const NOW: u64 = 10_000_000;

    fn rec(seq: u64, at: u64, session: &str, ev: CanvasEvent) -> CanvasRecord {
        CanvasRecord {
            seq,
            at,
            session: session.into(),
            ev,
        }
    }

    fn result() -> CanvasEvent {
        CanvasEvent::Result {
            text: "Done".into(),
            detail: String::new(),
            artifacts: Vec::new(),
        }
    }

    fn check(ok: bool) -> CanvasEvent {
        CanvasEvent::Check {
            ok,
            text: "build".into(),
            id: None,
        }
    }

    #[test]
    fn empty_is_idle() {
        assert_eq!(derive(&[], NOW), Activity::Idle);
    }

    #[test]
    fn working_with_stage_and_eta() {
        let records = [
            rec(1, NOW - 9_000, "t1", CanvasEvent::Stage { id: Stage::Plan }),
            rec(
                2,
                NOW - 5_000,
                "t1",
                CanvasEvent::Stage { id: Stage::Check },
            ),
            rec(
                3,
                NOW - 20_000,
                "t1",
                CanvasEvent::Eta {
                    scope: EtaScope::Task,
                    seconds: 80,
                },
            ),
            rec(
                4,
                NOW - 1_000,
                "t1",
                CanvasEvent::Think {
                    id: "core".into(),
                    text: "looking".into(),
                },
            ),
        ];
        assert_eq!(
            derive(&records, NOW),
            Activity::Working {
                stage: Some(Stage::Check),
                eta_secs: Some(60),
            }
        );
    }

    #[test]
    fn only_the_last_run_counts() {
        let records = [
            rec(
                1,
                NOW - 9_000,
                "old",
                CanvasEvent::Stage { id: Stage::Check },
            ),
            rec(2, NOW - 8_000, "old", check(false)),
            rec(
                3,
                NOW - 1_000,
                "new",
                CanvasEvent::Stage { id: Stage::Plan },
            ),
            rec(4, NOW - 500, "new", result()),
        ];
        assert_eq!(derive(&records, NOW), Activity::Done);
        let records = [
            rec(1, NOW - 1_000, "b", CanvasEvent::Stage { id: Stage::Check }),
            rec(2, NOW - 500, "b", check(true)),
            rec(3, NOW - 400, "b", check(false)),
            rec(4, NOW - 300, "b", result()),
        ];
        assert_eq!(derive(&records, NOW), Activity::Failed);
    }

    #[test]
    fn failed_offers_and_pause() {
        assert_eq!(
            derive(&[rec(1, NOW, "b", check(false))], NOW),
            Activity::Failed
        );
        let offers = CanvasEvent::Offers { items: Vec::new() };
        assert_eq!(
            derive(&[rec(1, NOW, "a", offers.clone())], NOW),
            Activity::NeedsYou
        );
        assert_eq!(
            derive(&[rec(1, NOW, "a", CanvasEvent::Paused)], NOW),
            Activity::Idle
        );
        // Old choices and failures are no longer shown; an old run is over.
        let old = NOW - SHOWN_FOR_MS - 1;
        assert_eq!(derive(&[rec(1, old, "a", offers)], NOW), Activity::Idle);
        assert_eq!(
            derive(&[rec(1, old, "b", check(false))], NOW),
            Activity::Idle
        );
        let stale = NOW - STALE_RUN_MS - 1;
        let stage = CanvasEvent::Stage { id: Stage::Change };
        assert_eq!(derive(&[rec(1, stale, "c", stage)], NOW), Activity::Idle);
    }

    #[test]
    fn tail_skips_cut_and_broken_lines() {
        let a = serde_json::to_string(&rec(1, 1, "s", CanvasEvent::Paused)).unwrap();
        let b = serde_json::to_string(&rec(2, 2, "s", CanvasEvent::Resumed)).unwrap();
        let text = format!("{a}\n{{broken\n{b}\n");
        assert_eq!(parse_tail(text.as_bytes(), false).len(), 2);
        assert_eq!(parse_tail(text.as_bytes(), true).len(), 1);
    }

    #[test]
    fn reads_the_most_recent_project() {
        let dir = std::env::temp_dir().join(format!("vf-tray-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let line = |r: CanvasRecord| format!("{}\n", serde_json::to_string(&r).unwrap());
        std::fs::write(dir.join("old-app.jsonl"), line(rec(1, NOW, "s", result()))).unwrap();
        std::fs::write(dir.join("not a project.jsonl"), "").unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        // A later write time makes `notes-app` the most recent.
        std::thread::sleep(std::time::Duration::from_millis(50));
        let stage = CanvasEvent::Stage { id: Stage::Plan };
        std::fs::write(dir.join("notes-app.jsonl"), line(rec(7, NOW, "s", stage))).unwrap();
        let (snapshot, recent) = read(&dir, NOW);
        assert_eq!(recent, vec!["notes-app".to_owned(), "old-app".to_owned()]);
        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.project, "notes-app");
        assert_eq!(snapshot.seq, 7);
        assert!(matches!(snapshot.activity, Activity::Working { .. }));
        assert_eq!(read(&dir.join("missing"), NOW), (None, Vec::new()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
