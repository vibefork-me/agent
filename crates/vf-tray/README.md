# vf-tray

**Layer 4 (binaries)**

The tray binary `vibefork-tray`: icon by the agent's state, tooltip, menu and desktop notifications.
Talks to the core through the local API (`GET /v1/hello`) and the `vibefork` binary next to it; reads
the projects' session streams (`<state>/canvas/<project>.jsonl`) for what runs.

| | Windows / macOS | Linux |
|---|---|---|
| Tray | `tray-icon` + `tao` (Shell_NotifyIcon / NSStatusItem) | `ksni`: StatusNotifierItem over D-Bus in pure Rust (zbus), no GTK, no libappindicator, no libdbus |
| Notifications | `notify-rust`: WinRT toasts (AppUserModelID `me.vibefork.agent`, registered under `HKCU\Software\Classes\AppUserModelId`) / NSUserNotification | `notify-rust`: `org.freedesktop.Notifications` over zbus |

- Icon: stopped / waiting / working / needs you / error.
- Tooltip: project and stage with the time left («notes-app» — check, ≈ 1 min left).
- Menu: state, current activity, recent projects (`vibefork open --project <name>`), open Vibefork,
  open the log, updates, start / stop the agent, quit.
- Notifications: done, the agent waits for a decision, the build failed — only for fresh events.
- Menu item "Agent status…" opens the status window.
- Strings: `vf_core::catalog` (`crates/vf-core/locales/*.ftl`).

## The agent's windows

The same binary runs the agent's windows (`vf-ui`, egui), each in a process of its own (a window
needs its own event loop, apart from the tray's):

| Command | Window |
|---|---|
| `vibefork-tray dialog confirm \| text \| secret \| secret-optional` | A question of the core (`vf_platform::dialog`): strings in `VF_DIALOG_*` variables; exit code 0 — accepted (a value on stdout), 1 — rejected / closed / timed out, 3 — no window could be shown |
| `vibefork-tray first-run` | Tools folder (moved with `vf_platform::storage`), free room, connect an AI model (opens the editor). Opened once by the tray on a fresh installation (empty tools folder); marker `<state>/first-run.done` |
| `vibefork-tray status` | The tray's state and actions in a window, refreshed like the tray |

## Invariants

- A tray crash never stops the core.
- The tray holds no session and no secret; text from the session streams is used only as display text.

## May depend on

Only crates of a lower layer (see `layers.toml`, `ARCHITECTURE.md`). Today: `vf-core`, `vf-protocol`, `vf-platform`, `vf-builds`, `vf-ui`.
