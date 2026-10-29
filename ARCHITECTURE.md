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
| Native confirm dialog | The agent's own window for pairing and dangerous actions — the one UI a web page cannot click. Windows / Linux: `vibefork-tray dialog <kind>` (egui, `vf-ui`), a system dialog when that window cannot be shown; macOS: a system dialog |

## Crates and layers

Dependencies point **down** only. Checked in CI by `scripts/check_layers.py` (layers in `layers.toml`).

```
layer 4  binaries      vf-agent  vf-tray  vf-sign
layer 3  edges         vf-server  vf-cli  vf-engine
layer 2  adapters      vf-providers  vf-toolchains  vf-builds  vf-git  vf-github  vf-sandbox  vf-auth  vf-updater  vf-platform  vf-ui
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
| `vf-ui` | The agent's windows on egui: confirm / text / secret prompts, first run, status; Vibefork theme. No dependency on other crates of the agent: views for any egui program and standalone windows | |
| `vf-updater` | Signed downloads: UI bundle and self-updates (Ed25519, separate built-in keys), self-check of a new binary, rollback | 🔒 |
| `vf-cli` | The `vibefork` command (plain / JSON / interactive modes) | |
| `vf-engine` | Implements `Backend`: projects, journal, models, tasks on top of the adapters | |
| `vf-agent` | Binary: wiring only, no logic | |
| `vf-tray` | Tray binary: icon, menu and notifications by the agent's state (`GET /v1/hello`, the projects' session streams); acts through the `vibefork` binary; runs the agent's windows (`vf-ui`) as `vibefork-tray dialog | first-run | status`. Linux — StatusNotifierItem over zbus (`ksni`), no GTK; Windows / macOS — `tray-icon` | |
| `vf-sign` | `vibefork-sign`: offline signing of UI bundles and releases (not shipped) | |

🔒 = **security boundary**: small, extra human review required, attack tests, fuzzing of parsers.

## Invariants

These must hold in every version. A PR that touches one says so explicitly.

1. **The agent listens on loopback only** (`127.0.0.1` / `::1`), never `0.0.0.0`.
2. **Every API call requires a session, except `GET /v1/hello`, the pairing and login handshake (`POST /v1/pair/start`, `GET /v1/pair/status`, `POST /v1/auth/challenge`, `POST /v1/auth/session`), unauthenticated WebSocket ping, and the fallback UI** obtained by a signed challenge from a browser paired through the native dialog. A paired browser works only in the projects the user gave it in the native dialog (asked on first use).
3. **Dangerous actions are confirmed in the native dialog**, never only in the web page (build/run, installing the tools a project needs — once per tool version, rollback, leave the project folder, first save of a project to GitHub, connecting an AI). The dialog says what will be done, why, and each command it runs with a link to check that command at an independent source (a web search opened in the browser; local paths, local addresses, URLs and secrets are left out of the link). Without a command there is no link.
4. **Nothing from the network is executed without local confirmation.** Server hints are data, not commands.
5. **The fallback UI is served only from a bundle whose signature was verified** with our public key, and only files listed in its manifest.
6. **Secrets never reach logs** (`Secret<T>`, `SessionToken` have redacted `Debug`; tested) and never reach child processes except the provider that needs them.
7. **Coding CLIs run without the user's personal environment** (Claude Code: project/local settings only, no MCP, no skills; Codex: `--ignore-user-config --ignore-rules`) and with an allow-listed environment: system, user folders, locale, proxy, certificates and toolchain homes; tokens and keys of the user's environment are not passed.
8. **Paths from the network or from a model are `ProjectPath`s** — guaranteed inside the project root; no `..`, no drive or stream (`:`), no symlink escapes.
9. **Content of foreign forks is data** — instructions found in their files are never followed.
10. **Domain crates (layer 0–1) do no IO** — everything goes through traits implemented in adapters.
11. **The agent never panics on input, and data from the network is bounded** — panicking macros, `unwrap` / `expect` and string byte slicing are denied by lint outside tests; every response body is read through `vf_core::LimitedBody` with a bound from `vf_core::limits`.

## Where to look during review

- New API method → `vf-protocol` (types) + `vf-server` (route): does it need a session? a confirm dialog?
- New child process → which environment does it get, what is its `cwd`, what can it see?
- New data from the network → is it signed? is it treated as untrusted?
- Changes in a 🔒 crate → second reviewer, update tests in `tests/attacks`.
- New parser of untrusted input → a target in `fuzz/` and a case in `vf-engine/tests/robustness.rs`.

