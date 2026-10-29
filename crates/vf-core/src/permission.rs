//! A request for permission to act on the person's computer: what will be done, why the person's
//! task needs it, and the exact commands it runs, each with a link to check the command at an
//! independent source (a web search with an AI answer).
//!
//! The link carries the command only: local paths are cut to their last name, and addresses on this
//! machine, URLs, e-mail addresses and anything that looks like a secret are left out. The search is
//! opened in the person's browser; no server of Vibefork takes part.

use crate::catalog::{self, Language};
use crate::{dialog_field, dialog_text};

/// Longest command shown, characters.
pub const MAX_COMMAND: usize = 300;
/// Most commands one request shows.
pub const MAX_COMMANDS: usize = 12;
/// Longest "what" and "why", characters each.
pub const MAX_LINE: usize = 400;
/// Words of a command that go into the search, at most.
const MAX_QUERY_WORDS: usize = 8;
/// The search every link opens; [`verify_url`] builds nothing else.
pub const VERIFY_BASE: &str = "https://www.google.com/search?";

/// What the person is asked to allow.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Permission {
    /// What will be done, in the person's language.
    pub what: String,
    /// Why the person's task needs it, in the person's language.
    pub why: String,
    /// The exact commands run on the computer; empty when the action runs none (then there is
    /// nothing to check and no link is shown).
    pub commands: Vec<String>,
}

impl Permission {
    /// A request without commands.
    pub fn new(what: impl Into<String>, why: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            why: why.into(),
            commands: Vec::new(),
        }
    }

    /// The same request with these commands.
    pub fn with_commands(mut self, commands: impl IntoIterator<Item = String>) -> Self {
        self.commands = commands.into_iter().collect();
        self
    }

    /// The request as shown: every value on one line without control characters or layout tricks,
    /// within [`MAX_LINE`] / [`MAX_COMMAND`], at most [`MAX_COMMANDS`] commands, no empty ones.
    pub fn cleaned(&self) -> Self {
        Self {
            what: dialog_field(&self.what, MAX_LINE),
            why: dialog_field(&self.why, MAX_LINE),
            commands: self
                .commands
                .iter()
                .map(|c| dialog_field(c, MAX_COMMAND))
                .filter(|c| !c.is_empty())
                .take(MAX_COMMANDS)
                .collect(),
        }
    }

    /// The request as plain text, for dialogs that show text only: who asks, what, why, and each
    /// command with its link.
    pub fn plain_text(&self, lang: Language, origin: Option<&str>) -> String {
        let p = self.cleaned();
        let mut text = catalog::permission_intro(lang, origin, !p.commands.is_empty());
        text.push_str("\n\n");
        text.push_str(&format!("{}: {}\n", catalog::permission_what(lang), p.what));
        text.push_str(&format!("{}: {}", catalog::permission_why(lang), p.why));
        if !p.commands.is_empty() {
            text.push_str(&format!("\n\n{}:", catalog::permission_commands(lang)));
            for c in &p.commands {
                text.push_str(&format!("\n{c}"));
                if let Some(url) = verify_url(c, lang) {
                    text.push_str(&format!("\n  {}: {url}", catalog::permission_verify(lang)));
                }
            }
        }
        dialog_text(&text)
    }
}

/// The link that checks `command` at an independent source: a web search "what does … do" in the
/// person's language. `None` when nothing of the command may leave the computer.
pub fn verify_url(command: &str, lang: Language) -> Option<String> {
    let query = verify_query(command)?;
    let hl = match lang {
        Language::EsEs => "es",
        l => l.as_str(),
    };
    Some(format!(
        "{VERIFY_BASE}hl={hl}&q={}",
        encode(&format!("what does {query} do"))
    ))
}

/// The words of `command` that may go into a search, or `None` when none may.
pub fn verify_query(command: &str) -> Option<String> {
    let mut words = Vec::new();
    let mut skip_next = false;
    for raw in command.split_whitespace() {
        let word = raw.trim_matches(|c| c == '"' || c == '\'');
        if skip_next {
            // `Authorization: Bearer X`: the value of a secret flag may be one more flag.
            skip_next = is_secret_flag(word);
            continue;
        }
        if is_secret_flag(word) {
            skip_next = true;
            continue;
        }
        if let Some(w) = shareable(word) {
            words.push(w);
        }
        if words.len() == MAX_QUERY_WORDS {
            break;
        }
    }
    (!words.is_empty()).then(|| words.join(" "))
}

/// A flag whose value is a secret: `--token X`, `-H "Authorization: …"`, `Bearer X`.
fn is_secret_flag(word: &str) -> bool {
    let w = word.to_ascii_lowercase();
    let name = w.trim_start_matches('-');
    w == "bearer"
        || w == "authorization:"
        || (w.starts_with('-') && !w.contains('=') && is_secret_name(name))
}

/// A name that holds a secret: `token`, `api-key`, `password`, …
fn is_secret_name(name: &str) -> bool {
    [
        "token",
        "secret",
        "password",
        "passwd",
        "pass",
        "key",
        "auth",
        "credential",
    ]
    .iter()
    .any(|s| name.contains(s))
}

/// The part of one word that may leave the computer: a path cut to its last name; nothing of a URL,
/// a local address, an e-mail address, a `name=value` with a secret name or a secret-looking value.
fn shareable(word: &str) -> Option<String> {
    let lower = word.to_ascii_lowercase();
    if word.is_empty()
        || lower.contains("://")
        || ["localhost", "127.0.0.1", "0.0.0.0", "[::1]", "::1"]
            .iter()
            .any(|h| lower.contains(h))
        || looks_like_address(word)
        || looks_like_email(word)
        || looks_like_secret(word)
    {
        return None;
    }
    if let Some((name, _)) = word.split_once('=')
        && is_secret_name(&name.trim_start_matches('-').to_ascii_lowercase())
    {
        return None;
    }
    let last = word.rsplit(['/', '\\']).next().unwrap_or_default();
    if last.is_empty() || last.ends_with(':') || last.starts_with('~') {
        return None;
    }
    if looks_like_secret(last) {
        return None;
    }
    Some(last.to_owned())
}

/// An IP address, with or without a port: only digits, dots and colons, with a dot.
fn looks_like_address(word: &str) -> bool {
    word.contains('.')
        && word
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ':')
}

/// `name@host.tld` (a tool spec such as `python@3.12` or `java@temurin-21` is not one).
fn looks_like_email(word: &str) -> bool {
    let Some((user, host)) = word.split_once('@') else {
        return false;
    };
    let tld = host.rsplit('.').next().unwrap_or_default();
    !user.is_empty()
        && host.contains('.')
        && tld.len() >= 2
        && tld.chars().all(|c| c.is_ascii_alphabetic())
}

/// A token: a known key prefix, or a long run of letters and digits mixed.
fn looks_like_secret(word: &str) -> bool {
    let w = word.trim_start_matches('-');
    if ["sk-", "ghp_", "gho_", "ghs_", "github_pat_", "xox", "aiza"]
        .iter()
        .any(|p| w.to_ascii_lowercase().starts_with(p))
        && w.len() > 8
    {
        return true;
    }
    w.chars().count() >= 24
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && w.chars().any(|c| c.is_ascii_digit())
        && w.chars().any(|c| c.is_ascii_alphabetic())
}

/// `application/x-www-form-urlencoded`: unreserved bytes as they are, space as `+`, the rest `%XX`.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b));
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_command_is_searched_as_it_is() {
        assert_eq!(
            verify_query("npm run build").as_deref(),
            Some("npm run build")
        );
        assert_eq!(
            verify_url("cargo build --release", Language::En).as_deref(),
            Some("https://www.google.com/search?hl=en&q=what+does+cargo+build+--release+do")
        );
    }

    #[test]
    fn the_link_is_in_the_persons_language() {
        for (lang, hl) in [
            (Language::Ru, "hl=ru&"),
            (Language::PtBr, "hl=pt-BR&"),
            (Language::EsEs, "hl=es&"),
            (Language::Es419, "hl=es-419&"),
            (Language::De, "hl=de&"),
        ] {
            let url = verify_url("make", lang).unwrap();
            assert!(url.starts_with(VERIFY_BASE), "{url}");
            assert!(url.contains(hl), "{url}");
        }
    }

    #[test]
    fn local_paths_are_cut_to_the_last_name() {
        assert_eq!(
            verify_query("/home/alex/projects/notes-app/target/debug/notes-app --verbose")
                .as_deref(),
            Some("notes-app --verbose")
        );
        assert_eq!(
            verify_query(r"C:\Users\alex\notes-app\gradlew.bat build").as_deref(),
            Some("gradlew.bat build")
        );
        assert_eq!(
            verify_query("./gradlew assemble").as_deref(),
            Some("gradlew assemble")
        );
        assert_eq!(verify_query("~/bin/tool").as_deref(), Some("tool"));
    }

    #[test]
    fn secrets_addresses_and_urls_stay_on_the_computer() {
        assert_eq!(
            verify_query("tool login --token abcdef123456 --name demo").as_deref(),
            Some("tool login --name demo")
        );
        assert_eq!(
            verify_query("tool --api-key=abc123 run").as_deref(),
            Some("tool run")
        );
        assert_eq!(
            verify_query("curl -H Authorization: Bearer abc123 https://example.org/x").as_deref(),
            Some("curl -H")
        );
        assert_eq!(
            verify_query("serve --host 127.0.0.1:8080 localhost:3000 10.0.0.7").as_deref(),
            Some("serve --host")
        );
        assert_eq!(
            verify_query("git config user.email someone@example.org").as_deref(),
            Some("git config user.email")
        );
        assert_eq!(
            verify_query("tool sk-abcdefghijklmnop Zx9fQ2mL7pR4tW8yB1nC5vK3").as_deref(),
            Some("tool")
        );
    }

    #[test]
    fn tool_specs_are_kept() {
        assert_eq!(
            verify_query("mise install java@temurin-21").as_deref(),
            Some("mise install java@temurin-21")
        );
        assert_eq!(
            verify_query("mise install python@3.12").as_deref(),
            Some("mise install python@3.12")
        );
    }

    #[test]
    fn nothing_shareable_means_no_link() {
        assert_eq!(verify_query(""), None);
        assert_eq!(verify_query("https://example.org/install.sh"), None);
        assert_eq!(verify_url("/home/alex/", Language::En), None);
    }

    #[test]
    fn long_commands_are_cut_to_a_few_words() {
        let q = verify_query("a b c d e f g h i j k").unwrap();
        assert_eq!(q.split(' ').count(), MAX_QUERY_WORDS);
    }

    #[test]
    fn the_query_is_encoded() {
        let url = verify_url("echo a&b=c#d", Language::En).unwrap();
        assert!(url.ends_with("q=what+does+echo+a%26b%3Dc%23d+do"), "{url}");
    }

    #[test]
    fn a_request_is_cleaned() {
        let p = Permission::new("Build\u{202e} the\nproject", "x".repeat(1000))
            .with_commands(["  ".to_owned(), "npm\trun build".to_owned()])
            .cleaned();
        assert_eq!(p.what, "Build the project");
        assert_eq!(p.why.chars().count(), MAX_LINE);
        assert_eq!(p.commands, ["npm run build"]);
        let many = Permission::new("w", "y")
            .with_commands((0..50).map(|i| format!("tool {i}")))
            .cleaned();
        assert_eq!(many.commands.len(), MAX_COMMANDS);
    }

    #[test]
    fn plain_text_shows_what_why_and_links() {
        let text = Permission::new("Build the project notes-app", "To check the changes.")
            .with_commands(["npm run build".to_owned()])
            .plain_text(Language::En, Some("http://localhost:5173"));
        assert!(text.contains("http://localhost:5173"), "{text}");
        assert!(text.contains("What: Build the project notes-app"), "{text}");
        assert!(text.contains("Why: To check the changes."), "{text}");
        assert!(text.contains("\nnpm run build\n"), "{text}");
        assert!(
            text.contains("https://www.google.com/search?hl=en&q=what+does+npm+run+build+do"),
            "{text}"
        );
    }

    #[test]
    fn without_commands_there_is_no_link_and_no_promise_to_check() {
        let text = Permission::new("Put the part back as it was", "You chose it.")
            .plain_text(Language::En, None);
        assert!(!text.contains("http"), "{text}");
        assert!(
            !text.contains(&catalog::permission_verify(Language::En)),
            "{text}"
        );
    }
}
