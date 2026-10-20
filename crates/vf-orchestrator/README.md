# vf-orchestrator

**Layer 1 (logic)**

Auto (rate the task → pick a model), Plan (a strong model splits the task, each step goes to the cheapest suitable model), Continue (native resume or handoff from the journal).

## Invariants

- No IO: providers and storage are traits.
- By default: only free models or models included in the user's subscriptions, unless the user chooses otherwise.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`, `vf-protocol`.
