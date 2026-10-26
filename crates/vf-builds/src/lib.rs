//! Builds: how a project is built ([`recipe`]), whether this machine can do it ([`estimate`]),
//! where the results go ([`artifacts`]).
//!
//! Resource estimation uses layers from exact to rough; implemented now: ecosystem heuristics and
//! the maintainer's hint in `AGENTS.md` ("needs 8 GB RAM"). Own and community experience come later.

pub mod artifacts;
pub mod estimate;
pub mod recipe;

pub use estimate::{Machine, Needs, Verdict, estimate};
pub use recipe::{Ecosystem, Recipe, Step, detect_recipe, detect_recipe_os, venv_python};
