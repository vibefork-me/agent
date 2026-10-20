# AGENTS.md — rules for anyone writing code here (people and AI models)

Read [ARCHITECTURE.md](ARCHITECTURE.md) first. The code is reviewed by people: write it so a reviewer
can see there are no hidden traps.

## Before you change anything
- Find the crate that owns the responsibility (table in ARCHITECTURE.md). Do not add logic to `vf-agent`.
- A crate may depend only on crates of a **lower layer** (`layers.toml`). `python3 scripts/check_layers.py` must pass.
- If you touch an **invariant** (ARCHITECTURE.md) or a 🔒 crate (`vf-auth`, `vf-server`, `vf-sandbox`, `vf-updater`), say so in the PR and add / update attack tests.

## Code rules
- No `unsafe` (forbidden by lint). Only `vf-platform` / `vf-sandbox` may use it, each site with a `// SAFETY:` comment.
- No `unwrap` / `expect` / `panic!` outside tests. Libraries return typed errors (`thiserror`); `anyhow` only in binaries.
- Untrusted input gets a type: paths → `ProjectPath`, secrets → `Secret<T>`, tokens → `SessionToken`.
- Domain crates (layer 0–1) do no IO; put IO behind a trait implemented in an adapter crate.
- Explicit over clever: no macro tricks, no deep generics, no hidden global state.
- Logging via `tracing` with structured fields; never log secrets; only `vf-cli` prints.
- Every public item has a doc comment saying what it guarantees.

## Dependencies
- Add a dependency only with a reason in the PR (what it saves, why not std / an existing crate).
- Versions live in the root `Cargo.toml` (`[workspace.dependencies]`).
- `cargo deny check` must pass (licenses, advisories, duplicates).

## Before you open a PR
```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/check_layers.py
```
Protocol types changed → regenerate `schema/`: `VF_UPDATE_SCHEMA=1 cargo test -p vf-server --test schema`.

## Commits
[Conventional Commits](https://www.conventionalcommits.org/), English: `feat(vf-auth): add pairing dialog`.
No AI co-author trailers.
