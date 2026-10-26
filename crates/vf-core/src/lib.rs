//! Domain types and rules of the agent. **Layer 0: no IO** — no network, no disk, no OS calls.
//!
//! Everything here is plain data and pure logic, so it can be read and tested in isolation:
//! - [`ids`] — typed identifiers (a `ProjectId` can't be passed where a `TaskId` is expected);
//! - [`path`] — [`ProjectPath`]: a path that is guaranteed to stay inside a project root (invariant 8);
//! - [`task`] — the task state machine;
//! - [`secret`] — [`Secret`]: a wrapper whose `Debug` never prints the value (invariant 6);
//! - [`ports`] — traits implemented by adapters ([`ConfirmDialog`]);
//! - [`sandbox`] — what a sandboxed process may touch, and the launcher prefix.

pub mod catalog;
pub mod ids;
pub mod path;
pub mod ports;
pub mod prompts;
pub mod sandbox;
pub mod secret;
pub mod task;

pub use catalog::Language;
pub use path::{PathError, ProjectPath};
pub use ports::{ConfirmDialog, dialog_field, dialog_text};
pub use secret::Secret;
pub use task::{TaskState, TransitionError};
