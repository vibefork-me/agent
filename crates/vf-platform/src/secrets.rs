//! Secrets in the OS store (Windows Credential Manager, macOS Keychain, Linux Secret Service),
//! service `vibefork`. Values are returned as [`Secret`] and never logged (invariant 6).

#[cfg(feature = "test-store")]
use std::collections::HashMap;
#[cfg(feature = "test-store")]
use std::sync::{LazyLock, Mutex};

use vf_core::Secret;

const SERVICE: &str = "vibefork";

#[cfg(feature = "test-store")]
static TEST_STORE: LazyLock<Mutex<Option<HashMap<String, String>>>> =
    LazyLock::new(|| Mutex::new(None));

/// Enables an in-memory secret store for automated tests where no OS secret service exists (e.g. headless CI).
/// Only with the `test-store` feature, which release builds do not enable (checked in CI).
#[cfg(feature = "test-store")]
pub fn enable_test_store() {
    #[cfg(feature = "test-store")]
    if let Ok(mut lock) = TEST_STORE.lock() {
        *lock = Some(HashMap::new());
    }
}

/// Names of the secrets the agent keeps.
pub const NAMES: &[&str] = &["openrouter", "github", "custom_openai", "gemini"];

/// Errors of the secret store.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// Not one of [`NAMES`].
    #[error("unknown secret `{0}`")]
    Unknown(String),
    /// The OS store failed.
    #[error("secret store: {0}")]
    Store(String),
}

fn entry(name: &str) -> Result<keyring::Entry, SecretError> {
    if !NAMES.contains(&name) {
        return Err(SecretError::Unknown(name.to_owned()));
    }
    keyring::Entry::new(SERVICE, name).map_err(|e| SecretError::Store(e.to_string()))
}

/// Reads a secret; `None` when it is not stored.
pub fn get(name: &str) -> Option<Secret<String>> {
    #[cfg(feature = "test-store")]
    if let Ok(lock) = TEST_STORE.lock()
        && let Some(map) = lock.as_ref()
    {
        return map.get(name).cloned().map(Secret::new);
    }
    entry(name).ok()?.get_password().ok().map(Secret::new)
}

/// Stores a secret.
pub fn set(name: &str, value: &str) -> Result<(), SecretError> {
    if !NAMES.contains(&name) {
        return Err(SecretError::Unknown(name.to_owned()));
    }
    #[cfg(feature = "test-store")]
    if let Ok(mut lock) = TEST_STORE.lock()
        && let Some(map) = lock.as_mut()
    {
        map.insert(name.to_owned(), value.to_owned());
        return Ok(());
    }
    entry(name)?
        .set_password(value)
        .map_err(|e| SecretError::Store(e.to_string()))
}

/// Removes a secret (no error when it is not stored).
pub fn remove(name: &str) -> Result<(), SecretError> {
    #[cfg(feature = "test-store")]
    if let Ok(mut lock) = TEST_STORE.lock()
        && let Some(map) = lock.as_mut()
    {
        map.remove(name);
        return Ok(());
    }
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretError::Store(e.to_string())),
    }
}

/// The OpenRouter key: the secret store, else `OPENROUTER_API_KEY` (development and tests).
pub fn openrouter_key() -> Option<Secret<String>> {
    get("openrouter").or_else(environment_openrouter_key)
}

fn environment_openrouter_key() -> Option<Secret<String>> {
    #[cfg(any(test, all(debug_assertions, feature = "dev-env-secrets")))]
    {
        environment_secret(std::env::var("OPENROUTER_API_KEY").ok(), true)
    }
    #[cfg(not(any(test, all(debug_assertions, feature = "dev-env-secrets"))))]
    {
        environment_secret(None, false)
    }
}

/// The Custom OpenAI key: from secret store, or env variable in dev/test.
pub fn custom_openai_key() -> Option<Secret<String>> {
    get("custom_openai").or_else(environment_custom_openai_key)
}

fn environment_custom_openai_key() -> Option<Secret<String>> {
    #[cfg(any(test, all(debug_assertions, feature = "dev-env-secrets")))]
    {
        environment_secret(std::env::var("CUSTOM_OPENAI_API_KEY").ok(), true)
    }
    #[cfg(not(any(test, all(debug_assertions, feature = "dev-env-secrets"))))]
    {
        environment_secret(None, false)
    }
}

/// The Google AI Studio (Gemini) key: from secret store, or env variable in dev/test.
pub fn gemini_key() -> Option<Secret<String>> {
    get("gemini").or_else(environment_gemini_key)
}

fn environment_gemini_key() -> Option<Secret<String>> {
    #[cfg(any(test, all(debug_assertions, feature = "dev-env-secrets")))]
    {
        environment_secret(std::env::var("GEMINI_API_KEY").ok(), true)
    }
    #[cfg(not(any(test, all(debug_assertions, feature = "dev-env-secrets"))))]
    {
        environment_secret(None, false)
    }
}

fn environment_secret(value: Option<String>, enabled: bool) -> Option<Secret<String>> {
    if enabled {
        value.filter(|key| !key.trim().is_empty()).map(Secret::new)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::environment_secret;

    #[test]
    fn environment_fallback_requires_explicit_enablement() {
        assert!(environment_secret(Some("dev-key".to_owned()), true).is_some());
        assert!(environment_secret(Some("production-key".to_owned()), false).is_none());
        assert!(environment_secret(Some("  ".to_owned()), true).is_none());
    }
}
