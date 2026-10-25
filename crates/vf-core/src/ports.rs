//! Ports: traits that logic and adapters depend on; implementations live in adapter crates and are
//! connected in `vf-agent`.

use crate::Secret;

/// A native OS dialog — the one UI a web page cannot click. Used for pairing and dangerous actions.
pub trait ConfirmDialog: Send + Sync {
    /// UI language of native dialogs. Defaults to English.
    fn language(&self) -> crate::catalog::Language {
        crate::catalog::Language::En
    }

    /// Shows `text` with an Allow / Block choice. Blocks the calling thread; returns `false` on
    /// Block, on timeout and when no dialog can be shown.
    fn confirm(&self, title: &str, text: &str) -> bool;

    /// Prompts the user for a secret (e.g. an API key) with input masked.
    /// Returns `None` on cancel, empty input, timeout or when no dialog can be shown.
    fn ask_secret(&self, title: &str, text: &str) -> Option<Secret<String>> {
        let _ = (title, text);
        None
    }
}

/// Characters that change how text is laid out or hide text: bidi controls, zero-width characters,
/// the byte order mark. A page could use them to make a dialog read differently from what it says.
fn is_layout_trick(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}'
    )
}

/// A value from a web page (browser name, project, target) made safe for one line of a dialog:
/// line breaks, tabs and other control characters become spaces, layout tricks are removed, runs of
/// spaces collapse, and the result is cut to `max` characters (ending with `…`).
pub fn dialog_field(value: &str, max: usize) -> String {
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

/// The whole text of a dialog: keeps line breaks, drops other control characters and layout tricks,
/// and caps the length at 2000 characters.
pub fn dialog_text(text: &str) -> String {
    text.chars()
        .filter(|&c| c == '\n' || !(c.is_control() || is_layout_trick(c)))
        .take(2000)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_values_cannot_forge_dialog_lines() {
        assert_eq!(
            dialog_field("Chrome\n\nCode: 000000\tok", 80),
            "Chrome Code: 000000 ok"
        );
        assert_eq!(dialog_field("evil\u{202E}txt.exe", 80), "eviltxt.exe");
        assert_eq!(dialog_field("a\u{200B}b\u{FEFF}", 80), "ab");
        assert_eq!(dialog_field("abcdef", 4), "abc…");
        assert_eq!(dialog_field("  spaced   out  ", 80), "spaced out");
    }

    #[test]
    fn dialog_text_keeps_line_breaks_only() {
        assert_eq!(
            dialog_text("Site: x\nCode: 1\r\u{202E}\u{0007}"),
            "Site: x\nCode: 1"
        );
        assert_eq!(dialog_text(&"x".repeat(5000)).chars().count(), 2000);
    }
}
