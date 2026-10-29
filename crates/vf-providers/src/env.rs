//! The environment of a child process (invariants 6, 7): only the agent's variables on an allow
//! list (system, user folders, locale, proxy, certificates, toolchain homes), plus what the agent
//! sets explicitly. Tokens and keys of the user's environment never reach a child.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use vf_core::sandbox::Wrapper;

/// Variables a child gets from the agent's environment (compared case-insensitively).
const ALLOWED: &[&str] = &[
    // Search path, shell, terminal.
    "PATH",
    "PATHEXT",
    "SHELL",
    "TERM",
    "COLORTERM",
    "NO_COLOR",
    // User and user folders.
    "HOME",
    "USER",
    "LOGNAME",
    "USERNAME",
    "USERDOMAIN",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    // Temporary folders.
    "TEMP",
    "TMP",
    "TMPDIR",
    // Locale and time zone (`LC_*` — by prefix).
    "LANG",
    "LANGUAGE",
    "TZ",
    // Windows system.
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "COMSPEC",
    "OS",
    "COMPUTERNAME",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
    "PROCESSOR_IDENTIFIER",
    "PROCESSOR_LEVEL",
    "PROCESSOR_REVISION",
    "PROGRAMDATA",
    "ALLUSERSPROFILE",
    "PUBLIC",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "COMMONPROGRAMFILES",
    "COMMONPROGRAMFILES(X86)",
    "COMMONPROGRAMW6432",
    "PSMODULEPATH",
    // macOS system.
    "__CF_USER_TEXT_ENCODING",
    // Proxy.
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    // Certificates of a corporate proxy.
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    // Homes of toolchains installed by the user.
    "JAVA_HOME",
    "GRADLE_HOME",
    "ANDROID_HOME",
    "ANDROID_SDK_ROOT",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "GOROOT",
    "GOPATH",
    "DOTNET_ROOT",
];
/// Prefixes of variables a child gets from the agent's environment: locale, user folders on Linux.
const ALLOWED_PREFIXES: &[&str] = &["LC_", "XDG_"];

/// Whether a variable of the agent's environment reaches a child.
fn allowed(key: &OsStr) -> bool {
    let k = key.to_string_lossy().to_ascii_uppercase();
    ALLOWED.contains(&k.as_str()) || ALLOWED_PREFIXES.iter().any(|p| k.starts_with(p))
}

/// Variables of a child process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChildEnv {
    vars: BTreeMap<OsString, OsString>,
    wrapper: Option<Wrapper>,
}

impl ChildEnv {
    /// The agent's environment, allow-listed; `PATH` ends with the per-user program folders it lacks
    /// ([`crate::process::user_bin_dirs`]).
    pub fn from_current() -> Self {
        let mut env = Self::from_vars(std::env::vars_os());
        env.append_path_missing(&crate::process::user_bin_dirs());
        env
    }

    /// Keeps the allow-listed variables of a given set.
    pub fn from_vars(vars: impl IntoIterator<Item = (OsString, OsString)>) -> Self {
        let vars = vars.into_iter().filter(|(k, _)| allowed(k)).collect();
        Self {
            vars,
            wrapper: None,
        }
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

    /// Puts folders at the end of `PATH`, those not on it yet.
    pub fn append_path_missing(&mut self, dirs: &[PathBuf]) {
        let key = self
            .vars
            .keys()
            .find(|k| k.to_string_lossy().eq_ignore_ascii_case("PATH"))
            .cloned()
            .unwrap_or_else(|| "PATH".into());
        let old = self.vars.get(&key).cloned().unwrap_or_default();
        let current: Vec<PathBuf> = std::env::split_paths(&old).collect();
        let missing: Vec<PathBuf> = dirs
            .iter()
            .filter(|d| !current.contains(d))
            .cloned()
            .collect();
        if missing.is_empty() {
            return;
        }
        if let Ok(path) = std::env::join_paths(current.into_iter().chain(missing)) {
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

    /// Runs every command made by [`ChildEnv::command`] through a sandbox launcher.
    pub fn set_wrapper(&mut self, wrapper: Option<Wrapper>) {
        self.wrapper = wrapper;
    }

    /// A command for `program` with these variables — inside the sandbox when a wrapper is set.
    pub fn command(&self, program: &Path) -> Command {
        let mut cmd = match &self.wrapper {
            Some(w) => {
                let mut c = crate::process::command(&w.program);
                c.args(&w.prefix).arg(program);
                c
            }
            None => crate::process::command(program),
        };
        self.apply(&mut cmd);
        if let Some(w) = &self.wrapper {
            cmd.envs(w.env.iter().map(|(k, v)| (k, v)));
        }
        cmd
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
    fn missing_folders_go_to_the_end_of_path() {
        let (a, b) = (PathBuf::from("/a"), PathBuf::from("/b"));
        let mut env = ChildEnv::from_vars([(OsString::from("PATH"), OsString::from("/a"))]);
        env.append_path_missing(&[a.clone(), b.clone()]);
        let path: Vec<PathBuf> =
            std::env::split_paths(env.get("PATH").unwrap_or_default()).collect();
        assert_eq!(path, [a, b]);
    }

    #[test]
    fn parent_session_and_secrets_are_dropped() {
        let env = ChildEnv::from_vars(
            [
                ("CLAUDECODE", "1"),
                ("CLAUDE_CODE_ENTRYPOINT", "x"),
                ("claude_code_sse_port", "1"),
                ("VF_STATE_DIR", "x"),
                ("OPENROUTER_API_KEY", "sk"),
                ("ANTHROPIC_API_KEY", "sk"),
                ("OPENAI_API_KEY", "sk"),
                ("GITHUB_TOKEN", "ghp"),
                ("GH_TOKEN", "ghp"),
                ("NPM_TOKEN", "npm"),
                ("AWS_SECRET_ACCESS_KEY", "aws"),
                ("SSH_AUTH_SOCK", "/s"),
                ("HOME", "/h"),
                ("Path", "/bin"),
            ]
            .map(|(k, v)| (OsString::from(k), OsString::from(v))),
        );
        assert_eq!(env.vars.len(), 2, "{env:?}");
        assert_eq!(env.get("path"), Some(OsStr::new("/bin")));
        assert_eq!(env.get("HOME"), Some(OsStr::new("/h")));
    }

    #[test]
    fn toolchains_keep_system_locale_and_proxy() {
        let keys = [
            "SystemRoot",
            "TEMP",
            "LANG",
            "LC_ALL",
            "https_proxy",
            "NO_PROXY",
            "XDG_RUNTIME_DIR",
            "ProgramFiles(x86)",
            "JAVA_HOME",
        ];
        let env = ChildEnv::from_vars(keys.map(|k| (OsString::from(k), OsString::from("v"))));
        for k in keys {
            assert_eq!(env.get(k), Some(OsStr::new("v")), "{k}");
        }
    }

    #[test]
    fn a_secret_of_the_agent_is_not_seen_by_a_command() {
        let env = ChildEnv::from_vars(
            [("PATH", "/bin"), ("GITHUB_TOKEN", "ghp_secret")]
                .map(|(k, v)| (OsString::from(k), OsString::from(v))),
        );
        let cmd = env.command(Path::new("/bin/sh"));
        assert!(cmd.get_envs().all(|(k, _)| k != "GITHUB_TOKEN"));
        assert!(cmd.get_envs().any(|(k, _)| k == "PATH"));
    }

    #[test]
    fn commands_go_through_the_wrapper() {
        let mut env = ChildEnv::from_vars([(OsString::from("HOME"), OsString::from("/h"))]);
        env.set_wrapper(Some(Wrapper {
            program: PathBuf::from("/agent"),
            prefix: vec!["sandbox-exec".into(), "--".into()],
            env: vec![("VF_SANDBOX_POLICY".into(), "{}".into())],
        }));
        let mut cmd = env.command(Path::new("/bin/sh"));
        cmd.arg("-c");
        assert_eq!(cmd.get_program(), "/agent");
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["sandbox-exec", "--", "/bin/sh", "-c"]);
        assert!(
            cmd.get_envs()
                .any(|(k, v)| k == "VF_SANDBOX_POLICY" && v == Some(OsStr::new("{}")))
        );
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
