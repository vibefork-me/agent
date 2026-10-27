//! Messages of the person to a running task (`note`): bounds and checks of what comes from the
//! network, applied before a message is queued. The engine takes queued messages at the next point
//! that allows it (before a step of a plan, before a step runs again, before planning) and answers
//! with `note.taken`.

use crate::canvas::NoteAttachment;

/// Longest message text, characters.
pub const MAX_NOTE_TEXT: usize = 8000;
/// Most attachments of one message.
pub const MAX_ATTACHMENTS: usize = 5;
/// Longest `data` of one attachment, bytes (base64 of about 5 MiB).
pub const MAX_ATTACHMENT_DATA: usize = 7 * 1024 * 1024;
/// Longest `data` of all attachments of one message, bytes.
pub const MAX_NOTE_DATA: usize = 16 * 1024 * 1024;
/// Longest attachment name, characters.
pub const MAX_ATTACHMENT_NAME: usize = 100;
/// Longest MIME type, characters.
pub const MAX_MIME: usize = 100;
/// Most messages waiting to be taken.
pub const MAX_PENDING_NOTES: usize = 20;
/// Most messages of one task.
pub const MAX_TASK_NOTES: u32 = 30;
/// Longest text attachment put into a prompt, characters; the rest is cut.
pub const MAX_TEXT_ATTACHMENT: usize = 2000;

/// A message of the person, queued for the running task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Number of the message within the task, from 1.
    pub id: u32,
    /// The message.
    pub text: String,
    /// Checked attachments (see [`check`]).
    pub attachments: Vec<NoteAttachment>,
}

/// Why a message was not queued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteRejected {
    /// Neither text nor attachments.
    Empty,
    /// The text is longer than [`MAX_NOTE_TEXT`].
    TextTooLong,
    /// More than [`MAX_ATTACHMENTS`] attachments.
    TooManyAttachments,
    /// An attachment is larger than [`MAX_ATTACHMENT_DATA`] or all are larger than [`MAX_NOTE_DATA`].
    TooLarge,
    /// An attachment name is empty, too long, or has a path or control character.
    BadName,
    /// A MIME type is not `type/subtype`.
    BadMime,
    /// Binary data is not base64.
    BadData,
    /// [`MAX_PENDING_NOTES`] messages are waiting already.
    QueueFull,
    /// The task had [`MAX_TASK_NOTES`] messages already.
    TooManyNotes,
    /// No task takes messages (it ended or is being cancelled).
    NotRunning,
}

impl std::fmt::Display for NoteRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("the message is empty"),
            Self::TextTooLong => write!(f, "the message is longer than {MAX_NOTE_TEXT} characters"),
            Self::TooManyAttachments => write!(f, "at most {MAX_ATTACHMENTS} attachments"),
            Self::TooLarge => write!(
                f,
                "an attachment is too large (at most {} MiB each, {} MiB in all)",
                MAX_ATTACHMENT_DATA / 1024 / 1024,
                MAX_NOTE_DATA / 1024 / 1024
            ),
            Self::BadName => f.write_str("an attachment has a bad name"),
            Self::BadMime => f.write_str("an attachment has a bad type"),
            Self::BadData => f.write_str("an attachment is not base64"),
            Self::QueueFull => f.write_str("too many messages are waiting; send it a bit later"),
            Self::TooManyNotes => write!(f, "at most {MAX_TASK_NOTES} messages per task"),
            Self::NotRunning => f.write_str("no task is running"),
        }
    }
}

/// What the engine does with an attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    /// Put into the prompt as text (cut to [`MAX_TEXT_ATTACHMENT`]).
    Text,
    /// An image: saved to a file the model is told about.
    Image,
    /// Other binary data: saved to a file the model is told about.
    File,
}

/// The type of an attachment by its MIME type (parameters such as `; charset=` are ignored).
pub fn kind(mime: &str) -> AttachmentKind {
    let essence = essence(mime);
    let text = essence.starts_with("text/")
        || matches!(
            essence.as_str(),
            "application/json"
                | "application/xml"
                | "application/yaml"
                | "application/x-yaml"
                | "application/toml"
                | "application/javascript"
                | "application/x-sh"
        );
    if text {
        AttachmentKind::Text
    } else if essence.starts_with("image/") {
        AttachmentKind::Image
    } else {
        AttachmentKind::File
    }
}

/// The base64 payload of binary `data`: as is, or after the comma of a `data:` URL.
pub fn base64_payload(data: &str) -> &str {
    match data.split_once(',') {
        Some((head, payload)) if head.starts_with("data:") => payload,
        _ => data,
    }
}

/// Checks a message against the bounds above; nothing is decoded.
pub fn check(text: &str, attachments: &[NoteAttachment]) -> Result<(), NoteRejected> {
    if text.trim().is_empty() && attachments.is_empty() {
        return Err(NoteRejected::Empty);
    }
    if text.chars().count() > MAX_NOTE_TEXT {
        return Err(NoteRejected::TextTooLong);
    }
    if attachments.len() > MAX_ATTACHMENTS {
        return Err(NoteRejected::TooManyAttachments);
    }
    let mut total = 0usize;
    for a in attachments {
        total = total.saturating_add(a.data.len());
        if a.data.len() > MAX_ATTACHMENT_DATA || total > MAX_NOTE_DATA {
            return Err(NoteRejected::TooLarge);
        }
        if !good_name(&a.name) {
            return Err(NoteRejected::BadName);
        }
        if !good_mime(&a.mime) {
            return Err(NoteRejected::BadMime);
        }
        if kind(&a.mime) != AttachmentKind::Text
            && !base64_payload(&a.data).bytes().all(|b| {
                b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'\r' | b'\n')
            })
        {
            return Err(NoteRejected::BadData);
        }
    }
    Ok(())
}

fn essence(mime: &str) -> String {
    mime.split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn good_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && name.chars().count() <= MAX_ATTACHMENT_NAME
        && !trimmed.starts_with('.')
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

fn good_mime(mime: &str) -> bool {
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(
                        b,
                        b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
                    )
            })
    };
    mime.chars().count() <= MAX_MIME
        && !mime.chars().any(char::is_control)
        && match essence(mime).split_once('/') {
            Some((t, s)) => token(t) && token(s),
            None => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn att(name: &str, mime: &str, data: &str) -> NoteAttachment {
        NoteAttachment {
            name: name.into(),
            mime: mime.into(),
            data: data.into(),
        }
    }

    #[test]
    fn a_plain_message_and_good_attachments_pass() {
        assert_eq!(check("make the tags blue", &[]), Ok(()));
        let ok = [
            att("notes.md", "text/markdown", "# Tags\nany text"),
            att("screen shot.png", "image/png", "iVBORw0KGgo="),
            att("pasted", "image/png", "data:image/png;base64,iVBORw0KGgo="),
            att("data.bin", "application/octet-stream", "AAEC\r\nAw=="),
        ];
        assert_eq!(check("", &ok), Ok(()));
    }

    #[test]
    fn hostile_messages_are_rejected() {
        assert_eq!(check("  ", &[]), Err(NoteRejected::Empty));
        let long = "x".repeat(MAX_NOTE_TEXT + 1);
        assert_eq!(check(&long, &[]), Err(NoteRejected::TextTooLong));
        let many = vec![att("a.txt", "text/plain", "x"); MAX_ATTACHMENTS + 1];
        assert_eq!(check("hi", &many), Err(NoteRejected::TooManyAttachments));
        let big = "A".repeat(MAX_ATTACHMENT_DATA + 1);
        assert_eq!(
            check("hi", &[att("a.png", "image/png", &big)]),
            Err(NoteRejected::TooLarge)
        );
        let part = "A".repeat(MAX_ATTACHMENT_DATA);
        let three = vec![att("a.png", "image/png", &part); 3];
        assert_eq!(check("hi", &three), Err(NoteRejected::TooLarge));
        for name in [
            "",
            " ",
            "../x.png",
            "a/b.png",
            "a\\b.png",
            "C:x",
            ".env",
            "a\nb",
            &"n".repeat(101),
        ] {
            assert_eq!(
                check("hi", &[att(name, "text/plain", "x")]),
                Err(NoteRejected::BadName),
                "{name:?}"
            );
        }
        for mime in [
            "",
            "png",
            "image/",
            "/png",
            "image/png\n",
            "image/p ng",
            "text/plain\u{0}",
        ] {
            assert_eq!(
                check("hi", &[att("a", mime, "x")]),
                Err(NoteRejected::BadMime),
                "{mime:?}"
            );
        }
        assert_eq!(
            check("hi", &[att("a.png", "image/png", "not base64!")]),
            Err(NoteRejected::BadData)
        );
    }

    #[test]
    fn kinds_by_mime() {
        assert_eq!(kind("text/plain; charset=utf-8"), AttachmentKind::Text);
        assert_eq!(kind("Application/JSON"), AttachmentKind::Text);
        assert_eq!(kind("image/webp"), AttachmentKind::Image);
        assert_eq!(kind("application/pdf"), AttachmentKind::File);
        assert_eq!(base64_payload("data:image/png;base64,QUJD"), "QUJD");
        assert_eq!(base64_payload("QUJD"), "QUJD");
    }
}
