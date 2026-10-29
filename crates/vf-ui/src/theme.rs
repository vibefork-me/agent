//! The Vibefork look: the editor's dark palette (its design tokens, converted from OKLCH to sRGB),
//! the editor's typeface (IBM Plex Sans and Plex Mono, built in) and larger text and buttons than
//! egui's defaults.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke,
    TextStyle, Visuals,
};

/// Window background (`--color-surface`).
pub const SURFACE: Color32 = Color32::from_rgb(12, 16, 22);
/// Raised areas: fields, inactive buttons, cards (`--color-surface-elevated`).
pub const ELEVATED: Color32 = Color32::from_rgb(21, 27, 36);
/// Main text (`--color-text`).
pub const TEXT: Color32 = Color32::from_rgb(238, 242, 249);
/// Secondary text (`--color-text-muted`).
pub const MUTED: Color32 = Color32::from_rgb(127, 135, 147);
/// Borders (`--color-border`).
pub const BORDER: Color32 = Color32::from_rgb(45, 51, 61);
/// Accent: focus, the main button, links (`--color-accent`).
pub const ACCENT: Color32 = Color32::from_rgb(89, 170, 248);
/// Success (`--color-success`).
pub const SUCCESS: Color32 = Color32::from_rgb(84, 191, 92);
/// Warning (`--color-warning`).
pub const WARNING: Color32 = Color32::from_rgb(227, 174, 40);
/// Danger (`--color-danger`).
pub const DANGER: Color32 = Color32::from_rgb(233, 70, 70);

/// Text on an accent-coloured button (dark on light blue: contrast above 7:1).
pub const ON_ACCENT: Color32 = SURFACE;

/// The brand, shown above a window's heading.
pub const BRAND: &str = "Vibefork";

/// Font family of headings and labels: IBM Plex Sans SemiBold.
pub fn semibold() -> FontFamily {
    FontFamily::Name("vf-semibold".into())
}

/// The editor's typeface, first in every family; egui's own fonts stay as fallbacks.
fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            "plex-sans",
            &include_bytes!("../fonts/IBMPlexSans-Regular.ttf")[..],
        ),
        (
            "plex-sans-semibold",
            &include_bytes!("../fonts/IBMPlexSans-SemiBold.ttf")[..],
        ),
        (
            "plex-mono",
            &include_bytes!("../fonts/IBMPlexMono-Regular.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (family, first) in [
        (FontFamily::Proportional, "plex-sans"),
        (FontFamily::Monospace, "plex-mono"),
    ] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, first.to_owned());
    }
    let mut strong = vec!["plex-sans-semibold".to_owned(), "plex-sans".to_owned()];
    strong.extend(fallback);
    fonts.families.insert(semibold(), strong);
    fonts
}

/// Applies the Vibefork look to a context: the typeface, dark visuals, the palette, larger text, a
/// visible focus ring.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());

    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = SURFACE;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = ELEVATED;
    visuals.faint_bg_color = ELEVATED;
    visuals.code_bg_color = SURFACE;
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
        w.expansion = 0.0;
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
            (TextStyle::Heading, FontId::new(21.0, semibold())),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::new(15.0, semibold())),
            (TextStyle::Small, FontId::proportional(13.0)),
            (TextStyle::Monospace, FontId::monospace(14.0)),
        ]
        .into();
        style.spacing.button_padding = egui::vec2(18.0, 9.0);
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.interact_size.y = 36.0;
    });
}

/// The main button of a window: accent fill, dark text.
pub fn primary_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.to_owned()).color(ON_ACCENT))
        .fill(ACCENT)
        .min_size(egui::vec2(120.0, 40.0))
}

/// A secondary button of a window.
pub fn secondary_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(label.to_owned()).min_size(egui::vec2(120.0, 40.0))
}

/// The top of a window: the brand in the accent colour, then the heading. A title that starts with
/// the brand ("Vibefork: …", "Vibefork — …") is shown without it, since the brand line already
/// says it.
pub fn header(ui: &mut egui::Ui, title: &str) {
    ui.label(
        RichText::new(BRAND)
            .family(semibold())
            .size(13.0)
            .color(ACCENT),
    );
    ui.add_space(2.0);
    ui.label(RichText::new(heading_of(title)).heading().color(TEXT));
}

/// A window title without a leading brand: "Vibefork: start work?" → "Start work?". Other titles
/// are kept as they are.
pub fn heading_of(title: &str) -> String {
    const SEPARATORS: [char; 5] = [':', '—', '–', '-', ' '];
    let rest = title
        .strip_prefix(BRAND)
        .filter(|r| r.starts_with(SEPARATORS))
        .map(|r| r.trim_start_matches(SEPARATORS));
    let mut chars = rest.unwrap_or_default().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => title.to_owned(),
    }
}

/// The name of a part of a window ("WHAT", "WHY", "TOOLS FOLDER"): small capitals in the muted
/// colour, above what it names.
pub fn section_label(ui: &mut egui::Ui, label: &str) {
    ui.label(
        RichText::new(label.to_uppercase())
            .family(semibold())
            .size(12.0)
            .color(MUTED),
    );
}

/// A raised card that groups the parts of a window.
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(ELEVATED)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(16, 14))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brand_is_not_repeated_in_the_heading() {
        assert_eq!(
            heading_of("Vibefork: начать работу над проектом?"),
            "Начать работу над проектом?"
        );
        assert_eq!(
            heading_of("Vibefork — состояние агента"),
            "Состояние агента"
        );
        assert_eq!(
            heading_of("Добро пожаловать в Vibefork"),
            "Добро пожаловать в Vibefork"
        );
        assert_eq!(heading_of("Vibefork"), "Vibefork");
        assert_eq!(heading_of("Vibeforking"), "Vibeforking");
    }

    #[test]
    fn the_typeface_is_built_in() {
        let f = fonts();
        assert_eq!(
            f.families[&FontFamily::Proportional]
                .first()
                .map(String::as_str),
            Some("plex-sans")
        );
        assert_eq!(
            f.families[&semibold()].first().map(String::as_str),
            Some("plex-sans-semibold")
        );
    }
}
