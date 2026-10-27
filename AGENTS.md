# AGENTS.md — rules for anyone writing code here (people and AI models)

Read [ARCHITECTURE.md](ARCHITECTURE.md) first. The code is reviewed by people: write it so a reviewer
can see there are no hidden traps.

## Before you change anything
- Find the crate that owns the responsibility (table in ARCHITECTURE.md). Do not add logic to `vf-agent`.
- A crate may depend only on crates of a **lower layer** (`layers.toml`). `python3 scripts/check_layers.py` must pass.
- If you touch an **invariant** (ARCHITECTURE.md) or a 🔒 crate (`vf-auth`, `vf-server`, `vf-sandbox`, `vf-updater`), say so in the PR and add / update attack tests.

## Code rules
- No `unsafe` (forbidden by lint). Only `vf-platform` / `vf-sandbox` may use it, each site with a `// SAFETY:` comment.
- The agent never panics on input. Outside tests: no `unwrap` / `expect` / `panic!` / `unreachable!` / `todo!` / `unimplemented!`, no byte slicing of strings (`get(..)`, `split_once`, char APIs) — all denied by lint. Tests may unwrap (`clippy.toml`). Libraries return typed errors (`thiserror`); `anyhow` only in binaries.
- Everything from the network is hostile: read bodies through `vf_core::LimitedBody` with a bound from `vf_core::limits` (never `.json()` / `.text()` / `.bytes()` of a response), index slices of network data with `get()`, size arithmetic is checked or saturating.
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
cargo xtask ci
```
`cargo xtask ci` runs every check CI runs on each PR: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/check_layers.py`; it stops at the first failure.

## Version
The agent version lives only in `[workspace.package] version` of the root `Cargo.toml`; every crate inherits it and `vibefork --version` prints it. `cargo xtask version` shows it, `cargo xtask version set X.Y.Z[-pre]` sets it (and refreshes `Cargo.lock`).
Protocol types changed → regenerate `schema/`: `VF_UPDATE_SCHEMA=1 cargo test -p vf-server --test schema`.

## Commits
[Conventional Commits](https://www.conventionalcommits.org/), English: `feat(vf-auth): add pairing dialog`.
No AI co-author trailers.
