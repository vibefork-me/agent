//! How long the stages of an analysis and of a task, and the steps of a plan, took before: measured
//! on the session streams the agent kept (the records' `at`), averaged, and turned into the time
//! left of a run. No guesses: a stage never measured adds nothing, and with nothing measured there
//! is no estimate.
//!
//! A stage lasts from its `stage` event until the next stage of the same session, or until the
//! session's last event. A step lasts from its `step` event until the next step or stage of the same
//! session, or until the session's last event. A stage repeated in a row (a plan enters `change`
//! before every step) goes on. Time spent paused counts as well.

use vf_protocol::canvas::{CanvasEvent, CanvasRecord, EtaScope, Stage};

/// Stages of an analysis, in order.
pub const ANALYSIS: [Stage; 5] = [
    Stage::Download,
    Stage::Parts,
    Stage::Links,
    Stage::Tools,
    Stage::Summary,
];

/// Stages of a task, in order.
pub const TASK: [Stage; 5] = [
    Stage::Understand,
    Stage::Plan,
    Stage::Change,
    Stage::Check,
    Stage::Done,
];

/// What a stage belongs to: an analysis or a task.
pub fn scope_of(stage: Stage) -> EtaScope {
    if ANALYSIS.contains(&stage) {
        EtaScope::Analysis
    } else {
        EtaScope::Task
    }
}

/// Sum of durations (ms) and how many were measured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Sum {
    ms: u64,
    count: u64,
}

impl Sum {
    fn add(&mut self, ms: u64) {
        self.ms = self.ms.saturating_add(ms);
        self.count = self.count.saturating_add(1);
    }

    fn average(self) -> Option<u64> {
        self.ms.checked_div(self.count)
    }
}

/// Measured durations of stages and plan steps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timings {
    stages: [Sum; 10],
    step: Sum,
}

fn slot(stage: Stage) -> usize {
    match stage {
        Stage::Download => 0,
        Stage::Parts => 1,
        Stage::Links => 2,
        Stage::Tools => 3,
        Stage::Summary => 4,
        Stage::Understand => 5,
        Stage::Plan => 6,
        Stage::Change => 7,
        Stage::Check => 8,
        Stage::Done => 9,
    }
}

impl Timings {
    /// Measures the stages and steps of `records` (oldest first, as a session stream keeps them).
    pub fn measure(records: &[CanvasRecord]) -> Self {
        let mut t = Self::default();
        // The open stage and step of the current session: what, and since when.
        let mut stage: Option<(Stage, u64)> = None;
        let mut step: Option<u64> = None;
        let mut session: Option<&str> = None;
        let mut last_at = 0;
        for r in records {
            if session != Some(r.session.as_str()) {
                t.close(&mut stage, &mut step, last_at);
                session = Some(r.session.as_str());
            }
            last_at = r.at;
            match &r.ev {
                CanvasEvent::Stage { id } if stage.is_some_and(|(s, _)| s == *id) => {}
                CanvasEvent::Stage { id } => {
                    t.close(&mut stage, &mut step, r.at);
                    stage = Some((*id, r.at));
                }
                CanvasEvent::Step { .. } => {
                    if let Some(since) = step.replace(r.at) {
                        t.step.add(r.at.saturating_sub(since));
                    }
                }
                _ => {}
            }
        }
        t.close(&mut stage, &mut step, last_at);
        t
    }

    fn close(&mut self, stage: &mut Option<(Stage, u64)>, step: &mut Option<u64>, at: u64) {
        if let Some((s, since)) = stage.take()
            && let Some(sum) = self.stages.get_mut(slot(s))
        {
            sum.add(at.saturating_sub(since));
        }
        if let Some(since) = step.take() {
            self.step.add(at.saturating_sub(since));
        }
    }

    /// Adds what `other` measured to these timings.
    pub fn merge(&mut self, other: &Self) {
        for (own, theirs) in self.stages.iter_mut().zip(other.stages.iter()) {
            own.ms = own.ms.saturating_add(theirs.ms);
            own.count = own.count.saturating_add(theirs.count);
        }
        self.step.ms = self.step.ms.saturating_add(other.step.ms);
        self.step.count = self.step.count.saturating_add(other.step.count);
    }

    /// These timings, with `other`'s for what these never measured.
    pub fn or(mut self, other: &Self) -> Self {
        for (own, theirs) in self.stages.iter_mut().zip(other.stages.iter()) {
            if own.count == 0 {
                *own = *theirs;
            }
        }
        if self.step.count == 0 {
            self.step = other.step;
        }
        self
    }

    /// Average duration of a stage, ms; `None` when it was never measured.
    pub fn stage_ms(&self, stage: Stage) -> Option<u64> {
        self.stages.get(slot(stage)).and_then(|s| s.average())
    }

    /// Average duration of a plan step, ms; `None` when none was measured.
    pub fn step_ms(&self) -> Option<u64> {
        self.step.average()
    }

    /// Seconds left of the analysis or task that enters `stage` now: the averages of this stage and
    /// of the stages after it. In `change` with `steps_left` plan steps to do (the one starting now
    /// included), the steps' average stands for the stage. `None` when none of them was measured.
    pub fn left(&self, stage: Stage, steps_left: Option<u32>) -> Option<u64> {
        let order: &[Stage] = if ANALYSIS.contains(&stage) {
            &ANALYSIS
        } else {
            &TASK
        };
        let from = order.iter().position(|s| *s == stage)?;
        let mut known = false;
        let mut ms: u64 = 0;
        for s in order.iter().skip(from) {
            let part = match (s, steps_left, self.step_ms()) {
                (Stage::Change, Some(n), Some(step)) => Some(step.saturating_mul(u64::from(n))),
                _ => self.stage_ms(*s),
            };
            if let Some(part) = part {
                known = true;
                ms = ms.saturating_add(part);
            }
        }
        known.then(|| ms.div_ceil(1000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(seq: u64, at: u64, session: &str, ev: CanvasEvent) -> CanvasRecord {
        CanvasRecord {
            seq,
            at,
            session: session.into(),
            ev,
        }
    }

    fn stage(id: Stage) -> CanvasEvent {
        CanvasEvent::Stage { id }
    }

    fn step(index: u32) -> CanvasEvent {
        CanvasEvent::Step { index }
    }

    /// Two tasks of a plan of two steps and one analysis of `notes-app`, times in ms by hand.
    fn history() -> Vec<CanvasRecord> {
        vec![
            // Task 1: understand 2 s, plan 4 s, steps 10 s and 20 s, check 6 s, done 0.
            r(1, 0, "t1", stage(Stage::Understand)),
            r(2, 2_000, "t1", stage(Stage::Plan)),
            r(3, 6_000, "t1", stage(Stage::Change)),
            r(4, 6_000, "t1", step(0)),
            r(5, 16_000, "t1", stage(Stage::Change)),
            r(6, 16_000, "t1", step(1)),
            r(7, 36_000, "t1", stage(Stage::Check)),
            r(8, 42_000, "t1", stage(Stage::Done)),
            // Analysis: parts 30 s, links and summary at once.
            r(9, 100_000, "a1", stage(Stage::Parts)),
            r(10, 130_000, "a1", stage(Stage::Links)),
            r(11, 130_000, "a1", stage(Stage::Summary)),
            // Task 2: understand 4 s, plan 8 s, one step of 30 s that ends the session.
            r(12, 200_000, "t2", stage(Stage::Understand)),
            r(13, 204_000, "t2", stage(Stage::Plan)),
            r(14, 212_000, "t2", stage(Stage::Change)),
            r(15, 212_000, "t2", step(0)),
            r(16, 242_000, "t2", CanvasEvent::Paused),
        ]
    }

    #[test]
    fn stages_and_steps_are_averaged_by_session() {
        let t = Timings::measure(&history());
        assert_eq!(t.stage_ms(Stage::Understand), Some(3_000));
        assert_eq!(t.stage_ms(Stage::Plan), Some(6_000));
        // Change goes on through its repeats: 30 s in task 1, 30 s in task 2.
        assert_eq!(t.stage_ms(Stage::Change), Some(30_000));
        assert_eq!(t.stage_ms(Stage::Check), Some(6_000));
        assert_eq!(t.stage_ms(Stage::Parts), Some(30_000));
        assert_eq!(t.stage_ms(Stage::Summary), Some(0));
        assert_eq!(t.stage_ms(Stage::Download), None);
        // Steps: 10 s, 20 s, 30 s.
        assert_eq!(t.step_ms(), Some(20_000));
    }

    #[test]
    fn time_left_is_the_sum_of_what_comes() {
        let t = Timings::measure(&history());
        // 3 + 6 + 30 + 6 + 0 s.
        assert_eq!(t.left(Stage::Understand, None), Some(45));
        // A plan of three steps: 3 × 20 s + 6 s.
        assert_eq!(t.left(Stage::Change, Some(3)), Some(66));
        assert_eq!(t.left(Stage::Check, None), Some(6));
        // Parts 30 s, links and summary 0, download and tools never measured.
        assert_eq!(t.left(Stage::Download, None), Some(30));
        assert_eq!(scope_of(Stage::Parts), EtaScope::Analysis);
        assert_eq!(scope_of(Stage::Check), EtaScope::Task);
    }

    #[test]
    fn nothing_measured_is_no_estimate() {
        let t = Timings::default();
        assert_eq!(t.left(Stage::Understand, Some(2)), None);
        let other = Timings::measure(&history());
        let t = t.or(&other);
        assert_eq!(t.left(Stage::Understand, None), Some(45));
        let own = Timings::measure(&[
            r(1, 0, "s", stage(Stage::Understand)),
            r(2, 1_000, "s", stage(Stage::Done)),
        ])
        .or(&other);
        let mut both = Timings::measure(&history());
        both.merge(&Timings::measure(&[
            r(1, 0, "s", stage(Stage::Check)),
            r(2, 12_000, "s", stage(Stage::Done)),
        ]));
        assert_eq!(both.stage_ms(Stage::Check), Some(9_000));
        // Its own understanding (1 s), the rest from the other history.
        assert_eq!(own.left(Stage::Understand, None), Some(43));
    }
}
