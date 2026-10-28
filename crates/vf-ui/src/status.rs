//! The agent's status: a headline, lines of state and buttons for what can be done. The caller
//! refreshes the page; a button press goes back to it as the button's id.

use eframe::egui::{self, Key, RichText};

use crate::text::{MAX_LINE, plain_line};
use crate::theme;

/// What the status window shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusPage {
    /// Window title.
    pub title: String,
    /// The main line (e.g. "“notes-app” — checking, ≈ 1 min left").
    pub headline: String,
    /// The tone of the headline.
    pub tone: Tone,
    /// More lines of state.
    pub lines: Vec<String>,
    /// Buttons: `(id, label)`; a press returns the id.
    pub actions: Vec<(String, String)>,
    /// Label of the close button.
    pub close: String,
}

/// The tone of a status headline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    /// Nothing special.
    #[default]
    Normal,
    /// Something runs.
    Busy,
    /// The person is needed.
    Attention,
    /// Something failed.
    Error,
}

/// What happened in the status window this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusEvent {
    /// A button was pressed: its id.
    Action(String),
    /// The person closed the window (close button or Esc).
    Close,
}

/// The status window drawn into an egui [`egui::Ui`].
pub struct StatusView {
    page: StatusPage,
}

impl StatusView {
    /// A view of `page`, cleaned to plain text.
    pub fn new(page: StatusPage) -> Self {
        let mut view = Self {
            page: StatusPage::default(),
        };
        view.set_page(page);
        view
    }

    /// Replaces what is shown.
    pub fn set_page(&mut self, page: StatusPage) {
        let line = |s: &str| plain_line(s, MAX_LINE);
        self.page = StatusPage {
            title: line(&page.title),
            headline: line(&page.headline),
            tone: page.tone,
            lines: page.lines.iter().map(|l| line(l)).collect(),
            actions: page
                .actions
                .iter()
                .map(|(id, label)| (id.clone(), line(label)))
                .collect(),
            close: line(&page.close),
        };
    }

    /// The page as shown.
    pub fn page(&self) -> &StatusPage {
        &self.page
    }

    /// Draws the page; returns what the person did this frame.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<StatusEvent> {
        let p = &self.page;
        let colour = match p.tone {
            Tone::Normal => theme::TEXT,
            Tone::Busy => theme::ACCENT,
            Tone::Attention => theme::WARNING,
            Tone::Error => theme::DANGER,
        };
        ui.add_space(4.0);
        ui.label(
            RichText::new(p.headline.as_str())
                .heading()
                .strong()
                .color(colour),
        );
        for l in &p.lines {
            ui.label(RichText::new(l.as_str()).color(theme::MUTED));
        }
        ui.add_space(8.0);
        let mut event = None;
        ui.horizontal_wrapped(|ui| {
            for (id, label) in &p.actions {
                if ui.add(theme::secondary_button(label)).clicked() {
                    event = Some(StatusEvent::Action(id.clone()));
                }
            }
        });
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(theme::primary_button(&p.close)).clicked() {
                event = Some(StatusEvent::Close);
            }
        });
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            event = Some(StatusEvent::Close);
        }
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_plain() {
        let view = StatusView::new(StatusPage {
            headline: "“notes-app”\n— checking".into(),
            actions: vec![("open".into(), "Open\u{202E}".into())],
            ..Default::default()
        });
        assert_eq!(view.page().headline, "“notes-app” — checking");
        assert_eq!(view.page().actions, vec![("open".into(), "Open".into())]);
    }
}
