//! Domain types and rules of the agent. **Layer 0: no IO** — no network, no disk, no OS calls.
//!
//! Everything here is plain data and pure logic, so it can be read and tested in isolation:
//! - [`ids`] — typed identifiers (a `ProjectId` can't be passed where a `TaskId` is expected);
//! - [`path`] — [`ProjectPath`]: a path that is guaranteed to stay inside a project root (invariant 8);
//! - [`task`] — the task state machine;
//! - [`secret`] — [`Secret`]: a wrapper whose `Debug` never prints the value (invariant 6);
//! - [`ports`] — traits implemented by adapters ([`ConfirmDialog`]);
//! - [`sandbox`] — what a sandboxed process may touch, and the launcher prefix;
//! - [`limits`] — upper bounds for data from the network ([`LimitedBody`]);
//! - [`progress`] — speed and time left of a download or an install.

pub mod catalog;
pub mod endpoint;
pub mod ids;
pub mod limits;
pub mod path;
pub mod permission;
pub mod ports;
pub mod progress;
pub mod prompts;
pub mod sandbox;
pub mod secret;
pub mod task;

pub use catalog::Language;
pub use endpoint::{endpoint_host_label, validate_custom_openai_endpoint};
pub use limits::{LimitedBody, TooLarge};
pub use path::{PathError, ProjectPath};
pub use permission::Permission;
pub use ports::{ConfirmDialog, dialog_field, dialog_text};
pub use secret::{Secret, clean_secret};
pub use task::{TaskState, TransitionError};
