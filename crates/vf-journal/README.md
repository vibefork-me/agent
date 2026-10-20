# vf-journal

**Layer 1 (logic)**

Per-project journal (tasks, models, outcomes, sessions, commits, cost) and checkpoints — the provider-independent history that makes "continue in another model" possible.

## Invariants

- No IO: storage and git go through traits.
- Schema is versioned; old entries stay readable.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`.
