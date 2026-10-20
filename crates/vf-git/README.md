# vf-git

**Layer 2 (adapters)**

Every git operation in one place: clone, branches, checkpoints, `Vibefork-Customization` trailers, step diffs, rollback.

## Invariants

- All git calls go through this crate (easy to audit).

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`.
