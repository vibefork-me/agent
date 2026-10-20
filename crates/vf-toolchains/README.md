# vf-toolchains

**Layer 2 (adapters)**

Detect what a project needs (AGENTS.md → version files → heuristics → the model) and install it into the agent's folder via `mise`.

## Invariants

- Nothing is installed into the system.
- No admin rights required.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`.
