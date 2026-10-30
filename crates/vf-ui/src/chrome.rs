//! The window's own frame: a title bar with the Vibefork logo and a close button, a thin border
//! around the window. The window has no system frame; dragging the bar moves it.

use eframe::egui::{
    self, Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions,
    ViewportCommand, pos2, vec2,
};

use crate::theme;

/// Height of the title bar, points.
pub(crate) const BAR_HEIGHT: f32 = 48.0;
/// Width of the close button, points.
const CLOSE_WIDTH: f32 = 48.0;
/// Height of the logo in the bar, points.
const LOGO_HEIGHT: f32 = 20.0;
/// The logo: the Vibefork mark and name, light, for a dark background (72 px tall).
const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");
/// The close button under the pointer.
const CLOSE_HOVER: Color32 = Color32::from_rgb(196, 43, 28);
/// The close button while pressed.
const CLOSE_PRESSED: Color32 = Color32::from_rgb(158, 36, 24);

/// The logo as a texture; `None` if the image cannot be decoded (the bar then shows the name).
pub(crate) fn logo(ctx: &egui::Context) -> Option<TextureHandle> {
    let icon = eframe::icon_data::from_png_bytes(LOGO_PNG).ok()?;
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    );
    Some(ctx.load_texture("vf-logo", image, TextureOptions::LINEAR))
}

/// Draws the title bar at the top of the window: the logo on the left, the close button
/// (`close_label` is its accessible name) on the right; the rest of the bar moves the window.
pub(crate) fn title_bar(ui: &mut egui::Ui, logo: Option<&TextureHandle>, close_label: &str) {
    egui::Panel::top("vf-title-bar")
        .exact_size(BAR_HEIGHT)
        .resizable(false)
        .frame(egui::Frame::new().fill(theme::SURFACE))
        .show(ui, |ui| {
            let bar = ui.max_rect();
            let background =
                ui.interact(bar, ui.scope_id().with("vf-bar"), Sense::click_and_drag());
            if background.drag_started_by(egui::PointerButton::Primary) {
                ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            }
            ui.painter().hline(
                bar.x_range(),
                bar.bottom() - 0.5,
                Stroke::new(1.0, theme::BORDER),
            );

            let left = bar.left() + crate::window::MARGIN;
            match logo {
                Some(t) => {
                    let size = t.size_vec2() * (LOGO_HEIGHT / t.size_vec2().y);
                    let rect = Rect::from_min_size(pos2(left, bar.center().y - size.y / 2.0), size);
                    ui.painter().image(
                        t.id(),
                        rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    ui.painter().text(
                        pos2(left, bar.center().y),
                        egui::Align2::LEFT_CENTER,
                        theme::BRAND,
                        egui::FontId::new(16.0, theme::semibold()),
                        theme::TEXT,
                    );
                }
            }
            close_button(ui, bar, close_label);
        });
}

/// The close button at the right end of `bar`.
fn close_button(ui: &mut egui::Ui, bar: Rect, label: &str) {
    let rect = Rect::from_min_max(pos2(bar.right() - CLOSE_WIDTH, bar.top()), bar.max);
    let response = ui.interact(rect, ui.scope_id().with("vf-close"), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let (fill, glyph) = if response.is_pointer_button_down_on() {
        (Some(CLOSE_PRESSED), Color32::WHITE)
    } else if response.hovered() {
        (Some(CLOSE_HOVER), Color32::WHITE)
    } else {
        (None, theme::MUTED)
    };
    let painter = ui.painter();
    if let Some(fill) = fill {
        painter.rect_filled(rect, CornerRadius::ZERO, fill);
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect.shrink(4.0),
            CornerRadius::same(4),
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    let (c, half) = (rect.center(), 5.0);
    let stroke = Stroke::new(1.4, glyph);
    painter.line_segment([c + vec2(-half, -half), c + vec2(half, half)], stroke);
    painter.line_segment([c + vec2(-half, half), c + vec2(half, -half)], stroke);
    if response.clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }
    response.on_hover_text(label);
}

/// A thin border around the whole window: without the system frame the window's edge would
/// melt into a dark desktop.
pub(crate) fn border(ctx: &egui::Context) {
    let rect = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("vf-border"),
    ))
    .rect_stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(1.0, theme::BORDER),
        StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_decodes() {
        let icon = eframe::icon_data::from_png_bytes(LOGO_PNG).expect("logo.png");
        assert_eq!(icon.height, 72);
        assert!(icon.width > icon.height * 4);
    }
}
