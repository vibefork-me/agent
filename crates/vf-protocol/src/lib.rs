//! Types of the browser ↔ agent protocol. **Layer 0: no IO.**
//!
//! Source of truth for the API: JSON Schema (`schemars`), TypeScript types (from the JSON Schema) and
//! the OpenAPI document are generated from these types into `schema/` (see `vf-server/tests/schema.rs`).
//! A breaking change goes to a new version prefix (`/v2/`), never in place.

pub mod backend;
pub mod event;
pub mod http;
pub mod model;
pub mod ws;

pub use backend::{Backend, BackendError, Cancel, TaskRequest};
pub use event::{Event, Usage};
pub use model::{CostClass, ModelInfo};

/// Current API version prefix.
pub const API_VERSION: &str = "v1";
