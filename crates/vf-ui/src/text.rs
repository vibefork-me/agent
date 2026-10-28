//! Plain text for windows. A window shows text that partly comes from outside (a web page's name,
//! a project's name), so it is cleaned before it is drawn: nothing may change how the rest of the
//! text reads.

/// Longest text of a window body, in characters.
pub const MAX_TEXT: usize = 2000;
/// Longest one-line value (title, button, field label), in characters.
pub const MAX_LINE: usize = 120;

/// Characters that change how text is laid out or hide text: bidi controls, zero-width characters,
/// the byte order mark.
fn is_layout_trick(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}'
    )
}

/// The body of a window: keeps line breaks, drops other control characters and layout tricks, and
/// caps the length at [`MAX_TEXT`] characters.
pub fn plain_text(text: &str) -> String {
    text.chars()
        .filter(|&c| c == '\n' || !(c.is_control() || is_layout_trick(c)))
        .take(MAX_TEXT)
        .collect()
}

/// One line (a title, a button, a label): line breaks, tabs and other control characters become
/// spaces, layout tricks are removed, runs of spaces collapse, and the result is cut to `max`
/// characters (ending with `…`).
pub fn plain_line(value: &str, max: usize) -> String {
    let mut out = String::new();
    for c in value.chars() {
        if is_layout_trick(c) {
            continue;
        }
        let c = if c.is_control() { ' ' } else { c };
        if c == ' ' && (out.is_empty() || out.ends_with(' ')) {
            continue;
        }
        out.push(c);
    }
    let out = out.trim_end();
    if out.chars().count() > max {
        let mut cut: String = out.chars().take(max.saturating_sub(1)).collect();
        cut.push('…');
        cut
    } else {
        out.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_keeps_line_breaks_only() {
        assert_eq!(
            plain_text("Site: x\nCode: 1\r\u{202E}\u{0007}"),
            "Site: x\nCode: 1"
        );
        assert_eq!(plain_text(&"x".repeat(5000)).chars().count(), MAX_TEXT);
    }

    #[test]
    fn a_line_cannot_be_forged() {
        assert_eq!(
            plain_line("Chrome\n\nCode: 000000\tok", 80),
            "Chrome Code: 000000 ok"
        );
        assert_eq!(plain_line("evil\u{202E}txt.exe", 80), "eviltxt.exe");
        assert_eq!(plain_line("a\u{200B}b\u{FEFF}", 80), "ab");
        assert_eq!(plain_line("abcdef", 4), "abc…");
        assert_eq!(plain_line("  spaced   out  ", 80), "spaced out");
    }
}
