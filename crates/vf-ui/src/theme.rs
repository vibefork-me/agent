//! The Vibefork look: the editor's dark palette (its design tokens, converted from OKLCH to sRGB),
//! the editor's typeface (IBM Plex Sans and Plex Mono, built in) and larger text and buttons than
//! egui's defaults.
//!
//! Text is fitted to the pixel grid by the font's own hinting in both directions ([`crisp`]), and
//! secondary text is lighter than the editor's: egui draws text in shades of grey (no ClearType),
//! and thin grey text on a dark background reads as blurred.

use std::sync::Arc;

use eframe::epaint::text::{HintingTarget, SmoothHinting};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, FontTweak, Painter,
    Rect, RichText, Sense, Stroke, StrokeKind, TextStyle, Visuals, pos2, vec2,
};

/// Window background (`--color-surface`).
pub const SURFACE: Color32 = Color32::from_rgb(12, 16, 22);
/// Raised areas: fields, inactive buttons, cards (`--color-surface-elevated`).
pub const ELEVATED: Color32 = Color32::from_rgb(21, 27, 36);
/// Main text (`--color-text`).
pub const TEXT: Color32 = Color32::from_rgb(238, 242, 249);
/// Secondary text: lighter than the editor's `--color-text-muted` (127, 135, 147), for a contrast
/// of about 7.5:1 on [`SURFACE`] instead of 5:1.
pub const MUTED: Color32 = Color32::from_rgb(158, 166, 178);
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

/// The brand: the logo of the title bar says it, so a heading does not repeat it.
pub const BRAND: &str = "Vibefork";

/// Font family of headings and labels: IBM Plex Sans SemiBold.
pub fn semibold() -> FontFamily {
    FontFamily::Name("vf-semibold".into())
}

/// How the built-in fonts are drawn: hinted by their own TrueType instructions in both
/// directions. egui's default fits outlines only vertically and position-independently, so a
/// vertical stem spreads over two grey pixels; fitted, it covers one.
fn crisp() -> FontTweak {
    FontTweak {
        hinting: Some(true),
        hinting_target: HintingTarget::Smooth(SmoothHinting {
            light: false,
            symmetric_rendering: false,
            preserve_linear_metrics: false,
        }),
        ..FontTweak::default()
    }
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
        fonts.font_data.insert(
            name.to_owned(),
            Arc::new(FontData::from_static(bytes).tweak(crisp())),
        );
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

/// A row of buttons from the right edge, one button high. (A right-to-left layout in a column
/// would take the column's whole height and push the window to grow under its buttons.)
pub fn button_row<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 40.0),
        egui::Layout::right_to_left(egui::Align::Center),
        add,
    )
    .inner
}

/// A secondary button of a window.
pub fn secondary_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(label.to_owned()).min_size(egui::vec2(120.0, 40.0))
}

/// The heading of a window. A title that starts with the brand ("Vibefork: …", "Vibefork — …")
/// is shown without it, since the logo in the title bar already says it.
pub fn header(ui: &mut egui::Ui, title: &str) {
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

/// The site that asks, as a browser shows a secure address: with `checked` (the agent checked the
/// site itself) a green mark with a lock, otherwise a grey one with a globe. `about`, on hover and
/// for screen readers, says what the mark means.
pub fn origin_mark(ui: &mut egui::Ui, host: &str, checked: bool, about: &str) -> egui::Response {
    let (fg, fill, edge) = if checked {
        (
            SUCCESS,
            SUCCESS.gamma_multiply(0.14),
            SUCCESS.gamma_multiply(0.5),
        )
    } else {
        (MUTED, ELEVATED, BORDER)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(host.to_owned(), FontId::new(14.0, semibold()), fg);
    let (pad, icon, gap) = (vec2(12.0, 6.0), 12.0, 7.0);
    let size = vec2(
        pad.x * 2.0 + icon + gap + galley.size().x,
        galley.size().y.max(icon) + pad.y * 2.0,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same((size.y / 2.0) as u8),
        fill,
        Stroke::new(1.0, edge),
        StrokeKind::Inside,
    );
    let mark = Rect::from_center_size(
        pos2(rect.left() + pad.x + icon / 2.0, rect.center().y),
        vec2(icon, icon + 1.0),
    );
    if checked {
        paint_lock(painter, mark, fg);
    } else {
        paint_globe(painter, mark, fg);
    }
    let text_at = pos2(mark.right() + gap, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_at, galley, fg);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, about));
    response.on_hover_text(about)
}

/// A closed padlock in `rect`.
fn paint_lock(painter: &Painter, rect: Rect, colour: Color32) {
    let body = Rect::from_min_max(
        pos2(rect.left(), rect.top() + rect.height() * 0.42),
        rect.max,
    );
    painter.rect_filled(body, CornerRadius::same(2), colour);
    // The shackle: a half circle over two short legs.
    let r = rect.width() * 0.3;
    let c = pos2(rect.center().x, rect.top() + r + 0.8);
    let mut points = vec![pos2(c.x - r, body.top())];
    points.extend((0..=12).map(|i| {
        let a = std::f32::consts::PI * (1.0 + i as f32 / 12.0);
        pos2(c.x + r * a.cos(), c.y + r * a.sin())
    }));
    points.push(pos2(c.x + r, body.top()));
    painter.add(egui::Shape::line(points, Stroke::new(1.8, colour)));
}

/// A globe (a circle with a meridian and the equator) in `rect`.
fn paint_globe(painter: &Painter, rect: Rect, colour: Color32) {
    let stroke = Stroke::new(1.2, colour);
    let (c, r) = (rect.center(), rect.width() / 2.0);
    painter.circle_stroke(c, r, stroke);
    painter.line_segment([pos2(c.x - r, c.y), pos2(c.x + r, c.y)], stroke);
    painter.add(egui::Shape::ellipse_stroke(c, vec2(r * 0.45, r), stroke));
}

/// A warning sign: a triangle with an exclamation mark, `size` points wide.
fn paint_warning(painter: &Painter, rect: Rect, colour: Color32) {
    let (l, r, t, b) = (rect.left(), rect.right(), rect.top(), rect.bottom());
    painter.add(egui::Shape::convex_polygon(
        vec![pos2((l + r) / 2.0, t), pos2(r, b), pos2(l, b)],
        colour,
        Stroke::NONE,
    ));
    let x = (l + r) / 2.0;
    let h = b - t;
    painter.line_segment(
        [pos2(x, t + h * 0.36), pos2(x, t + h * 0.66)],
        Stroke::new(1.8, SURFACE),
    );
    painter.circle_filled(pos2(x, t + h * 0.82), 1.1, SURFACE);
}

/// What to check before answering: a warning sign and the text in an amber-tinted box.
pub fn note(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(WARNING.gamma_multiply(0.09))
        .stroke(Stroke::new(1.0, WARNING.gamma_multiply(0.4)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (sign, _) = ui.allocate_exact_size(vec2(16.0, 20.0), Sense::hover());
                let sign = Rect::from_center_size(sign.center(), vec2(16.0, 14.0));
                paint_warning(ui.painter(), sign, WARNING);
                ui.add(egui::Label::new(RichText::new(text).color(TEXT)).wrap());
            });
        });
}

/// A text quoted as given: on a sunken background behind an accent bar.
pub fn quote(ui: &mut egui::Ui, text: &str) {
    let shown = egui::Frame::new()
        .fill(SURFACE)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin {
            left: 16,
            right: 12,
            top: 10,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).size(16.0).italics().color(TEXT)).wrap());
        });
    let r = shown.response.rect;
    ui.painter().rect_filled(
        Rect::from_min_size(r.min, vec2(3.0, r.height())),
        CornerRadius {
            nw: 6,
            sw: 6,
            ne: 0,
            se: 0,
        },
        ACCENT,
    );
}

/// A labelled value in a card: the label above, the value below.
pub fn fact(ui: &mut egui::Ui, label: &str, value: &str) {
    section_label(ui, label);
    ui.add(
        egui::Label::new(
            RichText::new(value)
                .family(semibold())
                .size(16.0)
                .color(TEXT),
        )
        .wrap(),
    );
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
