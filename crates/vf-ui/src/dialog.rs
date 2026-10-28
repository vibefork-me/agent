//! A question that only the person at the machine can answer: Allow / Block, a text value, or a
//! secret typed with masked input.
//!
//! Keyboard: Tab moves between the field and the buttons, Enter presses the focused button (in a
//! field: submits), Esc rejects. A confirm prompt starts with the focus on the reject button, so an
//! Enter typed elsewhere never allows. For the first [`INPUT_GUARD`] after the window appears it
//! takes no input at all: keys and clicks meant for another window are not answers. When the time
//! is up the window answers [`Answer::TimedOut`].

use std::time::Duration;

use eframe::egui::{self, Key, RichText};

use crate::text::{MAX_LINE, plain_line, plain_text};
use crate::theme;

/// How long a fresh window ignores keys and clicks.
pub const INPUT_GUARD: Duration = Duration::from_millis(600);
/// Longest value typed into a field, in characters.
pub const MAX_INPUT: usize = 4096;

/// What is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    /// Accept (allow) or reject (block).
    Confirm,
    /// A text value, shown as typed; an empty value cannot be submitted.
    Text,
    /// A secret, masked; with `allow_empty` an empty value can be submitted (e.g. a local server
    /// that needs no key).
    Secret {
        /// Whether an empty value is an answer.
        allow_empty: bool,
    },
}

/// A question, with every string in the person's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    /// What is asked.
    pub kind: DialogKind,
    /// Window title and heading.
    pub title: String,
    /// The question; may hold line breaks.
    pub text: String,
    /// Label of the accepting button (Allow, OK).
    pub accept: String,
    /// Label of the rejecting button (Block, Cancel).
    pub reject: String,
    /// How long the window waits for the person.
    pub timeout: Duration,
}

/// The person's answer.
#[derive(Clone, PartialEq, Eq)]
pub enum Answer {
    /// Confirm: accepted.
    Accepted,
    /// Text or secret: the value submitted (trimmed; for a secret, possibly empty only with
    /// `allow_empty`).
    Entered(String),
    /// Rejected: the reject button, Esc, or the window closed.
    Rejected,
    /// Nobody answered in time.
    TimedOut,
}

impl std::fmt::Debug for Answer {
    /// Never prints an entered value: it may be a secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accepted => f.write_str("Accepted"),
            Self::Entered(_) => f.write_str("Entered(…)"),
            Self::Rejected => f.write_str("Rejected"),
            Self::TimedOut => f.write_str("TimedOut"),
        }
    }
}

/// A [`Dialog`] drawn into an egui [`egui::Ui`]: call [`DialogView::ui`] every frame until it
/// returns an answer.
pub struct DialogView {
    dialog: Dialog,
    input: String,
    /// `ui.input().time` of the first frame.
    opened_at: Option<f64>,
    answer: Option<Answer>,
    /// Whether the first focus was placed.
    focus_placed: bool,
}

impl DialogView {
    /// A view of `dialog`; title, text and labels are cleaned to plain text.
    pub fn new(dialog: Dialog) -> Self {
        let dialog = Dialog {
            title: plain_line(&dialog.title, MAX_LINE),
            text: plain_text(&dialog.text),
            accept: plain_line(&dialog.accept, 40),
            reject: plain_line(&dialog.reject, 40),
            ..dialog
        };
        Self {
            dialog,
            input: String::new(),
            opened_at: None,
            answer: None,
            focus_placed: false,
        }
    }

    /// The question as shown (cleaned).
    pub fn dialog(&self) -> &Dialog {
        &self.dialog
    }

    /// The answer, once given.
    pub fn answer(&self) -> Option<&Answer> {
        self.answer.as_ref()
    }

    /// Takes the answer out, once given.
    pub fn take_answer(&mut self) -> Option<Answer> {
        self.answer.take()
    }

    /// Draws the question; returns the answer in the frame it is given (and on every later frame).
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Answer> {
        if let Some(a) = &self.answer {
            return Some(a.clone());
        }
        let now = ui.input(|i| i.time);
        let opened = *self.opened_at.get_or_insert(now);
        let elapsed = Duration::from_secs_f64((now - opened).max(0.0));
        if elapsed >= self.dialog.timeout {
            self.answer = Some(Answer::TimedOut);
            return self.answer.clone();
        }
        let ready = elapsed >= INPUT_GUARD;
        // Keep the countdown moving and wake up when the guard ends.
        ui.ctx().request_repaint_after(Duration::from_millis(200));

        ui.add_space(4.0);
        ui.label(RichText::new(self.dialog.title.as_str()).heading().strong());
        ui.add_space(4.0);
        ui.label(self.dialog.text.as_str());
        ui.add_space(6.0);

        let mut submit = false;
        let field = match self.dialog.kind {
            DialogKind::Confirm => None,
            DialogKind::Text | DialogKind::Secret { .. } => {
                let masked = matches!(self.dialog.kind, DialogKind::Secret { .. });
                let edit = egui::TextEdit::singleline(&mut self.input)
                    .password(masked)
                    .char_limit(MAX_INPUT)
                    .desired_width(f32::INFINITY)
                    .interactive(ready);
                let r = ui.add(edit);
                if ready && r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    if self.can_submit() {
                        submit = true;
                    } else {
                        // Enter on an empty field: keep typing there.
                        r.request_focus();
                    }
                }
                Some(r)
            }
        };

        // Time left, as a thin bar.
        let left = 1.0 - elapsed.as_secs_f32() / self.dialog.timeout.as_secs_f32().max(1.0);
        ui.add(
            egui::ProgressBar::new(left.clamp(0.0, 1.0))
                .desired_height(3.0)
                .fill(theme::BORDER),
        );
        ui.add_space(4.0);

        let can_submit = self.can_submit();
        let (accept, reject) = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Right to left: the reject button is the rightmost.
                let reject = ui.add_enabled(ready, theme::secondary_button(&self.dialog.reject));
                let accept = ui.add_enabled(
                    ready && can_submit,
                    theme::primary_button(&self.dialog.accept),
                );
                (accept, reject)
            })
            .inner;

        // First focus: the field, or the reject button of a confirm prompt.
        if ready && !self.focus_placed {
            self.focus_placed = true;
            match &field {
                Some(f) => f.request_focus(),
                None => reject.request_focus(),
            }
        }

        let escape = ui.input(|i| i.key_pressed(Key::Escape));
        let answer = if !ready {
            None
        } else if escape || reject.clicked() {
            Some(Answer::Rejected)
        } else if accept.clicked() || (submit && can_submit) {
            Some(self.accepted())
        } else {
            None
        };
        if answer.is_some() {
            self.answer = answer;
        }
        self.answer.clone()
    }

    fn can_submit(&self) -> bool {
        match self.dialog.kind {
            DialogKind::Confirm | DialogKind::Secret { allow_empty: true } => true,
            DialogKind::Text | DialogKind::Secret { allow_empty: false } => {
                !self.input.trim().is_empty()
            }
        }
    }

    fn accepted(&self) -> Answer {
        match self.dialog.kind {
            DialogKind::Confirm => Answer::Accepted,
            DialogKind::Text | DialogKind::Secret { .. } => {
                Answer::Entered(self.input.trim().to_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn confirm() -> Dialog {
        Dialog {
            kind: DialogKind::Confirm,
            title: "Give access to the project?".into(),
            text: "Site: example.test\nProject: notes-app".into(),
            accept: "Allow".into(),
            reject: "Block".into(),
            timeout: Duration::from_secs(60),
        }
    }

    /// Runs one frame at `time` seconds with `events`.
    fn frame(
        ctx: &egui::Context,
        view: &mut DialogView,
        time: f64,
        events: Vec<egui::Event>,
    ) -> Option<Answer> {
        let input = egui::RawInput {
            time: Some(time),
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(480.0, 320.0),
            )),
            ..Default::default()
        };
        let mut out = None;
        let mut full = ctx.run_ui(input, |ui| out = view.ui(ui));
        full.textures_delta.clear();
        out
    }

    fn key(k: Key) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn keys_in_the_first_moment_are_not_answers() {
        let ctx = egui::Context::default();
        let mut view = DialogView::new(confirm());
        assert_eq!(frame(&ctx, &mut view, 0.0, vec![]), None);
        assert_eq!(frame(&ctx, &mut view, 0.1, vec![key(Key::Escape)]), None);
        assert_eq!(frame(&ctx, &mut view, 0.2, vec![key(Key::Enter)]), None);
    }

    #[test]
    fn escape_rejects() {
        let ctx = egui::Context::default();
        let mut view = DialogView::new(confirm());
        frame(&ctx, &mut view, 0.0, vec![]);
        frame(&ctx, &mut view, 1.0, vec![]);
        assert_eq!(
            frame(&ctx, &mut view, 1.1, vec![key(Key::Escape)]),
            Some(Answer::Rejected)
        );
    }

    #[test]
    fn enter_on_a_confirm_prompt_rejects() {
        let ctx = egui::Context::default();
        let mut view = DialogView::new(confirm());
        frame(&ctx, &mut view, 0.0, vec![]);
        // The focus lands on the reject button once the guard is over.
        frame(&ctx, &mut view, 1.0, vec![]);
        frame(&ctx, &mut view, 1.1, vec![]);
        let mut answer = None;
        for t in [1.2, 1.3, 1.4] {
            answer = answer.or(frame(&ctx, &mut view, t, vec![key(Key::Enter)]));
        }
        assert_eq!(answer, Some(Answer::Rejected));
    }

    #[test]
    fn time_out() {
        let ctx = egui::Context::default();
        let mut view = DialogView::new(Dialog {
            timeout: Duration::from_secs(5),
            ..confirm()
        });
        assert_eq!(frame(&ctx, &mut view, 10.0, vec![]), None);
        assert_eq!(frame(&ctx, &mut view, 14.0, vec![]), None);
        assert_eq!(frame(&ctx, &mut view, 15.0, vec![]), Some(Answer::TimedOut));
    }

    #[test]
    fn typed_text_is_submitted_with_enter() {
        let ctx = egui::Context::default();
        let mut view = DialogView::new(Dialog {
            kind: DialogKind::Secret { allow_empty: false },
            ..confirm()
        });
        frame(&ctx, &mut view, 0.0, vec![]);
        frame(&ctx, &mut view, 1.0, vec![]);
        // Empty: Enter does nothing.
        assert_eq!(frame(&ctx, &mut view, 1.1, vec![key(Key::Enter)]), None);
        frame(&ctx, &mut view, 1.2, vec![]);
        frame(
            &ctx,
            &mut view,
            1.3,
            vec![egui::Event::Text(" sk-test-123 ".into())],
        );
        let mut answer = None;
        for t in [1.4, 1.5] {
            answer = answer.or(frame(&ctx, &mut view, t, vec![key(Key::Enter)]));
        }
        assert_eq!(answer, Some(Answer::Entered("sk-test-123".into())));
    }

    #[test]
    fn an_empty_secret_is_an_answer_only_when_allowed() {
        let mut view = DialogView::new(Dialog {
            kind: DialogKind::Secret { allow_empty: true },
            ..confirm()
        });
        assert!(view.can_submit());
        assert_eq!(view.accepted(), Answer::Entered(String::new()));
        view.dialog.kind = DialogKind::Text;
        assert!(!view.can_submit());
    }

    #[test]
    fn shown_text_is_plain() {
        let view = DialogView::new(Dialog {
            title: "Access\n\u{202E}granted".into(),
            text: "line one\nline two\u{0007}\u{200B}".into(),
            ..confirm()
        });
        assert_eq!(view.dialog().title, "Access granted");
        assert_eq!(view.dialog().text, "line one\nline two");
    }

    #[test]
    fn an_entered_value_is_not_printed() {
        assert_eq!(
            format!("{:?}", Answer::Entered("sk-test-123".into())),
            "Entered(…)"
        );
    }
}
