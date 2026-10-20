# vf-tray

**Layer 4 (binaries)**

The tray binary: talks to the core through the local API.

## Invariants

- A tray crash never stops the core.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: none.
