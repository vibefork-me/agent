//! `vibefork-tray`: the tray icon and menu — open the interface, start / stop the agent, quit.
//!
//! A separate process: the core has no GUI dependencies and runs headless; a tray crash does not stop
//! the core. It starts the agent when launched (autostart at login points here), shows its state
//! from `GET /v1/hello`, and acts through the `vibefork` binary next to it.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

/// How often the agent's state is refreshed.
const POLL: Duration = Duration::from_secs(5);

fn main() {
    let core = core_binary();
    run_core(&core, "start");

    let event_loop = EventLoopBuilder::new().build();
    let menu = Menu::new();
    let status = MenuItem::new("vibefork: checking…", false, None);
    let open = MenuItem::new("Open vibefork", true, None);
    let toggle = MenuItem::new("Stop the agent", true, None);
    let quit = MenuItem::new("Quit tray", true, None);
    let added = menu.append_items(&[
        &status,
        &PredefinedMenuItem::separator(),
        &open,
        &toggle,
        &PredefinedMenuItem::separator(),
        &quit,
    ]);
    if added.is_err() {
        return;
    }
    let mut tray = None;
    let mut last_poll = Instant::now() - POLL;
    let mut running = false;
    let port = port();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));
        if let Event::NewEvents(StartCause::Init) = event {
            // The icon is created inside the loop (required on macOS).
            let mut builder = TrayIconBuilder::new()
                .with_menu(Box::new(menu.clone()))
                .with_tooltip("vibefork");
            if let Some(i) = icon(false) {
                builder = builder.with_icon(i);
            }
            tray = builder.build().ok();
        }
        if let Ok(e) = MenuEvent::receiver().try_recv() {
            if e.id == open.id() {
                run_core(&core, "open");
            } else if e.id == toggle.id() {
                run_core(&core, if running { "stop" } else { "start" });
                last_poll = Instant::now() - POLL;
            } else if e.id == quit.id() {
                tray = None;
                *control_flow = ControlFlow::Exit;
                return;
            }
        }
        if last_poll.elapsed() >= POLL {
            last_poll = Instant::now();
            let version = hello(port);
            running = version.is_some();
            status.set_text(match &version {
                Some(v) => format!("vibefork {v}: running on port {port}"),
                None => "vibefork: stopped".into(),
            });
            toggle.set_text(if running {
                "Stop the agent"
            } else {
                "Start the agent"
            });
            open.set_enabled(true);
            if let Some(t) = &tray {
                let _ = t.set_icon(icon(running));
            }
        }
    });
}

/// The `vibefork` binary next to this one.
fn core_binary() -> PathBuf {
    let name = if cfg!(windows) {
        "vibefork.exe"
    } else {
        "vibefork"
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn run_core(core: &PathBuf, command: &str) {
    let mut cmd = Command::new(core);
    cmd.arg(command)
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

/// The agent's port: `VF_PORT`, else the last port in `config.json`, else the default.
fn port() -> u16 {
    std::env::var("VF_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .or_else(|| {
            let state = std::env::var_os("VF_STATE_DIR")
                .map(PathBuf::from)
                .or_else(default_state_dir)?;
            let text = std::fs::read_to_string(state.join("config.json")).ok()?;
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            v["port"].as_u64().and_then(|p| u16::try_from(p).ok())
        })
        .unwrap_or(47801)
}

fn default_state_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("vibefork"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".vibefork"))
    }
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
    s.read_to_string(&mut text).ok()?;
    let v: serde_json::Value = serde_json::from_str(text.split("\r\n\r\n").nth(1)?.trim()).ok()?;
    (v["agent"] == "vibefork").then(|| v["version"].as_str().unwrap_or_default().to_owned())
}

/// A round icon drawn in code: violet when the agent runs, grey when it does not.
fn icon(running: bool) -> Option<Icon> {
    const N: u32 = 32;
    let (r, g, b) = if running {
        (124, 58, 237)
    } else {
        (140, 140, 140)
    };
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let alpha = (15.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[r, g, b, (alpha * 255.0) as u8]);
        }
    }
    Icon::from_rgba(rgba, N, N).ok()
}
