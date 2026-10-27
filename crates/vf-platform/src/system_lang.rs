//! System language detection for native UI and CLI.

use vf_core::Language;

/// Detects the system language for native UI and CLI (Windows UI language, LANG/LC_ALL/LC_MESSAGES, macOS AppleLanguages).
/// Returns the first language the agent knows (`Language::ALL`); English when none matches.
pub fn system_language() -> Language {
    // 1. Environment variable override (for tests or explicit user configuration).
    for var in ["VF_LANG", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            let val = val.trim();
            if !val.is_empty() {
                return Language::parse(val);
            }
        }
    }

    // 2. OS-specific detection.
    #[cfg(windows)]
    {
        #[allow(unsafe_code)]
        let lang_id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        if let Some(lang) = windows_primary_language(lang_id) {
            return lang;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLanguages"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            // `(` then the preferred languages, one `"xx-YY",` per line; the first two count.
            if let Some(lang) = text
                .lines()
                .take(3)
                .map(|l| Language::parse(l.trim().trim_start_matches('"')))
                .find(|l| *l != Language::En)
            {
                return lang;
            }
        }
    }

    Language::En
}

/// The language of a Windows LANGID by its primary language (lower 10 bits).
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_primary_language(lang_id: u16) -> Option<Language> {
    match lang_id & 0x03ff {
        0x19 => Some(Language::Ru),   // LANG_RUSSIAN
        0x16 => Some(Language::PtBr), // LANG_PORTUGUESE (Brazil and Portugal)
        0x0a => Some(Language::Es),   // LANG_SPANISH
        0x07 => Some(Language::De),   // LANG_GERMAN
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(unsafe_code)]
    fn env_override_detection() {
        unsafe {
            std::env::set_var("VF_LANG", "ru");
        }
        assert_eq!(system_language(), Language::Ru);

        unsafe {
            std::env::set_var("VF_LANG", "ru_RU.UTF-8");
        }
        assert_eq!(system_language(), Language::Ru);

        unsafe {
            std::env::set_var("VF_LANG", "en");
        }
        assert_eq!(system_language(), Language::En);

        unsafe {
            std::env::set_var("VF_LANG", "en_US");
        }
        assert_eq!(system_language(), Language::En);

        unsafe {
            std::env::set_var("VF_LANG", "de_DE.UTF-8");
        }
        assert_eq!(system_language(), Language::De);

        unsafe {
            std::env::set_var("VF_LANG", "pt_BR");
        }
        assert_eq!(system_language(), Language::PtBr);

        unsafe {
            std::env::remove_var("VF_LANG");
        }
    }

    #[test]
    fn windows_language_ids() {
        assert_eq!(windows_primary_language(0x0419), Some(Language::Ru));
        assert_eq!(windows_primary_language(0x0416), Some(Language::PtBr)); // pt-BR
        assert_eq!(windows_primary_language(0x0816), Some(Language::PtBr)); // pt-PT
        assert_eq!(windows_primary_language(0x0c0a), Some(Language::Es));
        assert_eq!(windows_primary_language(0x0407), Some(Language::De));
        assert_eq!(windows_primary_language(0x0409), None); // en-US
        assert_eq!(windows_primary_language(0x040c), None); // fr-FR
    }
}
