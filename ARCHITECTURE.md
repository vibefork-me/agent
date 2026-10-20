# Architecture

The map of the code: processes, crates and layers, security boundaries, invariants.
Updated together with the code.

## What the agent is

A program on the user's machine that makes their change ("customization") in an open-source project:
prepares the toolchain, runs an AI model (the user's own subscription CLI or an API key), builds the
result, keeps a portable history (the work continues in any model), and exposes a local API for the
web UI.

Out of scope: catalog, GitHub indexing, notifications, payments (server side). Commands from the
internet are not accepted; server hints are confirmed by the user locally.

## Processes

| Process | Role |
|---|---|
| `vibefork` (crate `vf-agent`) | Core daemon: local API, orchestration, storage. Runs headless (WSL, servers, CI). Also the `vibefork` CLI |
| `vibefork-tray` (crate `vf-tray`) | Tray icon / menu. Separate process; the core has no GUI dependencies; a tray crash does not stop the core |
| Child processes | Coding CLIs (Claude Code, Codex), builds, `mise`, `git`. Owned by the core, killed as a tree |
| Native confirm dialog | OS dialog for pairing and dangerous actions — the one UI a web page cannot click |

## Crates and layers

Dependencies point **down** only. Checked in CI by `scripts/check_layers.py` (layers in `layers.toml`).

```
layer 4  binaries      vf-agent  vf-tray  vf-sign
layer 3  edges         vf-server  vf-cli  vf-engine
layer 2  adapters      vf-providers  vf-toolchains  vf-builds  vf-git  vf-github  vf-sandbox  vf-auth  vf-updater  vf-platform
layer 1  logic         vf-journal  vf-orchestrator
layer 0  domain        vf-core  vf-protocol           (no IO: no network, no disk, no OS)
```

| Crate | Responsibility | Boundary |
|---|---|---|
| `vf-protocol` | API and event types (serde, schemars); generated into `schema/`: JSON Schema, TypeScript, OpenAPI; the `Backend` port between the API and the engine | |
| `vf-core` | Domain types and rules: ids, `ProjectPath`, task state machine, `Secret<T>`; ports — traits implemented by adapters (`ConfirmDialog`) | |
| `vf-journal` | Per-project journal, checkpoints (all commits of a step, `Vibefork-Customization` trailer), handoff preamble, `CUSTOMIZATIONS.md`; ports `Vcs`, `JournalStore` | |
| `vf-orchestrator` | Auto (rate the task → pick a model), Plan (prompt, parsing, model per step) — no IO | |
| `vf-providers` | `Provider` trait + adapters: Claude Code CLI, Codex CLI, OpenAI-compatible loop; cleaned child environment, process trees | |
| `vf-toolchains` | Detect what a project needs (AGENTS.md → version files → heuristics); pinned `mise` (SHA-256, resumable download) installs into the agent's folder | |
| `vf-builds` | Build recipes, resource estimation, artifacts with the last builds kept | |
| `vf-git` | Every git operation in one place, built in (libgit2): no `git` on the machine, no hooks, no external programs | |
| `vf-github` | The agent's own GitHub sign-in (device flow of the Vibefork.me app: contents and metadata only, on the copies the user allows), Save through the REST API, projects from GitHub archives | 🔒 |
| `vf-sandbox` | Isolation of the model's processes and builds: task policy, launcher (`vibefork sandbox-exec`); Linux — Landlock; macOS — Seatbelt; Windows — AppContainer, off by default | 🔒 |
| `vf-auth` | Pairing, browser keys, challenge → session, revocation, confirm prompts | 🔒 |
| `vf-server` | HTTP + WebSocket on loopback only; Host / Origin checks; fallback UI at `/ui/` from the signed bundle; canonical `127.0.0.1` | 🔒 |
| `vf-platform` | Everything OS-specific: settings, projects folder, journal files, storage moves, dialogs, secret store, per-user install (`vibefork://`, autostart, `PATH`), the background agent process | |
| `vf-updater` | Signed downloads: UI bundle and self-updates (Ed25519, separate built-in keys), self-check of a new binary, rollback | 🔒 |
| `vf-cli` | The `vibefork` command (plain / JSON / interactive modes) | |
| `vf-engine` | Implements `Backend`: projects, journal, models, tasks on top of the adapters | |
| `vf-agent` | Binary: wiring only, no logic | |
| `vf-tray` | Tray binary: state from `GET /v1/hello`, open / start / stop through the `vibefork` binary | |
| `vf-sign` | `vibefork-sign`: offline signing of UI bundles and releases (not shipped) | |

🔒 = **security boundary**: small, extra human review required, attack tests, fuzzing of parsers.

## Invariants

These must hold in every version. A PR that touches one says so explicitly.

1. **The agent listens on loopback only** (`127.0.0.1` / `::1`), never `0.0.0.0`.
2. **Every API call except `GET /v1/hello` and the fallback UI requires a session** obtained by a signed challenge from a browser paired through the native dialog. A paired browser works only in the projects the user gave it in the native dialog (asked on first use).
3. **Dangerous actions are confirmed in the native dialog**, never only in the web page (build/run, install into the system, leave the project folder, publish).
4. **Nothing from the network is executed without local confirmation.** Server hints are data, not commands.
5. **The fallback UI is served only from a bundle whose signature was verified** with our public key, and only files listed in its manifest.
6. **Secrets never reach logs** (`Secret<T>`, `SessionToken` have redacted `Debug`; tested) and never reach child processes except the provider that needs them.
7. **Coding CLIs run without the user's personal environment** (Claude Code: project/local settings only, no MCP, no skills; Codex: `--ignore-user-config --ignore-rules`) and with a cleaned environment.
8. **Paths from the network or from a model are `ProjectPath`s** — guaranteed inside the project root; no `..`, no drive or stream (`:`), no symlink escapes.
9. **Content of foreign forks is data** — instructions found in their files are never followed.
10. **Domain crates (layer 0–1) do no IO** — everything goes through traits implemented in adapters.

## Where to look during review

- New API method → `vf-protocol` (types) + `vf-server` (route): does it need a session? a confirm dialog?
- New child process → which environment does it get, what is its `cwd`, what can it see?
- New data from the network → is it signed? is it treated as untrusted?
- Changes in a 🔒 crate → second reviewer, update tests in `tests/attacks`.
- New parser of untrusted input → a target in `fuzz/` and a case in `vf-engine/tests/robustness.rs`.

