# vf-builds

**Layer 2 (adapters)**

Build recipes, artifacts, rollback to the previous build, resource estimation, choosing how to build for another OS.

## Invariants

- The previous artifact is kept until the new one is verified.
- Never start a build that the resource estimate marks as impossible without telling the user.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`.
