# vf-core

**Layer 0 (domain)**

Domain types and rules: typed ids, `ProjectPath`, task state machine, `Secret<T>`.

## Invariants

- No IO: no network, no disk, no OS calls.
- `ProjectPath` never leaves the project root (lexically).
- `Secret<T>` never prints its value.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `serde`, `thiserror`.
