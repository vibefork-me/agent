//! The environment of a child process (invariants 6, 7): the agent's own environment without
//! variables of a parent coding-agent session, the agent's settings and the agent's secrets.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::Command;

/// Prefixes of variables that never reach a child (compared case-insensitively).
const DROPPED_PREFIXES: &[&str] = &["CLAUDECODE", "CLAUDE_CODE_", "CLAUDE_", "VF_"];
/// Secrets the agent itself uses; given only to the provider that needs them.
const DROPPED_SECRETS: &[&str] = &["OPENROUTER_API_KEY"];

/// Variables of a child process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChildEnv {
    vars: BTreeMap<OsString, OsString>,
}

impl ChildEnv {
    /// The agent's environment, cleaned.
    pub fn from_current() -> Self {
        Self::from_vars(std::env::vars_os())
    }

    /// Cleans a given set of variables.
    pub fn from_vars(vars: impl IntoIterator<Item = (OsString, OsString)>) -> Self {
        let vars = vars
            .into_iter()
            .filter(|(k, _)| {
                let k = k.to_string_lossy().to_ascii_uppercase();
                !DROPPED_PREFIXES.iter().any(|p| k.starts_with(p))
                    && !DROPPED_SECRETS.contains(&k.as_str())
            })
            .collect();
        Self { vars }
    }

    /// Sets a variable.
    pub fn set(&mut self, key: impl Into<OsString>, value: impl Into<OsString>) {
        let key = key.into();
        self.remove(&key);
        self.vars.insert(key, value.into());
    }

    /// Removes a variable (any case on Windows).
    pub fn remove(&mut self, key: &OsStr) {
        let upper = key.to_string_lossy().to_ascii_uppercase();
        self.vars
            .retain(|k, _| k.to_string_lossy().to_ascii_uppercase() != upper);
    }

    /// A variable (any case).
    pub fn get(&self, key: &str) -> Option<&OsStr> {
        self.vars
            .iter()
            .find(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_os_str())
    }

    /// Puts folders in front of `PATH`.
    pub fn prepend_path(&mut self, dirs: &[PathBuf]) {
        if dirs.is_empty() {
            return;
        }
        let key = self
            .vars
            .keys()
            .find(|k| k.to_string_lossy().eq_ignore_ascii_case("PATH"))
            .cloned()
            .unwrap_or_else(|| "PATH".into());
        let old = self.vars.get(&key).cloned().unwrap_or_default();
        let joined = std::env::join_paths(dirs.iter().cloned().chain(std::env::split_paths(&old)));
        if let Ok(path) = joined {
            self.vars.insert(key, path);
        }
    }

    /// Removes `PATH` entries matching `hide` (case-insensitive) — the "clean machine" simulation.
    pub fn hide_path_entries(&mut self, hide: &[&str]) {
        let Some((key, old)) = self
            .vars
            .iter()
            .find(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case("PATH"))
            .map(|(k, v)| (k.clone(), v.clone()))
        else {
            return;
        };
        let kept = std::env::split_paths(&old).filter(|p| {
            let s = p.to_string_lossy().to_ascii_lowercase();
            !hide.iter().any(|h| s.contains(h))
        });
        if let Ok(path) = std::env::join_paths(kept) {
            self.vars.insert(key, path);
        }
    }

    /// Replaces the environment of `cmd` with these variables.
    pub fn apply(&self, cmd: &mut Command) {
        cmd.env_clear().envs(&self.vars);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_session_and_agent_secrets_are_dropped() {
        let env = ChildEnv::from_vars(
            [
                ("CLAUDECODE", "1"),
                ("CLAUDE_CODE_ENTRYPOINT", "x"),
                ("claude_code_sse_port", "1"),
                ("VF_STATE_DIR", "x"),
                ("OPENROUTER_API_KEY", "sk"),
                ("HOME", "/h"),
                ("Path", "/bin"),
            ]
            .map(|(k, v)| (OsString::from(k), OsString::from(v))),
        );
        assert_eq!(env.vars.len(), 2, "{env:?}");
        assert_eq!(env.get("path"), Some(OsStr::new("/bin")));
    }

    #[test]
    fn path_is_prepended_and_filtered() {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let mut env = ChildEnv::from_vars([(
            OsString::from("PATH"),
            OsString::from(format!("/usr/bin{sep}/opt/jdk/bin")),
        )]);
        env.prepend_path(&[PathBuf::from("/tools/java/bin")]);
        env.hide_path_entries(&["jdk"]);
        assert_eq!(
            env.get("PATH"),
            Some(OsStr::new(&format!("/tools/java/bin{sep}/usr/bin")))
        );
    }
}
