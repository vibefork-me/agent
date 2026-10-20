# vf-auth

**Layer 2 (adapters)** · 🔒 **security boundary**

Pairing via a native dialog, non-extractable browser keys, challenge → session, revocation, confirm prompts.

## Invariants

- Invariants 2, 3, 6 (ARCHITECTURE.md).

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`.

## Review

Changes here need a second human reviewer and updated attack tests.
