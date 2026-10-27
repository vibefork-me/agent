//! The session stream of a project: canvas events are numbered, kept in the project's event log and
//! sent just before the messages they come from. Messages of the existing protocol that mean the same
//! as a canvas event produce both, so an editor of either kind follows the work.

use std::time::{SystemTime, UNIX_EPOCH};

use vf_journal::CanvasLog;
use vf_protocol::canvas::{CanvasEvent, Stage};
use vf_protocol::ws::ServerMessage;

/// Numbers and records the events of one run (a task or a build) of a project. The caller holds the
/// project exclusively, so numbers have no gaps and no repeats.
pub(crate) struct Stream<'a> {
    log: &'a dyn CanvasLog,
    project: String,
    session: String,
    next: u64,
    out: &'a mut dyn FnMut(ServerMessage),
    /// What the run produced, put into its `result` event.
    artifacts: Vec<vf_protocol::canvas::Artifact>,
}

impl<'a> Stream<'a> {
    /// Starts a run of `project`; numbering continues the project's event log.
    pub(crate) fn new(
        log: &'a dyn CanvasLog,
        project: &str,
        out: &'a mut dyn FnMut(ServerMessage),
    ) -> Self {
        let last = log.last_seq(project).unwrap_or_else(|e| {
            tracing::warn!(project, error = %e, "cannot read the event log");
            0
        });
        let next = last.saturating_add(1);
        Self {
            log,
            project: project.to_owned(),
            session: format!("{}-{next}", now_ms()),
            next,
            out,
            artifacts: Vec::new(),
        }
    }

    /// What the run produced: the `result` event carries it.
    pub(crate) fn result_artifacts(&mut self, artifacts: Vec<vf_protocol::canvas::Artifact>) {
        self.artifacts = artifacts;
    }

    /// Records and sends a canvas event.
    pub(crate) fn emit(&mut self, ev: CanvasEvent) {
        let record = vf_protocol::canvas::CanvasRecord {
            seq: self.next,
            at: now_ms(),
            session: self.session.clone(),
            ev,
        };
        if let Err(e) = self.log.append(&self.project, &record) {
            tracing::warn!(project = %self.project, error = %e, "cannot write the event log");
        }
        self.next = self.next.saturating_add(1);
        (self.out)(ServerMessage::Canvas {
            project: self.project.clone(),
            seq: record.seq,
            at: record.at,
            session: record.session,
            ev: record.ev,
        });
    }

    /// Sends the canvas events that mean the same as a message of the existing protocol, then the
    /// message: a final message (`done`, `cancelled`, `build_done`) stays the last one of the run.
    /// A canvas event raised by the work itself (`seq` 0) is numbered and recorded here instead.
    pub(crate) fn forward(&mut self, message: ServerMessage) {
        if let ServerMessage::Canvas { ev, .. } = message {
            return self.emit(ev);
        }
        for mut ev in events_for(&message) {
            if let CanvasEvent::Result { artifacts, .. } = &mut ev
                && artifacts.is_empty()
            {
                *artifacts = std::mem::take(&mut self.artifacts);
            }
            self.emit(ev);
        }
        (self.out)(message);
    }
}

/// Canvas events that mean the same as a message of the existing protocol.
pub(crate) fn events_for(message: &ServerMessage) -> Vec<CanvasEvent> {
    match message {
        ServerMessage::Planning { .. } => vec![CanvasEvent::Stage { id: Stage::Plan }],
        ServerMessage::Plan { steps, .. } => vec![CanvasEvent::Plan {
            steps: steps.iter().map(|s| s.title.clone()).collect(),
        }],
        ServerMessage::Started { step, .. } => {
            let mut events = vec![CanvasEvent::Stage { id: Stage::Change }];
            if let Some(step) = step {
                events.push(CanvasEvent::Step {
                    index: step.n.saturating_sub(1),
                });
            }
            events
        }
        ServerMessage::Estimate { .. } => vec![CanvasEvent::Stage { id: Stage::Check }],
        ServerMessage::BuildDone { code, .. } => vec![CanvasEvent::Check {
            ok: *code == 0,
            text: if *code == 0 {
                "The build passed".into()
            } else {
                format!("The build failed (code {code})")
            },
            id: None,
        }],
        ServerMessage::Done { code, status, .. } => vec![
            CanvasEvent::Stage { id: Stage::Done },
            CanvasEvent::Result {
                text: if *code == 0 {
                    "Done".into()
                } else {
                    format!("Failed (code {code})")
                },
                detail: status.clone(),
                artifacts: Vec::new(),
            },
        ],
        ServerMessage::Paused => vec![CanvasEvent::Paused],
        ServerMessage::Resumed => vec![CanvasEvent::Resumed],
        ServerMessage::Cancelled => vec![CanvasEvent::Result {
            text: "Cancelled".into(),
            detail: String::new(),
            artifacts: Vec::new(),
        }],
        _ => Vec::new(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use vf_journal::JournalError;
    use vf_protocol::canvas::CanvasRecord;
    use vf_protocol::ws::{PlanStepInfo, StepInfo};

    use super::*;

    #[derive(Default)]
    struct Memory(RefCell<Vec<CanvasRecord>>);

    impl CanvasLog for Memory {
        fn last_seq(&self, _: &str) -> Result<u64, JournalError> {
            Ok(self.0.borrow().last().map_or(0, |r| r.seq))
        }
        fn append(&self, _: &str, record: &CanvasRecord) -> Result<(), JournalError> {
            self.0.borrow_mut().push(record.clone());
            Ok(())
        }
        fn read_from(
            &self,
            _: &str,
            from: u64,
            limit: usize,
        ) -> Result<Vec<CanvasRecord>, JournalError> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|r| r.seq >= from)
                .take(limit)
                .cloned()
                .collect())
        }
    }

    fn started(step: Option<StepInfo>) -> ServerMessage {
        ServerMessage::Started {
            project: "p".into(),
            provider: "codex".into(),
            model: String::new(),
            resume: false,
            handoff: false,
            toolchain: Vec::new(),
            step,
            isolation: String::new(),
        }
    }

    #[test]
    fn old_messages_are_sent_with_their_canvas_events_numbered_and_logged() {
        let log = Memory::default();
        let mut sent = Vec::new();
        {
            let mut out = |m: ServerMessage| sent.push(m);
            let mut stream = Stream::new(&log, "p", &mut out);
            stream.emit(CanvasEvent::Task {
                text: "fix".into(),
                nodes: Vec::new(),
            });
            stream.forward(ServerMessage::Plan {
                planner: "m".into(),
                cost_usd: None,
                steps: vec![PlanStepInfo {
                    title: "Read".into(),
                    complexity: "s".into(),
                    label: "l".into(),
                }],
            });
            stream.forward(started(Some(StepInfo {
                n: 1,
                of: 1,
                title: "Read".into(),
            })));
            stream.forward(ServerMessage::Stderr {
                text: "noise".into(),
            });
        }
        let kinds: Vec<String> = sent
            .iter()
            .map(|m| match m {
                ServerMessage::Canvas { seq, ev, .. } => format!(
                    "{seq}:{}",
                    serde_json::to_value(ev).unwrap()["type"].as_str().unwrap()
                ),
                other => serde_json::to_value(other).unwrap()["type"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "1:task", "2:plan", "plan", "3:stage", "4:step", "started", "stderr"
            ]
        );
        assert_eq!(
            log.0.borrow().iter().map(|r| r.seq).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert!(
            log.0
                .borrow()
                .iter()
                .all(|r| r.session == log.0.borrow()[0].session)
        );
    }

    #[test]
    fn a_new_run_continues_the_numbering_in_a_new_session() {
        let log = Memory::default();
        let mut out = |_: ServerMessage| {};
        Stream::new(&log, "p", &mut out).emit(CanvasEvent::Stage {
            id: Stage::Understand,
        });
        let mut out = |_: ServerMessage| {};
        Stream::new(&log, "p", &mut out).emit(CanvasEvent::Stage {
            id: Stage::Understand,
        });
        let records = log.0.borrow();
        assert_eq!(records.iter().map(|r| r.seq).collect::<Vec<_>>(), [1, 2]);
        assert_ne!(records[0].session, records[1].session);
    }

    #[test]
    fn the_end_of_a_task_is_a_result() {
        let done = ServerMessage::Done {
            code: 0,
            provider: None,
            model: None,
            session_id: None,
            commit: None,
            diff: String::new(),
            status: "2 files changed".into(),
            plan: false,
        };
        assert_eq!(
            events_for(&done),
            [
                CanvasEvent::Stage { id: Stage::Done },
                CanvasEvent::Result {
                    text: "Done".into(),
                    detail: "2 files changed".into(),
                    artifacts: Vec::new()
                },
            ]
        );
        assert!(events_for(&started(None)).len() == 1);
    }
}
