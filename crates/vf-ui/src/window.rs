//! Standalone windows: open a native window (eframe, OpenGL through glow), run a view until it
//! answers or the window closes, return the answer.
//!
//! Each call creates its own event loop. Some window systems allow only one event loop per process
//! over its lifetime; a program that shows several windows in a row should run each in a fresh
//! process (as the agent does) or draw the views into its own egui.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, ViewportCommand};

use crate::dialog::{Answer, Dialog, DialogView};
use crate::first_run::{FirstRun, FirstRunAnswer, FirstRunView};
use crate::status::{StatusEvent, StatusPage, StatusView};
use crate::{UiError, theme};

/// Opens standalone windows with a common icon.
#[derive(Debug, Clone, Default)]
pub struct Shell {
    icon: Option<Arc<egui::IconData>>,
}

impl Shell {
    /// Windows with egui's default icon.
    pub fn new() -> Self {
        Self::default()
    }

    /// Windows with this icon: RGBA pixels, row by row, `width × height`.
    pub fn with_icon(mut self, rgba: Vec<u8>, width: u32, height: u32) -> Self {
        self.icon = Some(Arc::new(egui::IconData {
            rgba,
            width,
            height,
        }));
        self
    }

    /// Shows `dialog` on top of other windows and blocks until it is answered; a link the person
    /// clicks goes to `open` (the window itself cannot open a browser). A closed window is
    /// [`Answer::Rejected`].
    pub fn dialog(&self, dialog: Dialog, mut open: impl FnMut(&str)) -> Result<Answer, UiError> {
        let mut view = DialogView::new(dialog);
        let title = view.dialog().title.clone();
        let detail = view.dialog().permission.as_ref().map_or(0, |p| p.lines());
        let lines = (view.dialog().text.lines().count() + detail) as f32;
        let field = if matches!(view.dialog().kind, crate::DialogKind::Confirm) {
            0.0
        } else {
            48.0
        };
        let height = (170.0 + field + 22.0 * lines).clamp(200.0, 620.0);
        let mut answer = None;
        self.run(&title, egui::vec2(520.0, height), true, |ui| {
            if let Some(a) = view.ui(ui) {
                answer = Some(a);
                ui.ctx().send_viewport_cmd(ViewportCommand::Close);
            }
            if let Some(link) = view.take_link() {
                open(&link);
            }
        })?;
        Ok(answer.unwrap_or(Answer::Rejected))
    }

    /// Shows the first-run window and blocks until the person continues (`Some`) or closes it
    /// (`None`).
    pub fn first_run(&self, run: FirstRun) -> Result<Option<FirstRunAnswer>, UiError> {
        let title = run.text.title.clone();
        let mut view = FirstRunView::new(run);
        let mut answer = None;
        self.run(&title, egui::vec2(600.0, 520.0), false, |ui| {
            if let Some(a) = view.ui(ui) {
                answer = a;
                ui.ctx().send_viewport_cmd(ViewportCommand::Close);
            }
        })?;
        Ok(answer)
    }

    /// Shows the status window until it is closed. `refresh` is asked for the page at start and
    /// every `every`; a button press calls `on_action` with the button's id.
    pub fn status(
        &self,
        every: Duration,
        mut refresh: impl FnMut() -> StatusPage,
        mut on_action: impl FnMut(&str),
    ) -> Result<(), UiError> {
        let first = refresh();
        let title = first.title.clone();
        let mut view = StatusView::new(first);
        let mut last = 0.0_f64;
        self.run(&title, egui::vec2(560.0, 360.0), false, |ui| {
            let now = ui.input(|i| i.time);
            if now - last >= every.as_secs_f64() {
                last = now;
                view.set_page(refresh());
            }
            ui.ctx().request_repaint_after(every);
            match view.ui(ui) {
                Some(StatusEvent::Action(id)) => {
                    on_action(&id);
                    // Show the effect without waiting for the next refresh.
                    last = 0.0;
                }
                Some(StatusEvent::Close) => ui.ctx().send_viewport_cmd(ViewportCommand::Close),
                None => {}
            }
        })
    }

    /// Runs one window around `draw` (called every frame inside a central panel).
    fn run(
        &self,
        title: &str,
        size: egui::Vec2,
        on_top: bool,
        draw: impl FnMut(&mut egui::Ui),
    ) -> Result<(), UiError> {
        let mut viewport = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_min_inner_size(egui::vec2(360.0, 180.0))
            .with_resizable(true)
            .with_minimize_button(false)
            .with_maximize_button(false)
            .with_active(true);
        if on_top {
            viewport = viewport.with_always_on_top();
        }
        if let Some(icon) = &self.icon {
            viewport = viewport.with_icon(icon.clone());
        }
        let options = eframe::NativeOptions {
            viewport,
            centered: true,
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        };
        let host = Host {
            draw: Box::new(draw),
            first_frame: None,
        };
        eframe::run_native(
            "vibefork",
            options,
            Box::new(move |cc| {
                theme::apply(&cc.egui_ctx);
                Ok(Box::new(host))
            }),
        )
        .map_err(|e| UiError::Open(e.to_string()))
    }
}

/// The eframe app around a view.
struct Host<'a> {
    draw: Box<dyn FnMut(&mut egui::Ui) + 'a>,
    /// `ui.input().time` of the first frame.
    first_frame: Option<f64>,
}

/// How long a fresh window keeps asking to be brought to the front.
const FOCUS_FOR_SECS: f64 = 2.0;

impl eframe::App for Host<'_> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Bring the window to the front: it was opened by a background process. The window system
        // takes the request only once the window is visible (after the first frames), so it is
        // repeated until the window has the focus or a moment has passed.
        let (now, has_focus) = ui.input(|i| (i.time, i.viewport().focused == Some(true)));
        let first = *self.first_frame.get_or_insert(now);
        if !has_focus && now - first < FOCUS_FOR_SECS {
            ui.ctx().send_viewport_cmd(ViewportCommand::Focus);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        let draw = &mut self.draw;
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(24.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| draw(ui));
            });
    }
}
