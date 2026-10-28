//! The agent's windows (`vf-ui`), each in a process of its own: `vibefork-tray dialog <kind>` for
//! the core's questions, `vibefork-tray first-run`, `vibefork-tray status`.

use std::cell::RefCell;
use std::io::Write as _;
use std::path::Path;
use std::time::Duration;

use vf_core::catalog;
use vf_platform::dialog::window;
use vf_ui::{Answer, Dialog, DialogKind, FirstRun, FirstRunText, Shell, StatusPage, Tone};

use crate::app::{self, Action, App, Entry, Look, Model, Notice};
use crate::icon;

/// Command: the first-run window.
pub(crate) const FIRST_RUN: &str = "first-run";
/// Command: the status window.
pub(crate) const STATUS: &str = "status";
/// Marker in the state folder: the first-run window was answered.
const FIRST_RUN_DONE: &str = "first-run.done";
/// Longest timeout a caller may ask of a dialog.
const MAX_TIMEOUT_SECS: u64 = 600;

/// Runs a window command; `None` when `args` is not one (the tray itself runs).
pub(crate) fn command(args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str) {
        Some(window::COMMAND) => Some(dialog(args.get(1).map(String::as_str).unwrap_or(""))),
        Some(FIRST_RUN) => Some(first_run()),
        Some(STATUS) => Some(status()),
        _ => None,
    }
}

fn shell() -> Shell {
    Shell::new().with_icon(icon::rgba(Look::Waiting), icon::SIZE, icon::SIZE)
}

/// `dialog <kind>`: the question in the environment, the answer as the exit code (and the value on
/// stdout). See `vf_platform::dialog::window`.
fn dialog(kind: &str) -> i32 {
    let kind = match kind {
        window::CONFIRM => DialogKind::Confirm,
        window::TEXT => DialogKind::Text,
        window::SECRET => DialogKind::Secret { allow_empty: false },
        window::SECRET_OPTIONAL => DialogKind::Secret { allow_empty: true },
        _ => return window::EXIT_NO_WINDOW,
    };
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let timeout = env(window::ENV_TIMEOUT_SECS)
        .parse::<u64>()
        .ok()
        .filter(|s| (1..=MAX_TIMEOUT_SECS).contains(s))
        .unwrap_or(60);
    let question = Dialog {
        kind,
        title: env(window::ENV_TITLE),
        text: env(window::ENV_TEXT),
        accept: env(window::ENV_ACCEPT),
        reject: env(window::ENV_REJECT),
        timeout: Duration::from_secs(timeout),
    };
    match shell().dialog(question) {
        Ok(Answer::Accepted) => window::EXIT_ACCEPTED,
        Ok(Answer::Entered(value)) => {
            let mut out = std::io::stdout().lock();
            if out
                .write_all(value.as_bytes())
                .and_then(|()| out.flush())
                .is_ok()
            {
                window::EXIT_ACCEPTED
            } else {
                window::EXIT_REJECTED
            }
        }
        Ok(Answer::Rejected | Answer::TimedOut) => window::EXIT_REJECTED,
        Err(_) => window::EXIT_NO_WINDOW,
    }
}

/// Opens the first-run window once, on a fresh installation (the tools folder is still empty).
/// An installation that already has tools is marked as done without asking.
pub(crate) fn first_run_if_new() {
    let settings = vf_platform::Settings::from_env();
    let marker = settings.state_dir.join(FIRST_RUN_DONE);
    if marker.exists() {
        return;
    }
    if vf_platform::storage::usage(&settings.storage.tools).files > 0 {
        mark_first_run(&marker);
        return;
    }
    app::spawn_window(FIRST_RUN);
}

fn mark_first_run(marker: &Path) {
    if let Some(dir) = marker.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(marker, b"");
}

/// `first-run`: the tools folder, the room it takes, connecting an AI model.
fn first_run() -> i32 {
    let lang = vf_platform::system_language();
    let mut settings = vf_platform::Settings::from_env();
    let tools = settings.storage.tools.clone();
    let free = vf_builds::Machine::measure(&tools).disk_free;
    // What the tools take is not estimated yet: only the free room is shown.
    let space = catalog::first_run_space(lang, None, free);
    let run = FirstRun {
        text: FirstRunText {
            title: catalog::first_run_title(lang),
            intro: catalog::first_run_intro(lang),
            folder: catalog::first_run_folder(lang),
            folder_hint: catalog::first_run_folder_hint(lang),
            connect_ai: catalog::first_run_connect_ai(lang),
            connect_ai_hint: catalog::first_run_connect_ai_hint(lang),
            start: catalog::first_run_start(lang),
            later: catalog::first_run_later(lang),
        },
        tools_dir: tools.display().to_string(),
        space: Some(space),
    };
    let answer = match shell().first_run(run) {
        Ok(a) => a,
        Err(_) => return window::EXIT_NO_WINDOW,
    };
    mark_first_run(&settings.state_dir.join(FIRST_RUN_DONE));
    let Some(answer) = answer else {
        return 0;
    };
    let chosen = Path::new(&answer.tools_dir);
    // `VF_TOOLS_DIR` wins over the stored folder: nothing to move then.
    if chosen != tools
        && std::env::var_os("VF_TOOLS_DIR").is_none()
        && let Err(e) = vf_platform::storage::move_folder(
            &mut settings,
            vf_platform::storage::Kind::Tools,
            chosen,
        )
    {
        app::show(&Notice {
            title: "Vibefork".into(),
            body: catalog::first_run_folder_failed(lang, &e.to_string()),
        });
    }
    if answer.connect_ai {
        App::new().act(&Action::Open);
    }
    0
}

/// `status`: the agent's state, refreshed like the tray's, with the tray's actions as buttons.
fn status() -> i32 {
    let app = RefCell::new(App::new());
    let lang = app.borrow().lang();
    let actions: RefCell<Vec<Action>> = RefCell::new(Vec::new());
    let shown = shell().status(
        app::POLL,
        || {
            // The tray process notifies; this window does not.
            let (model, _) = app.borrow_mut().poll();
            let (page, acts) = page(lang, &model);
            *actions.borrow_mut() = acts;
            page
        },
        |id| {
            let action = id
                .parse::<usize>()
                .ok()
                .and_then(|i| actions.borrow().get(i).cloned());
            if let Some(a) = action {
                app.borrow().act(&a);
            }
        },
    );
    match shown {
        Ok(()) => 0,
        Err(_) => window::EXIT_NO_WINDOW,
    }
}

/// The status page for a tray model, and the actions behind its buttons (a button's id is its
/// index).
fn page(lang: vf_core::Language, model: &Model) -> (StatusPage, Vec<Action>) {
    let mut lines = Vec::new();
    let mut buttons: Vec<(String, Action)> = Vec::new();
    // Lines of state are the disabled items above the first separator; below it a disabled item
    // is an action not available now (e.g. no update to install).
    let mut head = true;
    for entry in &model.menu {
        match entry {
            Entry::Item {
                label,
                action: None,
            } => {
                if head {
                    lines.push(label.clone());
                }
            }
            Entry::Item {
                label,
                action: Some(a),
            } => buttons.push((label.clone(), a.clone())),
            Entry::Sub { label, items } => {
                for item in items {
                    if let Entry::Item {
                        label: name,
                        action: Some(a),
                    } = item
                    {
                        buttons.push((format!("{label}: {name}"), a.clone()));
                    }
                }
            }
            Entry::Separator => head = false,
        }
    }
    // Quitting the tray and opening this window again are not for this window.
    buttons.retain(|(_, a)| !matches!(a, Action::Quit | Action::ShowStatus));
    // The activity line, when there is one, is the headline; the agent's own line follows.
    let headline = if lines.len() > 1 {
        lines.remove(1)
    } else if lines.is_empty() {
        String::new()
    } else {
        lines.remove(0)
    };
    let tone = match model.look {
        Look::Stopped | Look::Waiting => Tone::Normal,
        Look::Working => Tone::Busy,
        Look::NeedsYou => Tone::Attention,
        Look::Error => Tone::Error,
    };
    let (labels, acts): (Vec<String>, Vec<Action>) = buttons.into_iter().unzip();
    let page = StatusPage {
        title: catalog::status_window_title(lang),
        headline,
        tone,
        lines,
        actions: labels
            .into_iter()
            .enumerate()
            .map(|(i, l)| (i.to_string(), l))
            .collect(),
        close: catalog::window_close(lang),
    };
    (page, acts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_page_from_the_tray_menu() {
        let model = Model {
            look: Look::Working,
            tooltip: String::new(),
            menu: vec![
                Entry::Item {
                    label: "Vibefork 1.0: running".into(),
                    action: None,
                },
                Entry::Item {
                    label: "“notes-app” — checking".into(),
                    action: None,
                },
                Entry::Sub {
                    label: "Recent projects".into(),
                    items: vec![Entry::Item {
                        label: "notes-app".into(),
                        action: Some(Action::OpenProject("notes-app".into())),
                    }],
                },
                Entry::Separator,
                Entry::Item {
                    label: "Open Vibefork".into(),
                    action: Some(Action::Open),
                },
                Entry::Item {
                    label: "Install update".into(),
                    action: None,
                },
                Entry::Item {
                    label: "Agent status…".into(),
                    action: Some(Action::ShowStatus),
                },
                Entry::Item {
                    label: "Quit tray".into(),
                    action: Some(Action::Quit),
                },
            ],
        };
        let (page, acts) = page(vf_core::Language::En, &model);
        assert_eq!(page.headline, "“notes-app” — checking");
        assert_eq!(page.lines, vec!["Vibefork 1.0: running".to_owned()]);
        assert_eq!(page.tone, Tone::Busy);
        assert_eq!(
            page.actions,
            vec![
                ("0".to_owned(), "Recent projects: notes-app".to_owned()),
                ("1".to_owned(), "Open Vibefork".to_owned()),
            ]
        );
        assert_eq!(
            acts,
            vec![Action::OpenProject("notes-app".into()), Action::Open]
        );
    }
}
