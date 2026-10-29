# vf-providers

**Layer 2 (adapters)**

`Provider` trait and adapters: Claude Code CLI, Codex CLI, GitHub Copilot CLI, OpenAI-compatible loop.

## Invariants

- Coding CLIs run without the user's personal environment and with an allow-listed environment (invariant 7): tokens and keys of the user's environment are not passed.
- Raw output is normalized to `vf_protocol::Event`.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`, `vf-protocol`.
