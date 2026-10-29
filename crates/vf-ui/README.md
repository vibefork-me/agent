# vf-ui

**Layer 2 (adapters)**

The agent's windows on egui / eframe (OpenGL through glow): a confirm prompt (Allow / Don't allow),
optionally for an action on the computer (what, why, and each command with a "Verify ↗" link), text
and secret prompts, the first run, the agent's status — in the Vibefork look (dark theme, the
editor's palette and typeface, large text and buttons).

The crate depends on no other crate of the agent. Every string comes from the caller, already in
the person's language; every answer goes back as a value. Any egui program can use it:

- **views** — `DialogView`, `FirstRunView`, `StatusView`: `view.ui(ui)` every frame, inside the
  program's own egui;
- **standalone windows** — `Shell::dialog`, `Shell::first_run`, `Shell::status`: open a native
  window, block until it closes, return the answer (`UiError::Open` when no window can be shown);
- **theme** — `theme::apply(ctx)`, `theme::primary_button`, `theme::secondary_button`,
  `theme::header`, `theme::section_label`, `theme::card`.

```rust
use std::time::Duration;
use vf_ui::{Answer, Dialog, DialogKind, Shell};

let question = Dialog {
    kind: DialogKind::Confirm,
    title: "Give access to the project?".into(),
    text: "Site: example.test\nProject: notes-app".into(),
    accept: "Allow".into(),
    reject: "Don't allow".into(),
    timeout: Duration::from_secs(60),
    permission: None,
};
// A clicked link goes to the caller: the window does not open a browser itself.
let answer = Shell::new().dialog(question, |link| println!("{link}"))?;
assert!(matches!(answer, Answer::Accepted | Answer::Rejected | Answer::TimedOut));
```

## Behaviour

- Keyboard: Tab moves between the field and the buttons, Enter presses the focused button (in a
  field: submits), Esc rejects. A confirm prompt starts with the focus on the reject button.
- A fresh dialog takes no input for 600 ms: keys and clicks meant for another window are not answers.
- The dialog answers `TimedOut` when its time is up; the time left is shown as `m:ss` next to the
  buttons. A closed window is `Rejected`.
- A secret is typed masked; an entered value is never printed by `Debug`.
- A permission shows what, why and the commands in a card, each part under its label; a command's
  "Verify ↗" link is kept only when it is a web search (`LINK_PREFIX`), and a click hands it to the
  caller.
- A window starts with the brand line "Vibefork" and the heading; a title that starts with the brand
  ("Vibefork: …") is shown under it without the brand.
- A standalone window takes the height of its content (160–680 points); longer content scrolls.
- Standalone windows ask to be brought to the front (they are usually opened by a background process);
  a dialog stays on top of other windows.
- Each standalone window runs its own event loop. Some window systems allow one event loop per process:
  show several windows in a row from separate processes, or draw the views into your own egui.

## Accessibility

AccessKit is on (eframe `accesskit`): screen readers (Narrator / NVDA through UI Automation, Orca
through AT-SPI, VoiceOver) get the window title, every label, button and field; a secret field is a
password field and its content is not exposed. Focus is always visible (accent ring); text on the
main button has contrast above 7:1, secondary text above 4.5:1.

## Invariants

- Text is plain: control characters and layout tricks (bidi controls, zero-width characters) are
  removed, the body is capped at 2000 characters and lines at 120; egui has no markup.
- No panics on input; no `unsafe`.

## Typeface

IBM Plex Sans (Regular, SemiBold) and IBM Plex Mono (Regular), built into the binary from `fonts/`;
licensed under the SIL Open Font License 1.1 (`fonts/OFL.txt`). egui's own fonts are the fallback
for characters Plex does not have.

## May depend on

No crate of the agent. External: `eframe` (egui), `thiserror`.
