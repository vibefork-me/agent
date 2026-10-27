//! Messages of the person to a running task: taken at the points that allow it (before planning,
//! before a step, before a step runs again), answered with `note.taken`, and put after the prompt of
//! every later call to a model. Text attachments go into the prompt (cut); images and other binary
//! data are saved to the task's temporary folder outside the project and the model gets their paths.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use vf_core::catalog::Language;
use vf_protocol::Cancel;
use vf_protocol::canvas::{CanvasEvent, NoteAttachment};
use vf_protocol::note::{AttachmentKind, MAX_TEXT_ATTACHMENT, Note, base64_payload, kind};
use vf_protocol::ws::ServerMessage;

use crate::step::{TmpGuard, new_run_id};

/// Longest part of a prompt made of the person's messages, characters; older messages are left out.
pub(crate) const MAX_NOTES_PROMPT: usize = 40_000;
/// Longest saved attachment file name (without the number in front), characters.
const MAX_FILE_NAME: usize = 60;

/// The messages one task took; the attachments' folder is removed when the task ends.
pub(crate) struct Inbox {
    dir: PathBuf,
    _guard: TmpGuard,
    lang: Language,
    taken: RefCell<Vec<String>>,
}

impl Inbox {
    /// An empty inbox of a task of `project`; attachments go to a new folder under `caches/tmp`.
    pub(crate) fn new(caches: &Path, project: &str, lang: &str) -> Self {
        let run_id = new_run_id(&format!("notes-{project}"));
        Self {
            dir: caches.join("tmp").join(&run_id),
            _guard: TmpGuard::new(caches, &run_id),
            lang: Language::parse(lang),
            taken: RefCell::new(Vec::new()),
        }
    }

    /// Takes the queued messages and answers each with `note.taken` (a canvas event raised by the
    /// work; the session stream numbers it).
    pub(crate) fn pick_up(&self, cancel: &Cancel, out: &mut dyn FnMut(ServerMessage)) {
        for note in cancel.take_notes() {
            let block = render(&note, &self.dir);
            self.taken.borrow_mut().push(block);
            out(ServerMessage::Canvas {
                project: String::new(),
                seq: 0,
                at: 0,
                session: String::new(),
                ev: CanvasEvent::NoteTaken {
                    note_id: note.id.to_string(),
                    related: Vec::new(),
                    reply: vf_core::catalog::note_taken(self.lang),
                },
            });
        }
    }

    /// The part of a prompt with every message taken so far (empty when none): the newest ones up
    /// to [`MAX_NOTES_PROMPT`] characters.
    pub(crate) fn addendum(&self) -> String {
        let taken = self.taken.borrow();
        if taken.is_empty() {
            return String::new();
        }
        let mut kept: Vec<&str> = Vec::new();
        let mut size = 0usize;
        for block in taken.iter().rev() {
            size = size.saturating_add(block.chars().count());
            if size > MAX_NOTES_PROMPT && !kept.is_empty() {
                break;
            }
            kept.push(block);
        }
        let left_out = taken.len().saturating_sub(kept.len());
        kept.reverse();
        let mut s = String::from(
            "\n\nMessages from the person while the task runs (take them into account; \
             a later message wins over an earlier one):\n",
        );
        if left_out > 0 {
            s.push_str(&format!("({left_out} earlier messages left out)\n"));
        }
        for block in kept {
            s.push('\n');
            s.push_str(block);
            s.push('\n');
        }
        s
    }
}

/// A message as prompt text; binary attachments are saved under `dir`.
fn render(note: &Note, dir: &Path) -> String {
    let mut s = format!("Message {}:\n{}", note.id, note.text.trim());
    for (k, a) in note.attachments.iter().enumerate() {
        s.push_str(&format!(
            "\nAttachment \"{}\" ({}): ",
            a.name.trim(),
            a.mime.trim()
        ));
        match kind(&a.mime) {
            AttachmentKind::Text => s.push_str(&clip_chars(&a.data, MAX_TEXT_ATTACHMENT)),
            other => match save(note.id, k, a, dir) {
                Ok(path) => {
                    s.push_str(&format!("saved at {}", path.display()));
                    if other == AttachmentKind::Image {
                        s.push_str(
                            " (an image, for example a screenshot; look at it if you can read \
                             images, otherwise go by the message)",
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(note = note.id, error = %e, "attachment not saved");
                    s.push_str("could not be read");
                }
            },
        }
    }
    s
}

/// The text up to `max` characters; a cut text says so.
fn clip_chars(text: &str, max: usize) -> String {
    let total = text.chars().count();
    if total <= max {
        return format!("\n{text}");
    }
    let kept: String = text.chars().take(max).collect();
    format!(
        "\n{kept}\n(cut: {} more characters)",
        total.saturating_sub(max)
    )
}

/// Decodes a binary attachment and writes it under `dir` with a name of the agent's own making.
fn save(note: u32, k: usize, a: &NoteAttachment, dir: &Path) -> Result<PathBuf, String> {
    let payload: String = base64_payload(&a.data)
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.as_bytes())
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(file_name(note, k, &a.name));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

/// `<note>-<k>-<name>` with only ASCII letters, digits, `.`, `_`, `-` of the name kept.
fn file_name(note: u32, k: usize, name: &str) -> String {
    let safe: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .take(MAX_FILE_NAME)
        .collect();
    let safe = safe.trim_start_matches('.');
    let safe = if safe.is_empty() { "attachment" } else { safe };
    format!("{note}-{}-{safe}", k.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_the_agents_own() {
        assert_eq!(file_name(2, 0, "screen shot.png"), "2-1-screenshot.png");
        assert_eq!(file_name(1, 1, "..\\..\\x"), "1-2-x");
        assert_eq!(file_name(1, 0, "снимок"), "1-1-attachment");
    }

    #[test]
    fn long_text_is_cut() {
        let s = clip_chars(&"я".repeat(10), 4);
        assert_eq!(s, "\nяяяя\n(cut: 6 more characters)");
        assert_eq!(clip_chars("ok", 4), "\nok");
    }
}
