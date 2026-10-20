# vf-server

**Layer 3 (edges)** · 🔒 **security boundary**

HTTP + WebSocket API on loopback only; Host / Origin checks; fallback UI from a signed bundle.

## Invariants

- Invariants 1, 2, 5 (ARCHITECTURE.md).

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`, `vf-protocol`, `vf-auth`.

## Review

Changes here need a second human reviewer and updated attack tests.
