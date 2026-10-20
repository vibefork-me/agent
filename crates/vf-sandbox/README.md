# vf-sandbox

**Layer 2 (adapters)** · 🔒 **security boundary**

`Sandbox` trait; the only implementation is `NoopSandbox` (no isolation yet).

## Invariants

- The real isolation level is reported honestly to the user.
- `unsafe` only with a `// SAFETY:` justification.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: none.

## Review

Changes here need a second human reviewer and updated attack tests.
