# vf-providers

**Layer 2 (adapters)**

`Provider` trait and adapters: Claude Code CLI, Codex CLI, OpenAI-compatible loop.

## Invariants

- Coding CLIs run without the user's personal environment and with a cleaned environment (invariant 7).
- Raw output is normalized to `vf_protocol::Event`.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`, `vf-protocol`.
