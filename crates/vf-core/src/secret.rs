//! [`Secret`]: a value that must never appear in logs (invariant 6).

use std::fmt;

/// Wraps a secret (API key, session token). `Debug` and `Display` print `[redacted]`.
/// Get the value only where it is really needed, with [`Secret::expose`].
#[derive(Clone, PartialEq, Eq)]
pub struct Secret<T>(T);

/// A secret as typed or pasted: without spaces, line breaks, byte order marks or other invisible
/// characters around it (keys and tokens are printable ASCII; a dialog's output may carry a BOM).
pub fn clean_secret(raw: &str) -> String {
    raw.trim_matches(|c: char| {
        c.is_whitespace() || c == '\u{feff}' || c == '\u{200b}' || c.is_control()
    })
    .to_owned()
}

impl<T> Secret<T> {
    /// Wraps a value.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Returns the secret value. Grep for `expose(` to review every use.
    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_prints_the_value() {
        let s = Secret::new("sk-or-very-secret".to_string());
        assert_eq!(format!("{s:?}"), "[redacted]");
        assert_eq!(format!("{s}"), "[redacted]");
        assert!(!format!("{:?}", Some(&s)).contains("very-secret"));
        assert_eq!(s.expose(), "sk-or-very-secret");
    }
}

#[cfg(test)]
mod clean_tests {
    use super::clean_secret;

    #[test]
    fn invisible_characters_around_a_key_are_dropped() {
        assert_eq!(clean_secret("\u{feff}sk-or-v1-abc\r\n"), "sk-or-v1-abc");
        assert_eq!(clean_secret("  \u{200b}sk-or-v1-abc "), "sk-or-v1-abc");
        assert_eq!(clean_secret("\u{feff}\r\n"), "");
    }
}
