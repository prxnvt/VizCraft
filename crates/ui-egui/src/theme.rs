//! Design tokens (light and dark), the UI fonts and the egui style. Widgets never hard-code a
//! colour: everything comes from [`Tokens`], so a craft re-skins by editing this file.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};

/// The app colour (and its darker ink for text on light backgrounds). A craft changes these two.
pub const APP_COLOR: Color32 = Color32::from_rgb(0x2F, 0x7D, 0x6D);
pub const APP_INK: Color32 = Color32::from_rgb(0x1F, 0x5E, 0x52);

/// Every colour the UI uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub title_bar: Color32,
    pub ribbon: Color32,
    pub tab_strip: Color32,
    pub canvas: Color32,
    /// Side panes and other secondary surfaces.
    pub panel: Color32,
    /// The editing surface inside the canvas (the "page").
    pub page: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub icon: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub checked: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    /// Text on an accent-filled button.
    pub on_accent: Color32,
    pub input: Color32,
    pub input_border: Color32,
    pub group_label: Color32,
    pub status_bar: Color32,
    pub selection: Color32,
    pub menu: Color32,
    pub shadow: Color32,
    pub red: Color32,
    pub green: Color32,
}

impl Tokens {
    pub fn light() -> Self {
        Tokens {
            dark: false,
            title_bar: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            ribbon: Color32::from_rgb(0xF9, 0xF9, 0xF9),
            tab_strip: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            canvas: Color32::from_rgb(0xE6, 0xE6, 0xE6),
            panel: Color32::from_rgb(0xFA, 0xFA, 0xFA),
            page: Color32::WHITE,
            border: Color32::from_rgb(0xE1, 0xE1, 0xE1),
            border_strong: Color32::from_rgb(0xC8, 0xC8, 0xC8),
            text: Color32::from_rgb(0x24, 0x24, 0x24),
            text_dim: Color32::from_rgb(0x61, 0x61, 0x61),
            text_disabled: Color32::from_rgb(0xA8, 0xA8, 0xA8),
            icon: Color32::from_rgb(0x42, 0x42, 0x42),
            hover: Color32::from_rgb(0xE6, 0xE6, 0xE6),
            pressed: Color32::from_rgb(0xD6, 0xD6, 0xD6),
            checked: Color32::from_rgb(0xD9, 0xEC, 0xE7),
            accent: APP_COLOR,
            accent_text: APP_INK,
            on_accent: Color32::WHITE,
            input: Color32::WHITE,
            input_border: Color32::from_rgb(0xC7, 0xC7, 0xC7),
            group_label: Color32::from_rgb(0x70, 0x70, 0x70),
            status_bar: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            selection: Color32::from_rgba_unmultiplied(0x2F, 0x7D, 0x6D, 0x48),
            menu: Color32::WHITE,
            shadow: Color32::from_black_alpha(40),
            red: Color32::from_rgb(0xD1, 0x3B, 0x3B),
            green: Color32::from_rgb(0x2E, 0x8B, 0x57),
        }
    }
    pub fn dark() -> Self {
        Tokens {
            dark: true,
            title_bar: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            ribbon: Color32::from_rgb(0x29, 0x29, 0x29),
            tab_strip: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            canvas: Color32::from_rgb(0x14, 0x14, 0x14),
            panel: Color32::from_rgb(0x26, 0x26, 0x26),
            page: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            border: Color32::from_rgb(0x3A, 0x3A, 0x3A),
            border_strong: Color32::from_rgb(0x55, 0x55, 0x55),
            text: Color32::from_rgb(0xEE, 0xEE, 0xEE),
            text_dim: Color32::from_rgb(0xB0, 0xB0, 0xB0),
            text_disabled: Color32::from_rgb(0x6A, 0x6A, 0x6A),
            icon: Color32::from_rgb(0xDD, 0xDD, 0xDD),
            hover: Color32::from_rgb(0x3D, 0x3D, 0x3D),
            pressed: Color32::from_rgb(0x4A, 0x4A, 0x4A),
            checked: Color32::from_rgb(0x24, 0x4A, 0x42),
            accent: Color32::from_rgb(0x5F, 0xB8, 0xA5),
            accent_text: Color32::from_rgb(0x86, 0xCF, 0xBF),
            on_accent: Color32::from_rgb(0x10, 0x10, 0x10),
            input: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            input_border: Color32::from_rgb(0x58, 0x58, 0x58),
            group_label: Color32::from_rgb(0xA0, 0xA0, 0xA0),
            status_bar: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            selection: Color32::from_rgba_unmultiplied(0x5F, 0xB8, 0xA5, 0x55),
            menu: Color32::from_rgb(0x2B, 0x2B, 0x2B),
            shadow: Color32::from_black_alpha(120),
            red: Color32::from_rgb(0xEE, 0x6A, 0x6A),
            green: Color32::from_rgb(0x5B, 0xC0, 0x8A),
        }
    }
    /// The tokens matching the style currently applied to `ctx`.
    pub fn get(ctx: &egui::Context) -> Tokens {
        if ctx.global_style().visuals.dark_mode { Tokens::dark() } else { Tokens::light() }
    }
}

/// Install the UI fonts: Inter for text (three weights), JetBrains Mono for code. egui's own
/// fonts stay in the families as fallbacks for symbols.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "Inter", include_bytes!("../../../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "InterMedium", include_bytes!("../../../assets/fonts/Inter-Medium.ttf"));
    add(&mut fonts, "InterSemiBold", include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "Mono", include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"));
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "Inter".into());
    }
    if let Some(m) = fonts.families.get_mut(&FontFamily::Monospace) {
        m.insert(0, "Mono".into());
    }
    fonts.families.insert(FontFamily::Name("medium".into()), vec!["InterMedium".into(), "Inter".into()]);
    fonts.families.insert(FontFamily::Name("semibold".into()), vec!["InterSemiBold".into(), "Inter".into()]);
    ctx.set_fonts(fonts);
}

/// Inter Medium at `size` points.
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
/// Inter SemiBold at `size` points.
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
/// Inter Regular at `size` points.
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Apply tokens to egui's style, so stock widgets (menus, sliders, text fields) match ours.
pub fn apply(ctx: &egui::Context, t: &Tokens) {
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.ribbon;
    v.window_fill = t.menu;
    v.extreme_bg_color = t.input;
    v.faint_bg_color = t.panel;
    v.window_stroke = Stroke::new(1.0, t.border_strong);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = t.selection;
    v.selection.stroke = Stroke::new(1.0, t.accent);
    v.hyperlink_color = t.accent_text;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.inactive.bg_fill = t.input;
    v.widgets.inactive.weak_bg_fill = t.input;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.input_border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.inactive.corner_radius = CornerRadius::same(4);
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.border_strong);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.hovered.corner_radius = CornerRadius::same(4);
    v.widgets.active.bg_fill = t.pressed;
    v.widgets.active.weak_bg_fill = t.pressed;
    v.widgets.active.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.active.corner_radius = CornerRadius::same(4);
    v.widgets.open.bg_fill = t.pressed;
    v.widgets.open.weak_bg_fill = t.pressed;
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: t.shadow };
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: t.shadow };
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size = egui::vec2(24.0, 22.0);
        s.spacing.menu_margin = egui::Margin::same(6);
        s.text_styles.insert(egui::TextStyle::Body, regular(12.5));
        s.text_styles.insert(egui::TextStyle::Button, regular(12.5));
        s.text_styles.insert(egui::TextStyle::Small, regular(10.5));
        s.text_styles.insert(egui::TextStyle::Heading, semibold(17.0));
        s.interaction.tooltip_delay = 0.45;
    });
}
