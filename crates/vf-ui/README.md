# vf-ui

**Layer 2 (adapters)**

The agent's windows on egui / eframe (OpenGL through glow): a confirm prompt (Allow / Block), text
and secret prompts, the first run, the agent's status — in the Vibefork look (dark theme, the
editor's palette, large text and buttons).

The crate depends on no other crate of the agent. Every string comes from the caller, already in
the person's language; every answer goes back as a value. Any egui program can use it:

- **views** — `DialogView`, `FirstRunView`, `StatusView`: `view.ui(ui)` every frame, inside the
  program's own egui;
- **standalone windows** — `Shell::dialog`, `Shell::first_run`, `Shell::status`: open a native
  window, block until it closes, return the answer (`UiError::Open` when no window can be shown);
- **theme** — `theme::apply(ctx)`, `theme::primary_button`, `theme::secondary_button`.

```rust
use std::time::Duration;
use vf_ui::{Answer, Dialog, DialogKind, Shell};

let answer = Shell::new().dialog(Dialog {
    kind: DialogKind::Confirm,
    title: "Give access to the project?".into(),
    text: "Site: example.test\nProject: notes-app".into(),
    accept: "Allow".into(),
    reject: "Block".into(),
    timeout: Duration::from_secs(60),
})?;
assert!(matches!(answer, Answer::Accepted | Answer::Rejected | Answer::TimedOut));
```

## Behaviour

- Keyboard: Tab moves between the field and the buttons, Enter presses the focused button (in a
  field: submits), Esc rejects. A confirm prompt starts with the focus on the reject button.
- A fresh dialog takes no input for 600 ms: keys and clicks meant for another window are not answers.
- The dialog answers `TimedOut` when its time is up; a thin bar shows the time left. A closed window
  is `Rejected`.
- A secret is typed masked; an entered value is never printed by `Debug`.
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

## May depend on

No crate of the agent. External: `eframe` (egui), `thiserror`.
