//! The session stream of a project: canvas events are numbered, kept in the project's event log and
//! sent just before the messages they come from. Messages of the existing protocol that mean the same
//! as a canvas event produce both, so an editor of either kind follows the work.

use std::time::{SystemTime, UNIX_EPOCH};

use vf_journal::CanvasLog;
use vf_journal::timing::{Timings, scope_of};
use vf_protocol::canvas::{CanvasEvent, CheckKind, ResultKind, Stage};
use vf_protocol::ws::{ServerMessage, ToolchainStatus};

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
    /// Measured durations: with them, the run's time left follows every stage and step.
    timings: Option<Timings>,
    /// The stage the run is in.
    stage: Option<Stage>,
    /// Steps of the plan, once it is made.
    steps: Option<u32>,
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
            timings: None,
            stage: None,
            steps: None,
        }
    }

    /// Sends the run's time left (`eta`) after every new stage, the plan and every step, by
    /// `timings`; nothing when they measured none of what is left.
    pub(crate) fn estimate(&mut self, timings: Timings) {
        self.timings = Some(timings);
    }

    /// What the run produced: the `result` event carries it.
    pub(crate) fn result_artifacts(&mut self, artifacts: Vec<vf_protocol::canvas::Artifact>) {
        self.artifacts = artifacts;
    }

    /// Records and sends a canvas event, then the time left it changes.
    pub(crate) fn emit(&mut self, ev: CanvasEvent) {
        let eta = self.eta_after(&ev);
        self.record(ev);
        if let Some(eta) = eta {
            self.record(eta);
        }
    }

    /// The time left once `ev` happened: a new stage starts, the plan is made (all its steps are
    /// left), a step starts (it and the ones after it are left). A plan's `change` stage waits for
    /// its step.
    fn eta_after(&mut self, ev: &CanvasEvent) -> Option<CanvasEvent> {
        let (stage, steps_left) = match ev {
            CanvasEvent::Stage { id } => {
                if self.stage.replace(*id) == Some(*id)
                    || (*id == Stage::Change && self.steps.is_some())
                {
                    return None;
                }
                (*id, None)
            }
            CanvasEvent::Plan { steps } => {
                let n = u32::try_from(steps.len()).unwrap_or(u32::MAX);
                self.steps = Some(n);
                (Stage::Change, Some(n))
            }
            CanvasEvent::Step { index } => (
                Stage::Change,
                Some(self.steps?.saturating_sub(*index).max(1)),
            ),
            _ => return None,
        };
        let seconds = self.timings.as_ref()?.left(stage, steps_left)?;
        Some(CanvasEvent::Eta {
            scope: scope_of(stage),
            seconds,
        })
    }

    fn record(&mut self, ev: CanvasEvent) {
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
        ServerMessage::BuildDone { code, .. } => vec![CanvasEvent::Check {
            ok: *code == 0,
            text: if *code == 0 {
                "The build passed".into()
            } else {
                format!("The build failed (code {code})")
            },
            id: None,
            kind: Some(CheckKind::Build),
            exit: Some(*code),
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
                kind: Some(if *code == 0 {
                    ResultKind::Done
                } else {
                    ResultKind::Failed
                }),
                exit: Some(*code),
                id: None,
            },
        ],
        // A tool being prepared is work on the build's node: bytes while its download size is
        // known, an unknown amount otherwise.
        ServerMessage::Toolchain {
            status:
                ToolchainStatus::Installing | ToolchainStatus::Downloading | ToolchainStatus::Extracting,
            tool,
            done,
            total,
            eta,
            ..
        } => vec![match (done, total, eta) {
            (Some(done), Some(total), Some(eta)) => CanvasEvent::Progress {
                done: *done,
                total: *total,
                unit: Some("bytes".into()),
                eta: *eta,
                text: None,
                id: Some(BUILD_NODE.into()),
                spec: Some(tool.clone()),
            },
            _ => unknown_amount(Some(tool.clone())),
        }],
        // The build of the project: its length is not known in advance.
        ServerMessage::Estimate { .. } => vec![
            CanvasEvent::Stage { id: Stage::Check },
            unknown_amount(None),
        ],
        ServerMessage::Paused => vec![CanvasEvent::Paused],
        ServerMessage::Resumed => vec![CanvasEvent::Resumed],
        ServerMessage::Cancelled => vec![CanvasEvent::Result {
            text: "Cancelled".into(),
            detail: String::new(),
            artifacts: Vec::new(),
            kind: Some(ResultKind::Cancelled),
            exit: None,
            id: None,
        }],
        _ => Vec::new(),
    }
}

/// The newest records of a project's stream that are measured for estimates.
const MEASURED_RECORDS: usize = 2000;
/// At most this many other projects fill in what a project's own stream never measured.
const MEASURED_PROJECTS: usize = 20;

impl crate::Engine {
    /// How long stages and steps took in this project's earlier runs; what it never measured comes
    /// from the earlier runs of other projects, together.
    pub(crate) fn timings(&self, project: &str) -> Timings {
        let measure = |p: &str| {
            let last = self.canvas.last_seq(p).unwrap_or(0);
            let back = u64::try_from(MEASURED_RECORDS).unwrap_or(u64::MAX);
            let from = last.saturating_sub(back).saturating_add(1);
            Timings::measure(
                &self
                    .canvas
                    .read_from(p, from, MEASURED_RECORDS)
                    .unwrap_or_default(),
            )
        };
        let mut others = Timings::default();
        for other in self
            .projects
            .list()
            .iter()
            .filter(|p| *p != project)
            .take(MEASURED_PROJECTS)
        {
            others.merge(&measure(other));
        }
        measure(project).or(&others)
    }
}

/// The map node of the project's build and of the tools it needs: the root, as the map has no node
/// of its own for the build.
pub(crate) const BUILD_NODE: &str = crate::map::ROOT;

/// Work on the build's node of an amount not known in advance; `spec` is the tool being prepared.
fn unknown_amount(spec: Option<String>) -> CanvasEvent {
    CanvasEvent::Progress {
        done: 0,
        total: 0,
        unit: None,
        eta: 0,
        text: None,
        id: Some(BUILD_NODE.into()),
        spec,
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
    fn a_tool_download_is_progress_of_the_build_node() {
        let downloading = crate::step::toolchain_message(vf_toolchains::Progress::Downloading {
            tool: "java@temurin-21".into(),
            done: 188,
            total: Some(520),
            eta: Some(10),
        });
        assert_eq!(
            events_for(&downloading),
            [CanvasEvent::Progress {
                done: 188,
                total: 520,
                unit: Some("bytes".into()),
                eta: 10,
                text: None,
                id: Some("root".into()),
                spec: Some("java@temurin-21".into()),
            }]
        );
        let unknown = crate::step::toolchain_message(vf_toolchains::Progress::Downloading {
            tool: "rust@stable".into(),
            done: 700,
            total: None,
            eta: None,
        });
        assert_eq!(
            events_for(&unknown),
            [unknown_amount(Some("rust@stable".into()))]
        );
        let extracting = crate::step::toolchain_message(vf_toolchains::Progress::Extracting {
            tool: "rust@stable".into(),
        });
        assert_eq!(
            events_for(&extracting),
            [CanvasEvent::Progress {
                done: 0,
                total: 0,
                unit: None,
                eta: 0,
                text: None,
                id: Some("root".into()),
                spec: Some("rust@stable".into()),
            }]
        );
        assert!(events_for(&failed_or_ready()).is_empty());
        let failed = crate::step::toolchain_message(vf_toolchains::Progress::Failed {
            tool: "go@1.23".into(),
            error: "timed out".into(),
            code: "timed_out",
        });
        assert!(matches!(
            failed,
            ServerMessage::Toolchain { code: Some(ref c), .. } if c == "timed_out"
        ));
    }

    fn failed_or_ready() -> ServerMessage {
        crate::step::toolchain_message(vf_toolchains::Progress::Ready {
            tool: "rust@stable".into(),
            path: "tools/rust".into(),
            seconds: 3.0,
        })
    }

    #[test]
    fn a_build_is_work_of_unknown_length_on_the_build_node() {
        let estimate = ServerMessage::Estimate {
            verdict: "ok".into(),
            text: String::new(),
        };
        assert_eq!(
            events_for(&estimate),
            [
                CanvasEvent::Stage { id: Stage::Check },
                unknown_amount(None)
            ]
        );
    }

    fn at(seq: u64, at: u64, session: &str, ev: CanvasEvent) -> CanvasRecord {
        CanvasRecord {
            seq,
            at,
            session: session.into(),
            ev,
        }
    }

    /// One earlier task of `notes-app`: understand 2 s, plan 4 s, two steps of 10 s, check 6 s.
    fn earlier_task() -> Memory {
        let stage = |id| CanvasEvent::Stage { id };
        Memory(RefCell::new(vec![
            at(1, 0, "t1", stage(Stage::Understand)),
            at(2, 2_000, "t1", stage(Stage::Plan)),
            at(3, 6_000, "t1", stage(Stage::Change)),
            at(4, 6_000, "t1", CanvasEvent::Step { index: 0 }),
            at(5, 16_000, "t1", stage(Stage::Change)),
            at(6, 16_000, "t1", CanvasEvent::Step { index: 1 }),
            at(7, 26_000, "t1", stage(Stage::Check)),
            at(8, 32_000, "t1", stage(Stage::Done)),
        ]))
    }

    #[test]
    fn time_left_follows_the_stages_and_steps_by_earlier_runs() {
        let log = earlier_task();
        let timings = Timings::measure(&log.0.borrow());
        let mut sent = Vec::new();
        {
            let mut out = |m: ServerMessage| sent.push(m);
            let mut stream = Stream::new(&log, "notes-app", &mut out);
            stream.estimate(timings);
            stream.emit(CanvasEvent::Stage {
                id: Stage::Understand,
            });
            stream.forward(ServerMessage::Planning { rating: None });
            stream.forward(ServerMessage::Plan {
                planner: "m".into(),
                cost_usd: None,
                steps: ["Tags", "Colors", "Search"]
                    .iter()
                    .map(|t| PlanStepInfo {
                        title: (*t).into(),
                        complexity: "s".into(),
                        label: "l".into(),
                    })
                    .collect(),
            });
            for n in 1..=3 {
                stream.forward(started(Some(StepInfo {
                    n,
                    of: 3,
                    title: "step".into(),
                })));
            }
            stream.forward(ServerMessage::Estimate {
                verdict: "ok".into(),
                text: String::new(),
            });
        }
        let etas: Vec<u64> = sent
            .iter()
            .filter_map(|m| match m {
                ServerMessage::Canvas {
                    ev: CanvasEvent::Eta { scope, seconds },
                    ..
                } => {
                    assert_eq!(*scope, vf_protocol::canvas::EtaScope::Task);
                    Some(*seconds)
                }
                _ => None,
            })
            .collect();
        // Understand: 2 + 4 + 20 (change) + 6 + 0; plan: 4 + 20 + 6; the plan made: three steps of
        // 10 s and the check; every step: the steps left and the check; the check.
        assert_eq!(etas, [32, 30, 36, 36, 26, 16, 6]);
    }

    #[test]
    fn without_measurements_no_time_left_is_sent() {
        let log = Memory::default();
        let mut sent = Vec::new();
        {
            let mut out = |m: ServerMessage| sent.push(m);
            let mut stream = Stream::new(&log, "notes-app", &mut out);
            stream.estimate(Timings::default());
            stream.emit(CanvasEvent::Stage {
                id: Stage::Understand,
            });
        }
        assert_eq!(sent.len(), 1);
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
                    artifacts: Vec::new(),
                    kind: Some(ResultKind::Done),
                    exit: Some(0),
                    id: None,
                },
            ]
        );
        assert!(events_for(&started(None)).len() == 1);
    }

    #[test]
    fn outcomes_carry_codes_for_the_editor_to_word() {
        let failed = ServerMessage::Done {
            code: 3,
            provider: None,
            model: None,
            session_id: None,
            commit: None,
            diff: String::new(),
            status: String::new(),
            plan: false,
        };
        let Some(CanvasEvent::Result {
            text, kind, exit, ..
        }) = events_for(&failed).pop()
        else {
            panic!("no result")
        };
        // The English text stays for older editors.
        assert_eq!(text, "Failed (code 3)");
        assert_eq!((kind, exit), (Some(ResultKind::Failed), Some(3)));

        let Some(CanvasEvent::Result { kind, exit, .. }) =
            events_for(&ServerMessage::Cancelled).pop()
        else {
            panic!("no result")
        };
        assert_eq!((kind, exit), (Some(ResultKind::Cancelled), None));

        let build = ServerMessage::BuildDone {
            code: 2,
            artifacts: None,
            files: Vec::new(),
            seconds: 1.0,
        };
        assert_eq!(
            events_for(&build),
            [CanvasEvent::Check {
                ok: false,
                text: "The build failed (code 2)".into(),
                id: None,
                kind: Some(CheckKind::Build),
                exit: Some(2),
            }]
        );
    }

    #[test]
    fn events_without_codes_from_older_logs_still_read() {
        let ev: CanvasEvent =
            serde_json::from_str(r#"{"type":"result","text":"Done","detail":""}"#).unwrap();
        assert!(matches!(
            ev,
            CanvasEvent::Result {
                kind: None,
                exit: None,
                id: None,
                ..
            }
        ));
        let json = serde_json::to_value(CanvasEvent::Result {
            text: "Part rolled back".into(),
            detail: String::new(),
            artifacts: Vec::new(),
            kind: Some(ResultKind::PartRolledBack),
            exit: None,
            id: Some("p1".into()),
        })
        .unwrap();
        assert_eq!(json["kind"], "part_rolled_back");
        assert_eq!(json["id"], "p1");
        assert!(json.get("exit").is_none());
    }
}
