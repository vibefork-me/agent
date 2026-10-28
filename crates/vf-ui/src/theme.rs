//! The Vibefork look: the editor's dark palette (its design tokens, converted from OKLCH to sRGB)
//! and larger text and buttons than egui's defaults.

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle, Visuals};

/// Window background (`--color-surface`).
pub const SURFACE: Color32 = Color32::from_rgb(12, 16, 22);
/// Raised areas: fields, inactive buttons (`--color-surface-elevated`).
pub const ELEVATED: Color32 = Color32::from_rgb(21, 27, 36);
/// Main text (`--color-text`).
pub const TEXT: Color32 = Color32::from_rgb(238, 242, 249);
/// Secondary text (`--color-text-muted`).
pub const MUTED: Color32 = Color32::from_rgb(127, 135, 147);
/// Borders (`--color-border`).
pub const BORDER: Color32 = Color32::from_rgb(45, 51, 61);
/// Accent: focus, the main button (`--color-accent`).
pub const ACCENT: Color32 = Color32::from_rgb(89, 170, 248);
/// Success (`--color-success`).
pub const SUCCESS: Color32 = Color32::from_rgb(84, 191, 92);
/// Warning (`--color-warning`).
pub const WARNING: Color32 = Color32::from_rgb(227, 174, 40);
/// Danger (`--color-danger`).
pub const DANGER: Color32 = Color32::from_rgb(233, 70, 70);

/// Text on an accent-coloured button (dark on light blue: contrast above 7:1).
pub const ON_ACCENT: Color32 = SURFACE;

/// Applies the Vibefork look to a context: dark visuals, the palette, larger text, a visible focus
/// ring.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = SURFACE;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = ELEVATED;
    visuals.faint_bg_color = ELEVATED;
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.45);
    visuals.selection.stroke = Stroke::new(2.0, ACCENT);
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    for w in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(6);
        w.fg_stroke = Stroke::new(1.0, TEXT);
    }
    visuals.widgets.inactive.bg_fill = ELEVATED;
    visuals.widgets.inactive.weak_bg_fill = ELEVATED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.weak_bg_fill = BORDER;
    visuals.widgets.hovered.bg_fill = BORDER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
    visuals.widgets.active.weak_bg_fill = BORDER;
    visuals.widgets.active.bg_stroke = Stroke::new(2.0, ACCENT);
    ctx.set_visuals_of(egui::Theme::Dark, visuals);
    ctx.set_theme(egui::ThemePreference::Dark);

    ctx.global_style_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(22.0)),
            (TextStyle::Body, FontId::proportional(16.0)),
            (TextStyle::Button, FontId::proportional(16.0)),
            (TextStyle::Small, FontId::proportional(13.0)),
            (TextStyle::Monospace, FontId::monospace(15.0)),
        ]
        .into();
        style.spacing.button_padding = egui::vec2(18.0, 9.0);
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.interact_size.y = 36.0;
    });
}

/// The main button of a window: accent fill, dark text.
pub fn primary_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label.to_owned())
            .color(ON_ACCENT)
            .strong(),
    )
    .fill(ACCENT)
    .min_size(egui::vec2(120.0, 40.0))
}

/// A secondary button of a window.
pub fn secondary_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(label.to_owned()).min_size(egui::vec2(120.0, 40.0))
}
