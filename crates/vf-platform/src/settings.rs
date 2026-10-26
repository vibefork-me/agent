//! Agent settings: environment variables override `config.json` in the state folder, which overrides
//! defaults.
//!
//! | Variable | `config.json` | Default | Meaning |
//! |---|---|---|---|
//! | `VF_STATE_DIR` | — | Windows `%LOCALAPPDATA%\vibefork`, otherwise `~/.vibefork` | Light data: config, pairings, journals, UI cache, logs |
//! | `VF_PORT` | `port` | last used port, else `47801` | Preferred port (the next 4 are tried if busy) |
//! | `VF_EXTRA_ORIGINS` | `extraOrigins` | — | Extra web origins allowed to call the agent (comma-separated); honoured only in builds with the `dev-extra-origins` feature |
//! | `VF_UI_BUNDLE_URL` | `uiBundleUrl` | `https://code.vibefork.me/bundle` | Where the signed UI bundle is downloaded from |
//! | `VF_PROJECTS_DIR` | `storage.projects` | `~/vibefork-projects` | Projects (git repositories) |
//! | `VF_TOOLS_DIR` | `storage.tools` | `<state>/tools` | Toolchains, `mise` |
//! | `VF_CACHES_DIR` | `storage.caches` | `<state>/caches` | Package caches (Gradle, npm, …) |
//! | `VF_ARTIFACTS_DIR` | `storage.artifacts` | `<state>/artifacts` | Build artifacts |
//! | `VF_UPDATE_URL` | `updateUrl` | `https://code.vibefork.me/releases/stable` | Signed release channel |
//! | `VF_AUTO_UPDATE` | `autoUpdate` | on (off for development builds `0.0.0`) | `0`: check, but do not install |
//! | `VF_SIMULATE_CLEAN_MACHINE` | — | off | `1`: child processes do not see Java / Gradle / Maven of the system (tests) |

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Web origin of the hosted UI.
pub const HOSTED_UI_ORIGIN: &str = "https://code.vibefork.me";
/// Default port.
pub const DEFAULT_PORT: u16 = 47801;
/// Default release channel.
pub const DEFAULT_UPDATE_URL: &str = "https://code.vibefork.me/releases/stable";
/// Default location of the signed UI bundle.
pub const DEFAULT_UI_BUNDLE_URL: &str = "https://code.vibefork.me/bundle";

/// "Heavy" folders; each one can live on any disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Storage {
    /// Projects (git repositories).
    pub projects: PathBuf,
    /// Toolchains and `mise`.
    pub tools: PathBuf,
    /// Package caches.
    pub caches: PathBuf,
    /// Build artifacts.
    pub artifacts: PathBuf,
}

/// The part of the settings stored in `config.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    /// Last used / preferred port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Extra allowed origins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_origins: Vec<String>,
    /// UI bundle URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_bundle_url: Option<String>,
    /// Release channel URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_url: Option<String>,
    /// Install a newer signed release in the background. Absent means the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_update: Option<bool>,
    /// Storage paths; missing ones use defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageConfig>,
    /// CLI home mode per provider ("agent" or "user").
    #[serde(
        default,
        rename = "cliHome",
        alias = "cli_home",
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub cli_home: std::collections::BTreeMap<String, vf_protocol::CliHomeMode>,
    /// Custom OpenAI-compatible endpoint settings.
    #[serde(
        default,
        rename = "customOpenAi",
        alias = "custom_openai",
        skip_serializing_if = "Option::is_none"
    )]
    pub custom_openai: Option<CustomOpenAiConfig>,
}

/// Configuration for custom OpenAI-compatible API endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomOpenAiConfig {
    /// Base URL of the API.
    pub endpoint: String,
    /// Default or configured model name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Storage paths in `config.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Projects folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<PathBuf>,
    /// Tools folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<PathBuf>,
    /// Caches folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caches: Option<PathBuf>,
    /// Artifacts folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<PathBuf>,
}

/// Settings of one agent process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Folder for light data.
    pub state_dir: PathBuf,
    /// Preferred port.
    pub port: u16,
    /// Web origins allowed besides the hosted UI and the agent's own loopback origin.
    /// Always empty in builds without the `dev-extra-origins` feature.
    pub extra_origins: Vec<String>,
    /// Configured extra origins that are not in effect: the build lacks `dev-extra-origins`,
    /// or the value is not `http(s)://host[:port]`.
    pub ignored_extra_origins: Vec<String>,
    /// Signed UI bundle location.
    pub ui_bundle_url: String,
    /// Heavy folders.
    pub storage: Storage,
    /// Signed release channel.
    pub update_url: String,
    /// Install a newer signed release in the background. When this is off, the check still
    /// runs and the available version is recorded for the user to install.
    pub auto_update: bool,
    /// Hide the system's Java / Gradle / Maven from child processes (tests of a clean machine).
    pub simulate_clean_machine: bool,
}

impl Settings {
    /// Reads settings: environment, then `config.json`, then defaults.
    pub fn from_env() -> Self {
        let state_dir = env_path("VF_STATE_DIR").unwrap_or_else(default_state_dir);
        let file = read_config(&state_dir);
        let stored = file.storage.clone().unwrap_or_default();
        let storage = Storage {
            projects: env_path("VF_PROJECTS_DIR")
                .or(stored.projects)
                .unwrap_or_else(|| home().join("vibefork-projects")),
            tools: env_path("VF_TOOLS_DIR")
                .or(stored.tools)
                .unwrap_or_else(|| state_dir.join("tools")),
            caches: env_path("VF_CACHES_DIR")
                .or(stored.caches)
                .unwrap_or_else(|| state_dir.join("caches")),
            artifacts: env_path("VF_ARTIFACTS_DIR")
                .or(stored.artifacts)
                .unwrap_or_else(|| state_dir.join("artifacts")),
        };
        let configured: Vec<String> = std::env::var("VF_EXTRA_ORIGINS")
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_else(|_| file.extra_origins.clone());
        let (extra_origins, ignored_extra_origins) = split_extra_origins(configured);
        Self {
            port: std::env::var("VF_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .or(file.port)
                .unwrap_or(DEFAULT_PORT),
            extra_origins,
            ignored_extra_origins,
            ui_bundle_url: std::env::var("VF_UI_BUNDLE_URL")
                .ok()
                .or(file.ui_bundle_url)
                .unwrap_or_else(|| DEFAULT_UI_BUNDLE_URL.to_owned()),
            storage,
            update_url: std::env::var("VF_UPDATE_URL")
                .ok()
                .or(file.update_url.clone())
                .unwrap_or_else(|| DEFAULT_UPDATE_URL.to_owned()),
            auto_update: {
                let env_auto = std::env::var("VF_AUTO_UPDATE").ok();
                resolve_auto_update(env_auto.as_deref(), file.auto_update)
            },
            state_dir,
            simulate_clean_machine: std::env::var("VF_SIMULATE_CLEAN_MACHINE")
                .is_ok_and(|v| v == "1"),
        }
    }

    /// Folder of the cached UI bundle.
    pub fn ui_cache_dir(&self) -> PathBuf {
        self.state_dir.join("cache").join("ui")
    }

    /// Folder of the project journals.
    pub fn journal_dir(&self) -> PathBuf {
        self.state_dir.join("journal")
    }

    /// Creates the state folder readable by its owner only (Unix `0700`; on Windows the per-user
    /// `%LOCALAPPDATA%` already is).
    pub fn secure_state_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.state_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&self.state_dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    /// Remembers the port the agent bound to, so the browser origin stays the same next time.
    pub fn save_port(&self, port: u16) -> std::io::Result<()> {
        let mut file = read_config(&self.state_dir);
        file.port = Some(port);
        write_config(&self.state_dir, &file)
    }

    /// Stores whether background installs are on. The environment variable still wins on the
    /// next read.
    pub fn save_auto_update(&self, enabled: bool) -> std::io::Result<()> {
        let mut file = read_config(&self.state_dir);
        file.auto_update = Some(enabled);
        write_config(&self.state_dir, &file)
    }

    /// Stores a storage path in `config.json` (used after moving a folder).
    pub fn save_storage(&self, storage: &Storage) -> std::io::Result<()> {
        let mut file = read_config(&self.state_dir);
        file.storage = Some(StorageConfig {
            projects: Some(storage.projects.clone()),
            tools: Some(storage.tools.clone()),
            caches: Some(storage.caches.clone()),
            artifacts: Some(storage.artifacts.clone()),
        });
        write_config(&self.state_dir, &file)
    }

    /// CLI home mode for a provider (default `Agent`).
    pub fn cli_home_mode(&self, provider: &str) -> vf_protocol::CliHomeMode {
        let file = read_config(&self.state_dir);
        file.cli_home.get(provider).copied().unwrap_or_default()
    }

    /// Stores the CLI home mode for a provider in `config.json`.
    pub fn save_cli_home_mode(
        &self,
        provider: &str,
        mode: vf_protocol::CliHomeMode,
    ) -> std::io::Result<()> {
        Self::save_cli_home_mode_at(&self.state_dir, provider, mode)
    }

    /// Stores the CLI home mode for a provider in `state_dir/config.json`.
    pub fn save_cli_home_mode_at(
        state_dir: &Path,
        provider: &str,
        mode: vf_protocol::CliHomeMode,
    ) -> std::io::Result<()> {
        let mut file = read_config(state_dir);
        file.cli_home.insert(provider.to_owned(), mode);
        write_config(state_dir, &file)
    }

    /// Reads custom OpenAI configuration from `config.json`.
    pub fn custom_openai_config(&self) -> Option<CustomOpenAiConfig> {
        read_config(&self.state_dir).custom_openai
    }

    /// Saves or clears custom OpenAI configuration in `config.json`.
    pub fn save_custom_openai_config(
        &self,
        config: Option<&CustomOpenAiConfig>,
    ) -> std::io::Result<()> {
        Self::save_custom_openai_config_at(&self.state_dir, config)
    }

    /// Saves or clears custom OpenAI configuration in `state_dir/config.json`.
    pub fn save_custom_openai_config_at(
        state_dir: &Path,
        config: Option<&CustomOpenAiConfig>,
    ) -> std::io::Result<()> {
        let mut file = read_config(state_dir);
        file.custom_openai = config.cloned();
        write_config(state_dir, &file)
    }
}

pub use vf_core::{endpoint_host_label, validate_custom_openai_endpoint};

/// Reads `config.json`; a missing or broken file counts as empty.
pub fn read_config(state_dir: &Path) -> ConfigFile {
    std::fs::read(state_dir.join("config.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// `autoUpdate` from the environment and the config file.
///
/// `VF_AUTO_UPDATE=0` forces a check without install. Any other value forces install.
/// With no variable, the file wins, and a missing file is on except for a `0.0.0` build.
pub fn resolve_auto_update(env: Option<&str>, configured: Option<bool>) -> bool {
    match env {
        Some("0") => false,
        Some(_) => true,
        None => configured.unwrap_or(env!("CARGO_PKG_VERSION") != "0.0.0"),
    }
}

/// File that names a signed release the user has not installed yet.
pub fn update_notice_path(state_dir: &Path) -> PathBuf {
    state_dir.join("update-available.json")
}

/// Records a version the background check found. The user installs it with `vibefork update`.
pub fn write_update_notice(state_dir: &Path, version: &str) -> std::io::Result<()> {
    if !is_notice_version(version) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "version is not a release number",
        ));
    }
    std::fs::create_dir_all(state_dir)?;
    let bytes = serde_json::to_vec(&UpdateNotice {
        available: version.to_owned(),
    })
    .map_err(std::io::Error::other)?;
    std::fs::write(update_notice_path(state_dir), bytes)
}

/// The recorded version, if the file is present and well formed.
pub fn read_update_notice(state_dir: &Path) -> Option<String> {
    let bytes = std::fs::read(update_notice_path(state_dir)).ok()?;
    let notice: UpdateNotice = serde_json::from_slice(&bytes).ok()?;
    is_notice_version(&notice.available).then_some(notice.available)
}

/// Removes the recorded version (the install finished, or this build is current).
pub fn clear_update_notice(state_dir: &Path) {
    let _ = std::fs::remove_file(update_notice_path(state_dir));
}

fn is_notice_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 64
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

#[derive(Serialize, Deserialize)]
struct UpdateNotice {
    available: String,
}

fn write_config(state_dir: &Path, file: &ConfigFile) -> std::io::Result<()> {
    std::fs::create_dir_all(state_dir)?;
    let bytes = serde_json::to_vec_pretty(file).map_err(std::io::Error::other)?;
    std::fs::write(state_dir.join("config.json"), bytes)
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn default_state_dir() -> PathBuf {
    if cfg!(windows)
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        return PathBuf::from(local).join("vibefork");
    }
    home().join(".vibefork")
}

/// Splits configured extra origins into those in effect and those ignored.
///
/// Builds without the `dev-extra-origins` feature (release builds) put every value in the ignored list.
/// Development builds keep only well-formed `http(s)://host[:port]` origins (no path, no `null`).
pub fn split_extra_origins(configured: Vec<String>) -> (Vec<String>, Vec<String>) {
    if !cfg!(feature = "dev-extra-origins") {
        return (Vec::new(), configured);
    }
    configured.into_iter().partition(|o| is_web_origin(o))
}

/// Whether `s` is a serialized web origin `http(s)://host[:port]`.
fn is_web_origin(s: &str) -> bool {
    let Some(rest) = s
        .strip_prefix("http://")
        .or_else(|| s.strip_prefix("https://"))
    else {
        return false;
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || rest.starts_with('[') => (h, Some(p)),
        _ => (rest, None),
    };
    let host_ok = host == "[::1]"
        || (!host.is_empty()
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'));
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.len() <= 5 && p.parse::<u16>().is_ok());
    host_ok && port_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_file_round_trip_and_bad_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("vf-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read_config(&dir), ConfigFile::default(), "missing file");
        let file = ConfigFile {
            port: Some(47803),
            ..ConfigFile::default()
        };
        assert!(write_config(&dir, &file).is_ok());
        assert_eq!(read_config(&dir).port, Some(47803));
        assert!(std::fs::write(dir.join("config.json"), b"{broken").is_ok());
        assert_eq!(read_config(&dir), ConfigFile::default(), "broken file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_update_env_wins_and_off_is_stored() {
        assert!(!resolve_auto_update(Some("0"), Some(true)));
        assert!(resolve_auto_update(Some("1"), Some(false)));
        assert!(!resolve_auto_update(None, Some(false)));
        assert!(resolve_auto_update(None, Some(true)));
        assert_eq!(
            resolve_auto_update(None, None),
            env!("CARGO_PKG_VERSION") != "0.0.0"
        );

        let dir = temp_dir("auto");
        let settings = Settings {
            state_dir: dir.clone(),
            port: 1,
            extra_origins: Vec::new(),
            ignored_extra_origins: Vec::new(),
            ui_bundle_url: String::new(),
            storage: Storage {
                projects: dir.join("p"),
                tools: dir.join("t"),
                caches: dir.join("c"),
                artifacts: dir.join("a"),
            },
            update_url: String::new(),
            auto_update: true,
            simulate_clean_machine: false,
        };
        assert!(settings.save_auto_update(false).is_ok());
        assert_eq!(read_config(&dir).auto_update, Some(false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_notice_round_trip() {
        let dir = temp_dir("notice");
        assert_eq!(read_update_notice(&dir), None);
        assert!(write_update_notice(&dir, "../1").is_err());
        assert!(write_update_notice(&dir, "1.2.3").is_ok());
        assert_eq!(read_update_notice(&dir).as_deref(), Some("1.2.3"));
        assert!(std::fs::write(update_notice_path(&dir), b"{broken").is_ok());
        assert_eq!(read_update_notice(&dir), None, "broken file");
        assert!(write_update_notice(&dir, "1.2.4").is_ok());
        clear_update_notice(&dir);
        assert_eq!(read_update_notice(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cli_home_persists_and_defaults_to_agent() {
        let dir = temp_dir("cli-home");
        let settings = Settings {
            state_dir: dir.clone(),
            port: 1,
            extra_origins: Vec::new(),
            ignored_extra_origins: Vec::new(),
            ui_bundle_url: String::new(),
            storage: Storage {
                projects: dir.join("p"),
                tools: dir.join("t"),
                caches: dir.join("c"),
                artifacts: dir.join("a"),
            },
            update_url: String::new(),
            auto_update: true,
            simulate_clean_machine: false,
        };
        assert_eq!(
            settings.cli_home_mode("claude"),
            vf_protocol::CliHomeMode::Agent
        );
        assert!(
            settings
                .save_cli_home_mode("claude", vf_protocol::CliHomeMode::User)
                .is_ok()
        );
        assert_eq!(
            settings.cli_home_mode("claude"),
            vf_protocol::CliHomeMode::User
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "vf-settings-{tag}-{}-{nanos}-{counter}",
            std::process::id()
        ))
    }

    #[test]
    fn extra_origins_are_ignored_without_the_dev_feature_and_validated_with_it() {
        let configured = vec![
            "http://localhost:5173".to_owned(),
            "null".to_owned(),
            "https://example.com/path".to_owned(),
            "http://[::1]:8080".to_owned(),
        ];
        let (active, ignored) = split_extra_origins(configured.clone());
        if cfg!(feature = "dev-extra-origins") {
            assert_eq!(active, vec!["http://localhost:5173", "http://[::1]:8080"]);
            assert_eq!(ignored, vec!["null", "https://example.com/path"]);
        } else {
            assert!(active.is_empty());
            assert_eq!(ignored, configured);
        }
    }

    #[test]
    fn custom_openai_endpoint_validation_and_host_label() {
        // Valid loopback http
        assert_eq!(
            validate_custom_openai_endpoint("http://localhost:11434/v1/"),
            Ok("http://localhost:11434/v1".to_owned())
        );
        assert_eq!(
            validate_custom_openai_endpoint("http://127.0.0.1:1234/v1"),
            Ok("http://127.0.0.1:1234/v1".to_owned())
        );
        assert_eq!(
            validate_custom_openai_endpoint("http://[::1]:11434"),
            Ok("http://[::1]:11434".to_owned())
        );

        // Valid remote https
        assert_eq!(
            validate_custom_openai_endpoint("https://api.together.xyz/v1/"),
            Ok("https://api.together.xyz/v1".to_owned())
        );
        assert_eq!(
            validate_custom_openai_endpoint("https://api.openai.com/v1"),
            Ok("https://api.openai.com/v1".to_owned())
        );

        // Invalid: non-loopback http rejected
        assert!(validate_custom_openai_endpoint("http://api.together.xyz/v1").is_err());
        assert!(validate_custom_openai_endpoint("http://192.168.1.50:11434").is_err());
        assert!(validate_custom_openai_endpoint("http://example.com").is_err());

        // Invalid scheme or missing host
        assert!(validate_custom_openai_endpoint("ftp://localhost:11434").is_err());
        assert!(validate_custom_openai_endpoint("localhost:11434").is_err());
        assert!(validate_custom_openai_endpoint("https://").is_err());

        // Host label
        assert_eq!(
            endpoint_host_label("http://localhost:11434/v1"),
            "localhost:11434"
        );
        assert_eq!(
            endpoint_host_label("https://api.together.xyz/v1/chat"),
            "api.together.xyz"
        );
        assert_eq!(endpoint_host_label("http://[::1]:8080/v1"), "[::1]:8080");
    }
}
