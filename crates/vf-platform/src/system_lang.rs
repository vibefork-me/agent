//! System language detection for native UI and CLI.

use vf_core::Language;

/// Detects the system language for native UI and CLI (Windows UI language, LANG/LC_ALL/LC_MESSAGES, macOS AppleLanguages).
/// Returns `Language::Ru` or `Language::En` (fallback).
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
        // Primary language ID is lower 10 bits: 0x19 is LANG_RUSSIAN.
        if (lang_id & 0x03ff) == 0x19 {
            return Language::Ru;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLanguages"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.lines().take(3).any(|l| l.trim().to_ascii_lowercase().starts_with("\"ru")) {
                return Language::Ru;
            }
        }
    }

    Language::En
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
            std::env::remove_var("VF_LANG");
        }
    }
}
