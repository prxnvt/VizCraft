//! Document themes: a named set of colours, a font and effect defaults, in four variants, that
//! styles every shape and connector which has no hand styling.
//!
//! The document stores only which theme and variant are active ([`DocTheme`] on
//! [`Document::theme`](crate::Document::theme)); the themes themselves are built in. Every theme
//! and colour here is our own.
//!
//! Hand styling is kept the way conditional styles keep it: a theme never records which fields it
//! set. Changing theme compares each themed field of an item with the value the old theme gives
//! that item (see [`restyle`]); a field that still has the old theme's value takes the new one, and
//! a field that differs was styled by hand and stays. The value "no theme" gives is the item's
//! base style (its master's style for a shape), so a document made before themes existed restyles
//! fully the first time a theme is applied, and choosing "no theme" again restores the masters'
//! colours.

use serde::{Deserialize, Serialize};

use crate::model::{Color, Style, truncate};
use crate::serde_util::lenient;
use crate::style_fields::{Gradient, GradientKind, GradientStop, Shadow};

/// Variants every theme has.
pub const VARIANTS: usize = 4;

/// Quick styles every theme offers: three intensities of four accent colours.
pub const QUICK_STYLES: usize = 12;

/// The theme and variant a document uses. The default (an empty id) is no theme: items keep their
/// masters' styles.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DocTheme {
    /// Id of a built-in theme ([`themes`]); empty, or an id no theme has, is no theme.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// Variant index, `0..VARIANTS`.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_zero")]
    pub variant: u8,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

impl DocTheme {
    /// The theme `id` in variant `variant`.
    pub fn new(id: impl Into<String>, variant: usize) -> Self {
        let mut t = DocTheme { id: id.into(), variant: variant.min(VARIANTS - 1) as u8 };
        t.sanitize();
        t
    }
    /// Clear an unknown theme id and clamp the variant.
    pub fn sanitize(&mut self) {
        truncate(&mut self.id, crate::model::MAX_ID);
        if theme(&self.id).is_none() {
            self.id.clear();
        }
        self.variant = self.variant.min((VARIANTS - 1) as u8);
    }
    /// The active theme, or `None` for no theme.
    pub fn theme(&self) -> Option<&'static Theme> {
        theme(&self.id)
    }
    /// The active variant's colours, or `None` for no theme.
    pub fn colors(&self) -> Option<Variant> {
        self.theme().map(|t| t.variant(usize::from(self.variant)))
    }
}

/// Effect defaults a theme gives shapes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effects {
    /// Line width of shapes and connectors.
    pub line_width: f64,
    /// Corner rounding of straight-sided shapes.
    pub corner_radius: f64,
    /// Drop shadow under shapes.
    pub shadow: Option<Shadow>,
    /// Shapes get a soft top-to-bottom gradient of their fill colour.
    pub gradient: bool,
}

/// A built-in theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// Stable id stored in documents and taken by `design.setTheme`.
    pub id: &'static str,
    /// Name shown in the gallery.
    pub name: &'static str,
    /// Font family for shape and connector text; empty is the default font.
    pub font: &'static str,
    /// Text colour.
    pub ink: Color,
    /// The colour page backgrounds are tinted from.
    pub paper: Color,
    /// The four accent colours; variant `i` leads with accent `i`.
    pub accents: [Color; VARIANTS],
    /// Variant names, one per accent.
    pub variant_names: [&'static str; VARIANTS],
    /// Effect defaults.
    pub effects: Effects,
}

/// The colours of one theme variant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variant {
    /// Variant name.
    pub name: &'static str,
    /// Shape fill.
    pub fill: Color,
    /// Shape outline.
    pub line: Color,
    /// Connector line.
    pub connector: Color,
    /// Text.
    pub text: Color,
    /// Page background colour for `design.setBackground` when no colour is given.
    pub background: Color,
    /// Accent colours, the variant's lead accent first (quick styles and decorations use them).
    pub accents: [Color; VARIANTS],
}

/// `a` mixed with `b`: `t` = 0 is `a`, 1 is `b`. Alpha is mixed too.
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let m = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color { r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b), a: m(a.a, b.a) }
}

/// A lighter `c` (towards white by `t`), keeping its alpha.
pub fn tint(c: Color, t: f64) -> Color {
    Color { a: c.a, ..mix(c, Color::WHITE, t) }
}

/// A darker `c` (towards black by `t`), keeping its alpha.
pub fn shade(c: Color, t: f64) -> Color {
    Color { a: c.a, ..mix(c, Color::BLACK, t) }
}

impl Theme {
    /// Variant `i` (clamped to the last).
    pub fn variant(&self, i: usize) -> Variant {
        let i = i.min(VARIANTS - 1);
        let lead = self.accents.get(i).copied().unwrap_or(self.ink);
        let mut accents = self.accents;
        accents.rotate_left(i);
        Variant {
            name: self.variant_names.get(i).copied().unwrap_or_default(),
            fill: tint(lead, 0.78),
            line: shade(lead, 0.2),
            connector: mix(lead, self.ink, 0.45),
            text: self.ink,
            background: mix(tint(lead, 0.9), self.paper, 0.5),
            accents,
        }
    }
}

const fn rgb(hex: u32) -> Color {
    Color::rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const fn shadow(dx: f64, dy: f64, blur: f64, alpha: u8) -> Option<Shadow> {
    Some(Shadow { dx, dy, blur, color: Color { r: 0x1a, g: 0x22, b: 0x2c, a: alpha } })
}

/// The built-in themes, in gallery order.
static THEMES: [Theme; 6] = [
    Theme {
        id: "harbor",
        name: "Harbor",
        font: "",
        ink: rgb(0x1d2a36),
        paper: rgb(0xf4f7fa),
        accents: [rgb(0x2a6f97), rgb(0x2c8c88), rgb(0x5b6f8a), rgb(0xd08a3a)],
        variant_names: ["Tide", "Lagoon", "Fog", "Lantern"],
        effects: Effects { line_width: 1.5, corner_radius: 0.0, shadow: None, gradient: false },
    },
    Theme {
        id: "meadow",
        name: "Meadow",
        font: "",
        ink: rgb(0x22301f),
        paper: rgb(0xf6f8f1),
        accents: [rgb(0x4f8a3a), rgb(0x7a9a2e), rgb(0x3d7f6a), rgb(0xb5893a)],
        variant_names: ["Clover", "Fern", "Brook", "Wheat"],
        effects: Effects { line_width: 1.5, corner_radius: 6.0, shadow: None, gradient: false },
    },
    Theme {
        id: "ember",
        name: "Ember",
        font: "",
        ink: rgb(0x33221c),
        paper: rgb(0xfbf6f2),
        accents: [rgb(0xc4452f), rgb(0xd27a1f), rgb(0x9c3b55), rgb(0x6b5a4a)],
        variant_names: ["Coal", "Flame", "Garnet", "Ash"],
        effects: Effects { line_width: 1.25, corner_radius: 0.0, shadow: shadow(3.0, 3.0, 4.0, 0x48), gradient: false },
    },
    Theme {
        id: "slate",
        name: "Slate",
        font: "",
        ink: rgb(0x1f2329),
        paper: rgb(0xf3f4f6),
        accents: [rgb(0x4a5562), rgb(0x2f62a3), rgb(0x76808c), rgb(0x2f7d6d)],
        variant_names: ["Graphite", "Ink", "Pewter", "Moss"],
        effects: Effects { line_width: 1.0, corner_radius: 0.0, shadow: None, gradient: false },
    },
    Theme {
        id: "orchard",
        name: "Orchard",
        font: "",
        ink: rgb(0x2c2233),
        paper: rgb(0xf8f4f8),
        accents: [rgb(0x5a4b8c), rgb(0xa8436b), rgb(0x3f7aa0), rgb(0xc9a227)],
        variant_names: ["Plum", "Blossom", "Dew", "Pear"],
        effects: Effects { line_width: 1.5, corner_radius: 12.0, shadow: shadow(0.0, 2.0, 6.0, 0x44), gradient: false },
    },
    Theme {
        id: "circuit",
        name: "Circuit",
        font: "JetBrains Mono",
        ink: rgb(0x14202b),
        paper: rgb(0xeef3f5),
        accents: [rgb(0x1f7a8c), rgb(0x3a5bc4), rgb(0x2d8a4e), rgb(0xb0412e)],
        variant_names: ["Trace", "Signal", "Ground", "Fault"],
        effects: Effects { line_width: 1.25, corner_radius: 3.0, shadow: None, gradient: true },
    },
];

/// Accents quick styles and decorations use when the document has no theme.
const PLAIN_ACCENTS: [Color; VARIANTS] = [rgb(0x2f62a3), rgb(0x2f7d6d), rgb(0xc4452f), rgb(0x5a4b8c)];

/// Text colour quick styles use when the document has no theme.
const PLAIN_INK: Color = rgb(0x1e2630);

/// Every built-in theme, in gallery order.
pub fn themes() -> &'static [Theme] {
    &THEMES
}

/// The theme with this id.
pub fn theme(id: &str) -> Option<&'static Theme> {
    THEMES.iter().find(|t| t.id == id)
}

/// The accent colours and text colour in effect: the theme variant's, or plain ones without a theme.
pub fn accents(t: &DocTheme) -> ([Color; VARIANTS], Color) {
    match t.colors() {
        Some(v) => (v.accents, v.text),
        None => (PLAIN_ACCENTS, PLAIN_INK),
    }
}

/// The page background colour in effect: the variant's, or a pale grey-blue without a theme.
pub fn background_color(t: &DocTheme) -> Color {
    t.colors().map_or(rgb(0xeef2f6), |v| v.background)
}

/// A gentle vertical gradient of `fill`: lighter at the top.
pub fn soft_gradient(fill: Color) -> Gradient {
    Gradient {
        kind: GradientKind::Linear,
        angle_deg: 90.0,
        stops: vec![GradientStop { offset: 0.0, color: tint(fill, 0.55) }, GradientStop { offset: 1.0, color: fill }],
    }
}

/// The style a shape gets under theme `t`, from its base style (its master's style). No theme
/// gives the base unchanged. A transparent fill or line in the base stays transparent, so text
/// boxes and outline-only shapes keep their look.
pub fn shape_style(t: &DocTheme, base: &Style) -> Style {
    let mut s = base.clone();
    let (Some(th), Some(v)) = (t.theme(), t.colors()) else { return s };
    let filled = base.fill.a > 0;
    if filled {
        s.fill = v.fill;
    }
    if base.line.a > 0 {
        s.line = v.line;
    }
    s.font_color = v.text;
    s.line_width = th.effects.line_width;
    s.font_family = th.font.to_string();
    s.corner_radius = th.effects.corner_radius;
    s.shadow = th.effects.shadow;
    s.gradient = if th.effects.gradient && filled { Some(soft_gradient(v.fill)) } else { None };
    s
}

/// The style a connector gets under theme `t`, from its base style. No theme gives the base.
pub fn connector_style(t: &DocTheme, base: &Style) -> Style {
    let mut s = base.clone();
    let (Some(th), Some(v)) = (t.theme(), t.colors()) else { return s };
    s.line = v.connector;
    s.font_color = v.text;
    s.line_width = th.effects.line_width;
    s.font_family = th.font.to_string();
    s
}

/// Move `cur` from theme style `was` to theme style `now`, field by field: a themed field (fill,
/// line, text colour, line width, font, corner radius, shadow, gradient) that equals `was` takes
/// `now`'s value; one that differs was styled by hand and stays. Returns whether anything changed.
pub fn restyle(cur: &mut Style, was: &Style, now: &Style) -> bool {
    let before = cur.clone();
    macro_rules! field {
        ($($f:ident),*) => {
            $(if cur.$f == was.$f { cur.$f = now.$f.clone(); })*
        };
    }
    field!(fill, line, font_color, line_width, font_family, corner_radius, shadow, gradient);
    *cur != before
}

/// One quick style: the fields it sets.
#[derive(Clone, Debug, PartialEq)]
pub struct QuickStyle {
    /// Name shown as the tooltip.
    pub name: String,
    /// Fill colour.
    pub fill: Color,
    /// Line colour.
    pub line: Color,
    /// Text colour.
    pub text: Color,
    /// Line width.
    pub line_width: f64,
    /// Gradient fill, for the intense row.
    pub gradient: Option<Gradient>,
    /// Shadow, for the intense row.
    pub shadow: Option<Shadow>,
    /// The accent colour the style is built on; connectors take it as their line colour.
    pub accent: Color,
}

impl QuickStyle {
    /// Apply to a shape's style (`with_fill`) or a connector's (line and text only).
    pub fn apply(&self, st: &mut Style, with_fill: bool) {
        if with_fill {
            st.fill = self.fill;
            st.gradient = self.gradient.clone();
            st.shadow = self.shadow;
            st.line = self.line;
            st.font_color = self.text;
        } else {
            st.line = self.accent;
        }
        st.line_width = self.line_width;
        st.sanitize();
    }
}

/// Quick style `n` (`0..QUICK_STYLES`) of theme `t`: row `n / 4` is the intensity (subtle,
/// moderate, intense), column `n % 4` the accent. `None` past the last.
pub fn quick_style(t: &DocTheme, n: usize) -> Option<QuickStyle> {
    if n >= QUICK_STYLES {
        return None;
    }
    let (accents, ink) = accents(t);
    let accent = accents.get(n % VARIANTS).copied().unwrap_or(ink);
    let width = t.theme().map_or(1.5, |th| th.effects.line_width);
    let column = n % VARIANTS + 1;
    Some(match n / VARIANTS {
        0 => QuickStyle {
            name: format!("Subtle, Accent {column}"),
            fill: tint(accent, 0.85),
            line: accent,
            text: ink,
            line_width: width,
            gradient: None,
            shadow: None,
            accent,
        },
        1 => QuickStyle {
            name: format!("Moderate, Accent {column}"),
            fill: accent,
            line: shade(accent, 0.3),
            text: Color::WHITE,
            line_width: width,
            gradient: None,
            shadow: None,
            accent,
        },
        _ => QuickStyle {
            name: format!("Intense, Accent {column}"),
            fill: accent,
            line: shade(accent, 0.45),
            text: Color::WHITE,
            line_width: width.max(1.5) + 0.5,
            gradient: Some(Gradient {
                kind: GradientKind::Linear,
                angle_deg: 90.0,
                stops: vec![GradientStop { offset: 0.0, color: tint(accent, 0.25) }, GradientStop { offset: 1.0, color: shade(accent, 0.2) }],
            }),
            shadow: Some(Shadow { dx: 2.0, dy: 3.0, blur: 4.0, color: Color { r: 0, g: 0, b: 0, a: 0x50 } }),
            accent,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_have_unique_ids_and_distinct_variants() {
        let mut ids = std::collections::HashSet::new();
        assert!(themes().len() >= 6);
        for t in themes() {
            assert!(ids.insert(t.id), "{}", t.id);
            let fills: std::collections::HashSet<String> = (0..VARIANTS).map(|i| t.variant(i).fill.to_hex()).collect();
            assert_eq!(fills.len(), VARIANTS, "{} variants differ", t.id);
            assert_eq!(theme(t.id), Some(t));
        }
        assert!(theme("nope").is_none());
    }

    #[test]
    fn doc_theme_sanitizes_unknown_ids_and_variants() {
        let mut t = DocTheme { id: "nope".into(), variant: 200 };
        t.sanitize();
        assert_eq!(t, DocTheme { id: String::new(), variant: 3 });
        assert_eq!(DocTheme::new("ember", 99).variant, 3);
        let back: DocTheme = serde_json::from_str(r#"{"id": 3, "variant": "x"}"#).unwrap();
        assert_eq!(back, DocTheme::default());
    }

    #[test]
    fn no_theme_keeps_the_base_and_a_theme_keeps_transparency() {
        let base = Style { fill: Color::TRANSPARENT, ..Style::DEFAULT };
        assert_eq!(shape_style(&DocTheme::default(), &base), base);
        let themed = shape_style(&DocTheme::new("harbor", 1), &base);
        assert_eq!(themed.fill, Color::TRANSPARENT);
        assert_ne!(themed.line, base.line);
        let c = connector_style(&DocTheme::new("circuit", 0), &Style::DEFAULT);
        assert_eq!(c.font_family, "JetBrains Mono");
    }

    #[test]
    fn restyle_moves_theme_fields_and_keeps_hand_styling() {
        let base = Style::DEFAULT;
        let a = shape_style(&DocTheme::new("harbor", 0), &base);
        let b = shape_style(&DocTheme::new("ember", 2), &base);
        let mut cur = a.clone();
        cur.fill = Color::rgb(1, 2, 3);
        assert!(restyle(&mut cur, &a, &b));
        assert_eq!(cur.fill, Color::rgb(1, 2, 3), "hand fill stays");
        assert_eq!(cur.line, b.line);
        assert_eq!(cur.shadow, b.shadow);
        // Back to no theme restores the base where the theme was in effect.
        assert!(restyle(&mut cur, &b, &base));
        assert_eq!(cur.line, base.line);
        assert_eq!(cur.shadow, None);
    }

    #[test]
    fn twelve_quick_styles_follow_the_theme() {
        let plain: Vec<QuickStyle> = (0..QUICK_STYLES).filter_map(|n| quick_style(&DocTheme::default(), n)).collect();
        assert_eq!(plain.len(), QUICK_STYLES);
        assert!(quick_style(&DocTheme::default(), QUICK_STYLES).is_none());
        let themed = quick_style(&DocTheme::new("meadow", 0), 4).unwrap();
        assert_ne!(themed.fill, plain[4].fill);
        assert!(quick_style(&DocTheme::new("meadow", 0), 9).unwrap().gradient.is_some());
    }

    #[test]
    fn colour_mixing_clamps() {
        assert_eq!(mix(Color::BLACK, Color::WHITE, 0.5), Color::rgb(128, 128, 128));
        assert_eq!(mix(Color::BLACK, Color::WHITE, f64::NAN), Color::BLACK);
        assert_eq!(tint(Color::BLACK, 9.0), Color::WHITE);
        assert_eq!(shade(Color { a: 7, ..Color::WHITE }, 1.0), Color { r: 0, g: 0, b: 0, a: 7 });
    }
}
