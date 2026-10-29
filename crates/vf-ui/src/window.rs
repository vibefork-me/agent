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
        let mut answer = None;
        self.run(&title, egui::vec2(540.0, 320.0), true, |ui| {
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
        self.run(&title, egui::vec2(600.0, 480.0), false, |ui| {
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
        self.run(&title, egui::vec2(540.0, 300.0), false, |ui| {
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

    /// Runs one window around `draw` (called every frame inside a central panel). The window opens
    /// at `size` and then takes the height of what is drawn (up to [`MAX_HEIGHT`]).
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
            .with_min_inner_size(egui::vec2(360.0, MIN_HEIGHT))
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
/// How long a fresh window follows the height of what it shows; later the person sizes it.
const FIT_FOR_SECS: f64 = 1.0;
/// Margin around a window's content, points.
const MARGIN: f32 = 24.0;
/// Lowest and highest height a window takes by itself, points.
const MIN_HEIGHT: f32 = 160.0;
/// See [`MIN_HEIGHT`].
const MAX_HEIGHT: f32 = 680.0;

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
        let content = egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(MARGIN))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| draw(ui))
                    .content_size
            })
            .inner;
        if now - first < FIT_FOR_SECS {
            fit_height(ui.ctx(), content.y + 2.0 * MARGIN);
        }
    }
}

/// Resizes the window to `wanted` points of height (within [`MIN_HEIGHT`]..[`MAX_HEIGHT`]),
/// keeping its centre where it was.
fn fit_height(ctx: &egui::Context, wanted: f32) {
    let wanted = wanted.clamp(MIN_HEIGHT, MAX_HEIGHT).ceil();
    let (inner, outer) = ctx.input(|i| (i.viewport().inner_rect, i.viewport().outer_rect));
    let Some(inner) = inner else {
        return;
    };
    let grow = wanted - inner.height();
    if grow.abs() < 2.0 {
        return;
    }
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
        inner.width(),
        wanted,
    )));
    if let Some(outer) = outer {
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(
            outer.min - egui::vec2(0.0, grow / 2.0),
        ));
    }
    ctx.request_repaint();
}
