# vf-protocol

**Layer 0 (domain)**

Types of the browser ↔ agent protocol; source of truth for OpenAPI, JSON Schema and TypeScript types.

## Invariants

- No IO.
- Breaking changes only under a new version prefix (`/v2/`).

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `serde`.
