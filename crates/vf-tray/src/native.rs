//! Windows / macOS: the system tray (Shell_NotifyIcon / NSStatusItem) through `tray-icon`, driven
//! by a `tao` event loop. These backends use no GTK (it is only on Linux, which has its own).

use std::time::Instant;

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::app::{Action, App, Entry, Model, POLL};
use crate::icon;

/// A built menu and the action of each clickable item (by menu id).
struct Built {
    menu: Menu,
    actions: Vec<(tray_icon::menu::MenuId, Action)>,
}

fn build(model: &Model) -> Option<Built> {
    let menu = Menu::new();
    let mut actions = Vec::new();
    for e in &model.menu {
        match e {
            Entry::Separator => menu.append(&PredefinedMenuItem::separator()).ok()?,
            Entry::Item { label, action } => {
                let item = MenuItem::new(label, action.is_some(), None);
                if let Some(a) = action {
                    actions.push((item.id().clone(), a.clone()));
                }
                menu.append(&item).ok()?;
            }
            Entry::Sub { label, items } => {
                let sub = Submenu::new(label, true);
                for e in items {
                    if let Entry::Item { label, action } = e {
                        let item = MenuItem::new(label, action.is_some(), None);
                        if let Some(a) = action {
                            actions.push((item.id().clone(), a.clone()));
                        }
                        sub.append(&item).ok()?;
                    }
                }
                menu.append(&sub).ok()?;
            }
        }
    }
    Some(Built { menu, actions })
}

fn icon_of(model: &Model) -> Option<Icon> {
    Icon::from_rgba(icon::rgba(model.look), icon::SIZE, icon::SIZE).ok()
}

/// Runs the tray until "Quit".
pub(crate) fn run(mut app: App) {
    let event_loop = EventLoopBuilder::new().build();
    let mut shown = app.initial();
    let mut built = build(&shown);
    let mut tray: Option<TrayIcon> = None;
    let mut next_poll = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(next_poll.min(Instant::now() + POLL / 10));
        if let Event::NewEvents(StartCause::Init) = event {
            // The icon is created inside the loop (required on macOS).
            let mut builder = TrayIconBuilder::new().with_tooltip(&shown.tooltip);
            if let Some(b) = &built {
                builder = builder.with_menu(Box::new(b.menu.clone()));
            }
            if let Some(i) = icon_of(&shown) {
                builder = builder.with_icon(i);
            }
            tray = builder.build().ok();
        }
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            let action = built
                .as_ref()
                .and_then(|b| b.actions.iter().find(|(id, _)| *id == e.id))
                .map(|(_, a)| a.clone());
            if let Some(a) = action {
                if !app.act(&a) {
                    tray = None;
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                // Show the effect of start / stop / auto-update soon.
                next_poll = Instant::now() + POLL / 3;
            }
        }
        if Instant::now() >= next_poll {
            next_poll = Instant::now() + POLL;
            let (model, notice) = app.poll();
            if let Some(n) = &notice {
                crate::app::show(n);
            }
            if model != shown {
                if let Some(t) = &tray {
                    if model.look != shown.look {
                        let _ = t.set_icon(icon_of(&model));
                    }
                    if model.tooltip != shown.tooltip {
                        let _ = t.set_tooltip(Some(&model.tooltip));
                    }
                    if model.menu != shown.menu {
                        built = build(&model);
                        if let Some(b) = &built {
                            t.set_menu(Some(Box::new(b.menu.clone())));
                        }
                    }
                }
                shown = model;
            }
        }
    });
}
