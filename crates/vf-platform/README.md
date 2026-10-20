# vf-platform

**Layer 2 (adapters)**

Everything OS-specific: native dialogs, secret store (Keychain / Credential Manager / Secret Service), `vibefork://` registration, autostart, paths and disk info.

## Invariants

- Secrets are stored only in the OS secret store, never in files.
- `unsafe` only with a `// SAFETY:` justification.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`.
