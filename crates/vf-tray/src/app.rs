//! The tray's logic, the same on every OS: what the icon, the tooltip and the menu show, which
//! notification a change of state raises, and what a menu item does. The OS backends only draw a
//! [`Model`] and send back [`Action`]s.

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vf_core::catalog::{self, Language};

use crate::status::{self, Activity, Snapshot};

/// How often the agent's state is refreshed.
pub(crate) const POLL: Duration = Duration::from_secs(3);
/// Notifications only for events this fresh (not for what happened while the tray was off).
const NOTIFY_WITHIN_MS: u64 = 2 * 60 * 1000;
/// Longest tooltip (Windows cuts at 127 characters).
const TOOLTIP_CHARS: usize = 120;

/// What the icon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Look {
    /// The agent does not run.
    Stopped,
    /// The agent runs, nothing to do.
    Waiting,
    /// A run goes on.
    Working,
    /// The person has to choose.
    NeedsYou,
    /// The last build failed.
    Error,
}

/// What a menu item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    /// Open the interface.
    Open,
    /// Open the interface on a project.
    OpenProject(String),
    /// Open the agent's log file.
    OpenLog,
    /// Install the recorded update.
    InstallUpdate,
    /// Turn automatic updates on / off.
    ToggleAutoUpdate,
    /// Start or stop the agent.
    ToggleAgent,
    /// Quit the tray (the agent keeps running).
    Quit,
}

/// One entry of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    /// A line of text; without an action it is shown disabled.
    Item {
        /// Label.
        label: String,
        /// What a click does.
        action: Option<Action>,
    },
    /// A submenu.
    Sub {
        /// Label.
        label: String,
        /// Its items.
        items: Vec<Entry>,
    },
    /// A separator.
    Separator,
}

/// Everything a backend draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Model {
    /// The icon.
    pub look: Look,
    /// The tooltip.
    pub tooltip: String,
    /// The menu, top to bottom.
    pub menu: Vec<Entry>,
}

/// A notification to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notice {
    /// Title.
    pub title: String,
    /// Text.
    pub body: String,
}

/// The tray's state between polls.
pub(crate) struct App {
    lang: Language,
    core: PathBuf,
    running: bool,
    /// The last snapshot seen; `None` before the first poll, when nothing is announced.
    seen: Option<Option<Snapshot>>,
}

impl App {
    /// The tray for the `vibefork` binary next to this one, in the system language.
    pub(crate) fn new() -> Self {
        Self {
            lang: vf_platform::system_language(),
            core: core_binary(),
            running: false,
            seen: None,
        }
    }

    /// Starts the agent (the tray is what autostart launches).
    pub(crate) fn start_agent(&self) {
        run_core(&self.core, &["start"]);
    }

    /// The model shown before the first poll.
    pub(crate) fn initial(&self) -> Model {
        Model {
            look: Look::Stopped,
            tooltip: "Vibefork".into(),
            menu: vec![
                Entry::Item {
                    label: catalog::tray_checking(self.lang),
                    action: None,
                },
                Entry::Separator,
                Entry::Item {
                    label: catalog::tray_quit(self.lang),
                    action: Some(Action::Quit),
                },
            ],
        }
    }

    /// Reads the agent's state: the model to draw and a notification, if the state changed into
    /// one worth telling.
    pub(crate) fn poll(&mut self) -> (Model, Option<Notice>) {
        let settings = vf_platform::Settings::from_env();
        let version = hello(settings.port);
        self.running = version.is_some();
        let now = now_ms();
        let (snapshot, recent) = status::read(&settings.canvas_dir(), now);
        let notice = match &self.seen {
            Some(prev) => notice(self.lang, prev.as_ref(), snapshot.as_ref(), now),
            None => None,
        };
        self.seen = Some(snapshot.clone());
        let facts = Facts {
            version,
            port: settings.port,
            update: vf_platform::settings::read_update_notice(&settings.state_dir)
                .filter(|v| safe_version(v)),
            auto_update: settings.auto_update,
            snapshot,
            recent,
        };
        (model(self.lang, &facts), notice)
    }

    /// Does what a menu item says. Returns `false` when the tray should quit.
    pub(crate) fn act(&self, action: &Action) -> bool {
        match action {
            Action::Open => run_core(&self.core, &["open"]),
            Action::OpenProject(name) => {
                if vf_platform::projects::valid_name(name) {
                    run_core(&self.core, &["open", "--project", name]);
                }
            }
            Action::OpenLog => {
                let settings = vf_platform::Settings::from_env();
                let log = vf_platform::agent_process::log_file(&settings.state_dir);
                let _ = vf_platform::agent_process::open_url(&log.to_string_lossy());
            }
            Action::InstallUpdate => run_core(&self.core, &["update"]),
            Action::ToggleAutoUpdate => {
                let next = if vf_platform::Settings::from_env().auto_update {
                    "off"
                } else {
                    "on"
                };
                run_core(&self.core, &["update", "--auto", next]);
            }
            Action::ToggleAgent => {
                run_core(&self.core, &[if self.running { "stop" } else { "start" }]);
            }
            Action::Quit => return false,
        }
        true
    }
}

/// What one poll found.
pub(crate) struct Facts {
    /// Agent version from `GET /v1/hello`; `None` when it does not answer.
    pub version: Option<String>,
    /// The agent's port.
    pub port: u16,
    /// A recorded update that was not installed.
    pub update: Option<String>,
    /// Whether automatic updates are on.
    pub auto_update: bool,
    /// The most recent project's activity.
    pub snapshot: Option<Snapshot>,
    /// Recent projects, most recent first.
    pub recent: Vec<String>,
}

/// The model for a poll's facts.
pub(crate) fn model(lang: Language, f: &Facts) -> Model {
    let running = f.version.is_some();
    let activity = f
        .snapshot
        .as_ref()
        .filter(|_| running)
        .and_then(|s| activity_line(lang, s).map(|line| (&s.activity, line)));
    let look = match activity.as_ref().map(|(a, _)| *a) {
        _ if !running => Look::Stopped,
        Some(Activity::Working { .. }) => Look::Working,
        Some(Activity::NeedsYou) => Look::NeedsYou,
        Some(Activity::Failed) => Look::Error,
        _ => Look::Waiting,
    };
    let update = f.update.as_deref();
    let status = match &f.version {
        Some(v) => catalog::tray_status_running(lang, v, f.port, update),
        None => catalog::tray_status_stopped(lang, update),
    };
    let tooltip = match (&activity, running) {
        (Some((_, line)), _) => format!("Vibefork — {line}"),
        (None, true) => format!("Vibefork — {}", catalog::tray_waiting(lang)),
        (None, false) => catalog::tray_status_stopped(lang, None),
    };
    let mut menu = vec![Entry::Item {
        label: status,
        action: None,
    }];
    if let Some((_, line)) = activity {
        menu.push(Entry::Item {
            label: line,
            action: None,
        });
    }
    if !f.recent.is_empty() {
        menu.push(Entry::Sub {
            label: catalog::tray_recent_projects(lang),
            items: f
                .recent
                .iter()
                .map(|name| Entry::Item {
                    label: name.clone(),
                    action: Some(Action::OpenProject(name.clone())),
                })
                .collect(),
        });
    }
    menu.extend([
        Entry::Separator,
        Entry::Item {
            label: catalog::tray_open_vibefork(lang),
            action: Some(Action::Open),
        },
        Entry::Item {
            label: catalog::tray_open_log(lang),
            action: Some(Action::OpenLog),
        },
        Entry::Separator,
        Entry::Item {
            label: catalog::tray_install_update(lang, update),
            action: update.map(|_| Action::InstallUpdate),
        },
        Entry::Item {
            label: catalog::tray_auto_updates_toggle(lang, f.auto_update),
            action: Some(Action::ToggleAutoUpdate),
        },
        Entry::Separator,
        Entry::Item {
            label: if running {
                catalog::tray_stop_agent(lang)
            } else {
                catalog::tray_start_agent(lang)
            },
            action: Some(Action::ToggleAgent),
        },
        Entry::Item {
            label: catalog::tray_quit(lang),
            action: Some(Action::Quit),
        },
    ]);
    Model {
        look,
        tooltip: tooltip.chars().take(TOOLTIP_CHARS).collect(),
        menu,
    }
}

/// The status line of a project's activity; `None` when there is nothing to tell.
fn activity_line(lang: Language, s: &Snapshot) -> Option<String> {
    match &s.activity {
        Activity::Working { stage, eta_secs } => {
            let stage = stage
                .map(|st| catalog::tray_stage(lang, status::stage_id(st)))
                .unwrap_or_default();
            Some(catalog::tray_activity(lang, &s.project, &stage, *eta_secs))
        }
        Activity::NeedsYou => Some(catalog::tray_needs_you(lang, &s.project)),
        Activity::Failed => Some(catalog::tray_failed(lang, &s.project)),
        Activity::Idle | Activity::Done => None,
    }
}

/// The notification for a change from `prev` to `next`: a new, fresh event that ended a run,
/// failed a build or waits for the person.
pub(crate) fn notice(
    lang: Language,
    prev: Option<&Snapshot>,
    next: Option<&Snapshot>,
    now_ms: u64,
) -> Option<Notice> {
    let next = next?;
    let same_event = prev.is_some_and(|p| p.project == next.project && p.seq == next.seq);
    if same_event || now_ms.saturating_sub(next.at) > NOTIFY_WITHIN_MS {
        return None;
    }
    let title = match next.activity {
        Activity::Done => catalog::notify_done(lang, &next.project),
        Activity::NeedsYou => catalog::notify_needs_you(lang),
        Activity::Failed => catalog::notify_build_failed(lang),
        Activity::Idle | Activity::Working { .. } => return None,
    };
    let body = match next.activity {
        Activity::Done => String::new(),
        _ => next.project.clone(),
    };
    Some(Notice { title, body })
}

/// Shows a notification; failures (no notification service) are ignored.
pub(crate) fn show(notice: &Notice) {
    let mut n = notify_rust::Notification::new();
    n.summary(&notice.title)
        .body(&notice.body)
        .appname("Vibefork");
    #[cfg(windows)]
    n.app_id(vf_platform::install::APP_USER_MODEL_ID);
    let _ = n.show();
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The `vibefork` binary next to this one.
fn core_binary() -> PathBuf {
    let name = vf_platform::install::exe_name("vibefork");
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn run_core(core: &Path, args: &[&str]) {
    let mut cmd = Command::new(core);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.spawn();
}

/// A version string fit for a menu: short, `[A-Za-z0-9._+-]` only.
fn safe_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

/// Version from `GET /v1/hello`, `None` when the agent does not answer.
fn hello(port: u16) -> Option<String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
    write!(
        s,
        "GET /v1/hello HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut text = String::new();
    // Any process can listen on the port: read at most 64 KiB of its answer.
    s.take(64 * 1024).read_to_string(&mut text).ok()?;
    let v: serde_json::Value = serde_json::from_str(text.split("\r\n\r\n").nth(1)?.trim()).ok()?;
    let version = v["version"].as_str().unwrap_or_default();
    (v["agent"] == "vibefork" && (version.is_empty() || safe_version(version)))
        .then(|| version.to_owned())
}

#[cfg(test)]
mod tests {
    use vf_protocol::canvas::Stage;

    use super::*;

    fn snap(seq: u64, at: u64, activity: Activity) -> Snapshot {
        Snapshot {
            project: "notes-app".into(),
            activity,
            seq,
            at,
        }
    }

    fn facts(version: Option<&str>, snapshot: Option<Snapshot>) -> Facts {
        Facts {
            version: version.map(str::to_owned),
            port: 47801,
            update: None,
            auto_update: true,
            snapshot,
            recent: vec!["notes-app".into()],
        }
    }

    fn labels(m: &Model) -> Vec<String> {
        m.menu
            .iter()
            .filter_map(|e| match e {
                Entry::Item { label, .. } | Entry::Sub { label, .. } => Some(label.clone()),
                Entry::Separator => None,
            })
            .collect()
    }

    #[test]
    fn working_project_in_tooltip_and_menu() {
        let working = Activity::Working {
            stage: Some(Stage::Check),
            eta_secs: Some(50),
        };
        let m = model(
            Language::Ru,
            &facts(Some("1.0.0"), Some(snap(3, 0, working))),
        );
        assert_eq!(m.look, Look::Working);
        assert_eq!(m.tooltip, "Vibefork — «notes-app» — проверка, ещё ≈ 1 мин");
        let l = labels(&m);
        assert_eq!(
            l.get(1).map(String::as_str),
            Some("«notes-app» — проверка, ещё ≈ 1 мин")
        );
        assert!(l.iter().any(|x| x == "Недавние проекты"));
        assert!(l.iter().any(|x| x == "Открыть журнал"));
        assert!(m.menu.contains(&Entry::Item {
            label: "Установить обновление".into(),
            action: None,
        }));
    }

    #[test]
    fn looks() {
        let look =
            |v: Option<&str>, a: Activity| model(Language::En, &facts(v, Some(snap(1, 0, a)))).look;
        assert_eq!(look(None, Activity::NeedsYou), Look::Stopped);
        assert_eq!(look(Some("1"), Activity::NeedsYou), Look::NeedsYou);
        assert_eq!(look(Some("1"), Activity::Failed), Look::Error);
        assert_eq!(look(Some("1"), Activity::Done), Look::Waiting);
        assert_eq!(
            model(Language::En, &facts(Some("1"), None)).tooltip,
            "Vibefork — waiting"
        );
        let long = Snapshot {
            project: "n".repeat(64),
            ..snap(1, 0, Activity::Failed)
        };
        assert!(
            model(Language::En, &facts(Some("1"), Some(long)))
                .tooltip
                .chars()
                .count()
                <= TOOLTIP_CHARS
        );
    }

    #[test]
    fn notices_only_for_new_fresh_endings() {
        let now = 1_000_000;
        let working = Activity::Working {
            stage: None,
            eta_secs: None,
        };
        let before = snap(4, now - 5_000, working.clone());
        let done = snap(6, now - 1_000, Activity::Done);
        let n = notice(Language::Ru, Some(&before), Some(&done), now).unwrap();
        assert_eq!(n.title, "Готово: notes-app");
        // The same event again or an old event: nothing; the next task's ending: again.
        assert_eq!(notice(Language::Ru, Some(&done), Some(&done), now), None);
        assert!(
            notice(
                Language::Ru,
                Some(&done),
                Some(&snap(9, now, Activity::Done)),
                now
            )
            .is_some()
        );
        let old = snap(6, now - NOTIFY_WITHIN_MS - 1, Activity::Failed);
        assert_eq!(notice(Language::Ru, Some(&before), Some(&old), now), None);
        // Working tells nothing; a failure and a choice do.
        assert_eq!(
            notice(Language::En, None, Some(&snap(5, now, working)), now),
            None
        );
        let failed = notice(
            Language::En,
            None,
            Some(&snap(5, now, Activity::Failed)),
            now,
        )
        .unwrap();
        assert_eq!(
            (failed.title.as_str(), failed.body.as_str()),
            ("The build failed", "notes-app")
        );
        let choose = notice(
            Language::Ru,
            Some(&before),
            Some(&snap(5, now, Activity::NeedsYou)),
            now,
        );
        assert_eq!(
            choose.map(|n| n.title),
            Some("Агенту нужно твоё решение".to_owned())
        );
    }

    #[test]
    fn versions_for_the_menu() {
        assert!(safe_version("1.2.3-beta.1"));
        assert!(!safe_version(""));
        assert!(!safe_version("1.0 <b>"));
        assert!(!safe_version(&"1".repeat(65)));
    }
}
