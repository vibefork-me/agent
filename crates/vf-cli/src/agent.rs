//! The background agent: `start`, `stop`, `restart`, `status`, `open`, `logs`, and the
//! `vibefork://` handler.

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use vf_platform::Settings;
use vf_platform::agent_process::{self, log_file, running_pid};
use vf_platform::settings::HOSTED_UI_ORIGIN;

/// What `GET /v1/hello` answered.
pub(crate) struct Alive {
    pub(crate) version: String,
}

/// Asks the agent on `port` whether it is alive (plain HTTP, no browser headers needed).
pub(crate) fn hello(port: u16) -> Option<Alive> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(
        s,
        "GET /v1/hello HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut text = String::new();
    s.read_to_string(&mut text).ok()?;
    let body = text.split("\r\n\r\n").nth(1)?;
    let v: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    (v["agent"] == "vibefork").then(|| Alive {
        version: v["version"].as_str().unwrap_or_default().to_owned(),
    })
}

fn wait_alive(port: u16, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if hello(port).is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// Starts the agent in the background unless it runs; `true` when it answers.
pub(crate) fn ensure_started(s: &Settings) -> bool {
    if hello(s.port).is_some() {
        return true;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    if let Err(e) = agent_process::start_detached(&exe, &s.state_dir) {
        eprintln!("vibefork: cannot start the agent: {e}");
        return false;
    }
    wait_alive(s.port, Duration::from_secs(10))
}

pub(crate) fn start() -> ExitCode {
    let s = Settings::from_env();
    if !s.extra_origins.is_empty() {
        eprintln!(
            "⚠️  SECURITY WARNING: extraOrigins enabled: {:?}",
            s.extra_origins
        );
        eprintln!("    Web pages on these origins can access the local agent, projects, and API.");
    }
    if hello(s.port).is_some() {
        println!("already running on port {}", s.port);
        return ExitCode::SUCCESS;
    }
    if ensure_started(&s) {
        println!(
            "started on port {} (log: {})",
            s.port,
            log_file(&s.state_dir).display()
        );
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "vibefork: the agent did not start — see {}",
            log_file(&s.state_dir).display()
        );
        ExitCode::FAILURE
    }
}

pub(crate) fn stop() -> ExitCode {
    let s = Settings::from_env();
    if agent_process::stop(&s.state_dir) {
        println!("stopped");
    } else if hello(s.port).is_some() {
        eprintln!(
            "vibefork: an agent answers on port {} but was not started by `vibefork start`",
            s.port
        );
        return ExitCode::FAILURE;
    } else {
        println!("not running");
    }
    ExitCode::SUCCESS
}

/// Restarts a running agent (after an update or a revoked pairing).
pub(crate) fn restart_if_running(s: &Settings) {
    if running_pid(&s.state_dir).is_some() {
        agent_process::stop(&s.state_dir);
        let deadline = Instant::now() + Duration::from_secs(5);
        while hello(s.port).is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
        }
        if ensure_started(s) {
            println!("agent restarted");
        }
    }
}

pub(crate) fn status(json: bool) -> ExitCode {
    let s = Settings::from_env();
    let alive = hello(s.port);
    let pid = running_pid(&s.state_dir);
    if json {
        let v = serde_json::json!({
            "running": alive.is_some(),
            "port": s.port,
            "version": alive.as_ref().map(|a| a.version.clone()),
            "pid": pid,
            "ui": format!("http://127.0.0.1:{}/ui/", s.port),
            "stateDir": s.state_dir.display().to_string(),
            "autoUpdate": s.auto_update,
            "updateAvailable": vf_platform::settings::read_update_notice(&s.state_dir),
        });
        println!("{v}");
    } else {
        match &alive {
            Some(a) => println!(
                "running · version {} · port {} · pid {}",
                a.version,
                s.port,
                pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into())
            ),
            None => println!("not running (port {})", s.port),
        }
        println!(
            "ui: {HOSTED_UI_ORIGIN} · fallback http://127.0.0.1:{}/ui/",
            s.port
        );
        if !s.extra_origins.is_empty() {
            println!("⚠️  extraOrigins enabled: {:?}", s.extra_origins);
        }
        println!(
            "automatic updates: {}",
            if s.auto_update { "on" } else { "off" }
        );
        if let Some(version) = vf_platform::settings::read_update_notice(&s.state_dir) {
            println!("update {version} is available — `vibefork update`");
        }
        println!("data: {}", s.state_dir.display());
    }
    if alive.is_some() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}

pub(crate) fn open(local: bool) -> ExitCode {
    let s = Settings::from_env();
    if !ensure_started(&s) {
        eprintln!(
            "vibefork: the agent did not start — see {}",
            log_file(&s.state_dir).display()
        );
        return ExitCode::FAILURE;
    }
    let url = if local {
        format!("http://127.0.0.1:{}/ui/", s.port)
    } else {
        HOSTED_UI_ORIGIN.to_owned()
    };
    match agent_process::open_url(&url) {
        Ok(()) => {
            println!("{url}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("vibefork: cannot open the browser: {e} — open {url}");
            ExitCode::FAILURE
        }
    }
}

/// `vibefork://…` links. The link is data: nothing in it is executed (invariant 4).
/// `vibefork://start` only starts the agent (the interface wakes it up); any other link starts it
/// and opens the interface (`vibefork://open/local` — the agent's own page).
pub(crate) fn handle(url: &str) -> ExitCode {
    let Some(rest) = url.strip_prefix("vibefork://") else {
        eprintln!("vibefork: not a vibefork:// link");
        return ExitCode::from(2);
    };
    let action = rest.trim_end_matches('/').to_ascii_lowercase();
    // `vibefork://start`: the interface wakes the agent up; no new browser tab.
    if action == "start" {
        let s = Settings::from_env();
        return if ensure_started(&s) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    open(action == "open/local")
}

pub(crate) fn logs(lines: usize) -> ExitCode {
    let s = Settings::from_env();
    let path = log_file(&s.state_dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let all: Vec<&str> = text.lines().collect();
            for l in &all[all.len().saturating_sub(lines)..] {
                println!("{l}");
            }
            ExitCode::SUCCESS
        }
        Err(_) => {
            println!("no log yet ({})", path.display());
            ExitCode::SUCCESS
        }
    }
}
