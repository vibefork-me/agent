//! The first run: where the tools go, how much room they take, and a way to connect an AI model
//! (in the editor).

use eframe::egui::{self, Key, RichText};

use crate::text::{MAX_LINE, plain_line, plain_text};
use crate::theme;

/// Longest folder path typed, in characters.
const MAX_PATH: usize = 1024;

/// Strings of the first-run window, in the person's language.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FirstRunText {
    /// Window title and heading.
    pub title: String,
    /// What the agent is about to do; may hold line breaks.
    pub intro: String,
    /// Label of the tools folder field.
    pub folder: String,
    /// A note under the folder field (e.g. "Toolchains and build caches go here").
    pub folder_hint: String,
    /// "Connect an AI model" button: continues and opens the editor.
    pub connect_ai: String,
    /// Explains the button above (e.g. "Your subscription or a key, in the editor").
    pub connect_ai_hint: String,
    /// The main button: continue.
    pub start: String,
    /// Close without changes.
    pub later: String,
}

/// What the first-run window shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FirstRun {
    /// Its strings.
    pub text: FirstRunText,
    /// The tools folder proposed (editable).
    pub tools_dir: String,
    /// How much room the tools take, worded by the caller (e.g. "About 2.4 GB; 120 GB free").
    /// `None`: the line is not shown.
    pub space: Option<String>,
}

/// What the person chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstRunAnswer {
    /// The tools folder (trimmed, never empty).
    pub tools_dir: String,
    /// Whether the person wants to connect an AI model now.
    pub connect_ai: bool,
}

/// The first-run window drawn into an egui [`egui::Ui`]. [`FirstRunView::ui`] returns
/// `Some(Some(answer))` when the person continues and `Some(None)` when they close it for later.
pub struct FirstRunView {
    run: FirstRun,
    answer: Option<Option<FirstRunAnswer>>,
}

impl FirstRunView {
    /// A view of `run`; strings are cleaned to plain text.
    pub fn new(run: FirstRun) -> Self {
        let line = |s: &str| plain_line(s, MAX_LINE);
        let t = &run.text;
        let text = FirstRunText {
            title: line(&t.title),
            intro: plain_text(&t.intro),
            folder: line(&t.folder),
            folder_hint: plain_text(&t.folder_hint),
            connect_ai: line(&t.connect_ai),
            connect_ai_hint: plain_text(&t.connect_ai_hint),
            start: line(&t.start),
            later: line(&t.later),
        };
        Self {
            run: FirstRun {
                text,
                tools_dir: plain_line(&run.tools_dir, MAX_PATH),
                space: run.space.as_deref().map(line),
            },
            answer: None,
        }
    }

    /// Updates the room line (e.g. once an estimate is ready).
    pub fn set_space(&mut self, space: Option<&str>) {
        self.run.space = space.map(|s| plain_line(s, MAX_LINE));
    }

    /// Draws the window; returns the choice in the frame it is made.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Option<FirstRunAnswer>> {
        if let Some(a) = &self.answer {
            return Some(a.clone());
        }
        let t = &self.run.text;
        ui.add_space(4.0);
        ui.label(RichText::new(t.title.as_str()).heading().strong());
        ui.label(t.intro.as_str());
        ui.add_space(8.0);

        ui.label(RichText::new(t.folder.as_str()).strong());
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.run.tools_dir)
                .char_limit(MAX_PATH)
                .desired_width(f32::INFINITY),
        );
        if !t.folder_hint.is_empty() {
            ui.label(
                RichText::new(t.folder_hint.as_str())
                    .small()
                    .color(theme::MUTED),
            );
        }
        // The room the tools take (a slot kept even before an estimate exists).
        if let Some(space) = &self.run.space {
            ui.label(RichText::new(space.as_str()).color(theme::TEXT));
        }
        ui.add_space(8.0);

        let connect = ui.add(theme::secondary_button(&t.connect_ai));
        if !t.connect_ai_hint.is_empty() {
            ui.label(
                RichText::new(t.connect_ai_hint.as_str())
                    .small()
                    .color(theme::MUTED),
            );
        }
        ui.add_space(8.0);

        let folder = self.run.tools_dir.trim().to_owned();
        let (start, later) = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let later = ui.add(theme::secondary_button(&t.later));
                let start = ui.add_enabled(!folder.is_empty(), theme::primary_button(&t.start));
                (start, later)
            })
            .inner;

        let submit = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        let choose = |connect_ai| {
            Some(Some(FirstRunAnswer {
                tools_dir: folder.clone(),
                connect_ai,
            }))
        };
        let answer = if ui.input(|i| i.key_pressed(Key::Escape)) || later.clicked() {
            Some(None)
        } else if folder.is_empty() {
            None
        } else if connect.clicked() {
            choose(true)
        } else if start.clicked() || submit {
            choose(false)
        } else {
            None
        };
        if answer.is_some() {
            self.answer = answer;
        }
        self.answer.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_and_folder_are_plain() {
        let view = FirstRunView::new(FirstRun {
            text: FirstRunText {
                title: "Welcome\n\u{202E}".into(),
                ..Default::default()
            },
            tools_dir: "D:\\Tools\u{200B}\n".into(),
            space: Some("About 2 GB\u{0007}".into()),
        });
        assert_eq!(view.run.text.title, "Welcome");
        assert_eq!(view.run.tools_dir, "D:\\Tools");
        assert_eq!(view.run.space.as_deref(), Some("About 2 GB"));
    }

    #[test]
    fn escape_is_later() {
        let ctx = egui::Context::default();
        let mut view = FirstRunView::new(FirstRun {
            tools_dir: "/home/user/.local/share/vibefork/tools".into(),
            ..Default::default()
        });
        let mut answer = None;
        for events in [
            vec![],
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| answer = view.ui(ui));
            full.textures_delta.clear();
        }
        assert_eq!(answer, Some(None));
    }
}
