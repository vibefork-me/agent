//! Linux: a StatusNotifierItem over the session D-Bus (ksni on zbus — pure Rust, no GTK, no
//! libdbus). Shown by KDE, GNOME with the AppIndicator extension, XFCE, Cinnamon, sway / waybar…

use std::sync::mpsc;

use ksni::blocking::TrayMethods as _;

use crate::app::{Action, App, Entry, Model, POLL};
use crate::icon;

struct Sni {
    model: Model,
    actions: mpsc::Sender<Action>,
}

impl ksni::Tray for Sni {
    fn id(&self) -> String {
        "vibefork".into()
    }

    fn title(&self) -> String {
        "Vibefork".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        // RGBA → ARGB, network byte order.
        let data = icon::rgba(self.model.look)
            .chunks_exact(4)
            .flat_map(|p| match *p {
                [r, g, b, a] => [a, r, g, b],
                _ => [0; 4],
            })
            .collect();
        vec![ksni::Icon {
            width: icon::SIZE as i32,
            height: icon::SIZE as i32,
            data,
        }]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.model.tooltip.clone(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        self.model.menu.iter().map(|e| item(e)).collect()
    }
}

fn item(entry: &Entry) -> ksni::MenuItem<Sni> {
    match entry {
        Entry::Separator => ksni::MenuItem::Separator,
        Entry::Sub { label, items } => ksni::menu::SubMenu {
            label: label.clone(),
            submenu: items.iter().map(item).collect(),
            ..Default::default()
        }
        .into(),
        Entry::Item { label, action } => {
            let action = action.clone();
            ksni::menu::StandardItem {
                label: label.replace('_', "__"),
                enabled: action.is_some(),
                activate: Box::new(move |t: &mut Sni| {
                    if let Some(a) = &action {
                        let _ = t.actions.send(a.clone());
                    }
                }),
                ..Default::default()
            }
            .into()
        }
    }
}

/// Runs the tray until "Quit". Without a StatusNotifierWatcher (no tray on this desktop) it still
/// polls and shows notifications.
pub(crate) fn run(mut app: App) {
    let (tx, rx) = mpsc::channel();
    let handle = Sni {
        model: app.initial(),
        actions: tx,
    }
    .assume_sni_available(true)
    .spawn()
    .ok();
    loop {
        let (model, notice) = app.poll();
        if let Some(n) = &notice {
            crate::app::show(n);
        }
        if let Some(h) = &handle {
            h.update(|t| {
                if t.model != model {
                    t.model = model;
                }
            });
        }
        match rx.recv_timeout(POLL) {
            Ok(action) => {
                if !app.act(&action) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if handle.is_some() {
                    break;
                }
                std::thread::sleep(POLL);
            }
        }
    }
    if let Some(h) = handle {
        h.shutdown().wait();
    }
}
