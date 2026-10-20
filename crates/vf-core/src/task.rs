//! The task state machine. Every task (and every plan step) moves only along these edges.
//!
//! ```text
//! Queued → PreparingToolchain → Running → Checkpointing → Done
//!    └──────────┴──────────────────┴───────────┴─────────→ Failed | Cancelled
//! ```

use serde::{Deserialize, Serialize};

/// State of a task or a plan step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Accepted, waiting to start.
    Queued,
    /// Detecting and installing the toolchain.
    PreparingToolchain,
    /// The model is working.
    Running,
    /// Recording the result: diff, checkpoint commit, journal entry.
    Checkpointing,
    /// Finished successfully.
    Done,
    /// Finished with an error.
    Failed,
    /// Cancelled by the user.
    Cancelled,
}

/// An edge that is not part of the state machine.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid task transition {from:?} → {to:?}")]
pub struct TransitionError {
    /// Current state.
    pub from: TaskState,
    /// Requested state.
    pub to: TaskState,
}

impl TaskState {
    /// Whether the task is finished.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }

    /// Moves to `to` if the edge exists.
    pub fn transition(self, to: TaskState) -> Result<TaskState, TransitionError> {
        use TaskState::*;
        let ok = match (self, to) {
            (Queued, PreparingToolchain)
            | (PreparingToolchain, Running)
            | (Running, Checkpointing)
            | (Checkpointing, Done) => true,
            (from, Failed | Cancelled) => !from.is_terminal(),
            _ => false,
        };
        if ok {
            Ok(to)
        } else {
            Err(TransitionError { from: self, to })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TaskState::*;

    #[test]
    fn happy_path() {
        let s = Queued
            .transition(PreparingToolchain)
            .and_then(|s| s.transition(Running));
        let s = s
            .and_then(|s| s.transition(Checkpointing))
            .and_then(|s| s.transition(Done));
        assert_eq!(s, Ok(Done));
    }

    #[test]
    fn no_skipping_and_no_leaving_terminal_states() {
        assert!(Queued.transition(Running).is_err());
        assert!(Done.transition(Running).is_err());
        assert!(Cancelled.transition(Failed).is_err());
        assert_eq!(Running.transition(Cancelled), Ok(Cancelled));
    }
}
