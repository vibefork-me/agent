# vf-updater

**Layer 2 (adapters)** · 🔒 **security boundary**

Self-update with signature verification and rollback on a failed start.

## Invariants

- Nothing is replaced before its signature is verified with the built-in public key.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`.

## Review

Changes here need a second human reviewer and updated attack tests.
