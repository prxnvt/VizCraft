//! The parts of [`Style`](crate::Style) beyond plain fill, line and font: text layout (case,
//! alignment, bullets, direction, margins), line detail (dash, cap) and fill effects (gradient,
//! shadow, glow). Every type has a default that draws exactly as before these fields existed, so
//! files without them look the same.

use serde::{Deserialize, Serialize};

use crate::model::Color;
use crate::serde_util::{lenient, lenient_vec, num};

/// How text letters are cased when drawn; the stored text is never changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextCase {
    /// As typed. Unknown names load as this.
    #[default]
    Normal,
    /// Every letter upper case.
    Upper,
    /// Every letter lower case.
    Lower,
    /// The first letter of every word upper case.
    Title,
}

crate::serde_util::named_enum!(TextCase, TextCase::Normal, {
    TextCase::Normal => "normal",
    TextCase::Upper => "upper",
    TextCase::Lower => "lower",
    TextCase::Title => "title",
});

impl TextCase {
    /// `text` with this case applied.
    pub fn apply(self, text: &str) -> String {
        match self {
            TextCase::Normal => text.to_string(),
            TextCase::Upper => text.to_uppercase(),
            TextCase::Lower => text.to_lowercase(),
            TextCase::Title => {
                let mut out = String::with_capacity(text.len());
                let mut start = true;
                for ch in text.chars() {
                    if start && ch.is_alphabetic() {
                        out.extend(ch.to_uppercase());
                    } else {
                        out.push(ch);
                    }
                    start = ch.is_whitespace();
                }
                out
            }
        }
    }
}

/// Horizontal text alignment inside the text block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HAlign {
    /// Against the left margin.
    Left,
    /// Centred. Unknown names load as this.
    #[default]
    Center,
    /// Against the right margin.
    Right,
    /// Stretched to both margins (drawn left aligned where justification is not available).
    Justify,
}

crate::serde_util::named_enum!(HAlign, HAlign::Center, {
    HAlign::Left => "left",
    HAlign::Center => "center",
    HAlign::Right => "right",
    HAlign::Justify => "justify",
});

/// Vertical text alignment inside the text block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VAlign {
    /// Against the top margin.
    Top,
    /// Centred. Unknown names load as this.
    #[default]
    Middle,
    /// Against the bottom margin.
    Bottom,
}

crate::serde_util::named_enum!(VAlign, VAlign::Middle, {
    VAlign::Top => "top",
    VAlign::Middle => "middle",
    VAlign::Bottom => "bottom",
});

/// A marker drawn before every text line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Bullets {
    /// No marker. Unknown names load as this.
    #[default]
    None,
    /// A round bullet.
    Disc,
    /// A dash.
    Dash,
    /// `1.`, `2.` and so on.
    Number,
}

crate::serde_util::named_enum!(Bullets, Bullets::None, {
    Bullets::None => "none",
    Bullets::Disc => "disc",
    Bullets::Dash => "dash",
    Bullets::Number => "number",
});

impl Bullets {
    /// The marker for line `index` (0 based), with a trailing space; empty for no bullets.
    pub fn marker(self, index: usize) -> String {
        match self {
            Bullets::None => String::new(),
            Bullets::Disc => "\u{2022} ".into(),
            Bullets::Dash => "\u{2013} ".into(),
            Bullets::Number => format!("{}. ", index.saturating_add(1)),
        }
    }
}

/// Which way text runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextDirection {
    /// Left to right. Unknown names load as this.
    #[default]
    LeftToRight,
    /// Right to left.
    RightToLeft,
    /// Top to bottom, lines from right to left.
    Vertical,
}

crate::serde_util::named_enum!(TextDirection, TextDirection::LeftToRight, {
    TextDirection::LeftToRight => "ltr",
    TextDirection::RightToLeft => "rtl",
    TextDirection::Vertical => "vertical",
});

/// Space between the text block edge and the text, in world units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Margins {
    /// Left margin.
    #[serde(deserialize_with = "num")]
    pub left: f64,
    /// Top margin.
    #[serde(deserialize_with = "num")]
    pub top: f64,
    /// Right margin.
    #[serde(deserialize_with = "num")]
    pub right: f64,
    /// Bottom margin.
    #[serde(deserialize_with = "num")]
    pub bottom: f64,
}

impl Margins {
    /// Four units on every side.
    pub const DEFAULT: Margins = Margins { left: 4.0, top: 4.0, right: 4.0, bottom: 4.0 };
    /// Largest margin kept.
    pub const MAX: f64 = 1000.0;

    /// Clamp every side to `0..=MAX`; non-finite sides take the default.
    pub fn sanitize(&mut self) {
        for v in [&mut self.left, &mut self.top, &mut self.right, &mut self.bottom] {
            *v = if v.is_finite() { v.clamp(0.0, Self::MAX) } else { 4.0 };
        }
    }
}

impl Default for Margins {
    fn default() -> Self {
        Margins::DEFAULT
    }
}

/// The dash pattern of a line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Dash {
    /// Unbroken. Unknown names load as this.
    #[default]
    Solid,
    /// Medium dashes.
    Dashed,
    /// Dots.
    Dotted,
    /// Dash, dot, dash.
    DashDot,
    /// Long dashes.
    LongDash,
}

crate::serde_util::named_enum!(Dash, Dash::Solid, {
    Dash::Solid => "solid",
    Dash::Dashed => "dashed",
    Dash::Dotted => "dotted",
    Dash::DashDot => "dashDot",
    Dash::LongDash => "longDash",
});

impl Dash {
    /// Dash and gap lengths as multiples of the line width; empty for a solid line.
    pub fn pattern(self) -> &'static [f64] {
        match self {
            Dash::Solid => &[],
            Dash::Dashed => &[4.0, 3.0],
            Dash::Dotted => &[1.0, 2.0],
            Dash::DashDot => &[4.0, 2.0, 1.0, 2.0],
            Dash::LongDash => &[8.0, 3.0],
        }
    }
}

/// How many outlines a shape draws: one, or a second one inset inside the first (an ER weak
/// entity, a Chen identifying relationship).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Compound {
    /// One outline. Unknown names load as this.
    #[default]
    Single,
    /// The outline and a second one inset by `2.5 * line_width` (see
    /// `crate::connector_look::inner_outline`).
    Double,
}

crate::serde_util::named_enum!(Compound, Compound::Single, {
    Compound::Single => "single",
    Compound::Double => "double",
});

/// How open line ends are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineCap {
    /// Cut square at the end point. Unknown names load as this.
    #[default]
    Butt,
    /// A half circle past the end point.
    Round,
    /// A half square past the end point.
    Square,
}

crate::serde_util::named_enum!(LineCap, LineCap::Butt, {
    LineCap::Butt => "butt",
    LineCap::Round => "round",
    LineCap::Square => "square",
});

/// Whether a gradient runs along a line or out from the centre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GradientKind {
    /// Along a line through the shape at the gradient's angle. Unknown names load as this.
    #[default]
    Linear,
    /// From the centre of the shape outwards.
    Radial,
}

crate::serde_util::named_enum!(GradientKind, GradientKind::Linear, {
    GradientKind::Linear => "linear",
    GradientKind::Radial => "radial",
});

/// One colour stop of a gradient.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GradientStop {
    /// Where the stop sits, 0 (start) to 1 (end).
    #[serde(deserialize_with = "num")]
    pub offset: f64,
    /// The colour at the stop.
    pub color: Color,
}

/// A gradient fill; drawn instead of the solid fill when it has at least two stops.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Gradient {
    /// Linear or radial.
    #[serde(deserialize_with = "lenient")]
    pub kind: GradientKind,
    /// Direction of a linear gradient in degrees clockwise from left-to-right; unused by radial.
    #[serde(deserialize_with = "num")]
    pub angle_deg: f64,
    /// Colour stops in offset order, at most [`Gradient::MAX_STOPS`].
    #[serde(deserialize_with = "lenient_vec")]
    pub stops: Vec<GradientStop>,
}

impl Gradient {
    /// Most stops kept.
    pub const MAX_STOPS: usize = 16;

    /// Keep at most [`Gradient::MAX_STOPS`] stops with offsets in `0..=1`, sorted, and the angle
    /// in `0..360`.
    pub fn sanitize(&mut self) {
        self.angle_deg = if self.angle_deg.is_finite() { self.angle_deg.rem_euclid(360.0) } else { 0.0 };
        self.stops.truncate(Self::MAX_STOPS);
        for s in &mut self.stops {
            s.offset = if s.offset.is_finite() { s.offset.clamp(0.0, 1.0) } else { 0.0 };
        }
        self.stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    }
    /// Whether there is enough to draw a gradient (two or more stops).
    pub fn is_drawable(&self) -> bool {
        self.stops.len() >= 2
    }
}

/// A drop shadow under a shape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Shadow {
    /// Horizontal offset in world units, positive to the right.
    #[serde(deserialize_with = "num")]
    pub dx: f64,
    /// Vertical offset in world units, positive downwards.
    #[serde(deserialize_with = "num")]
    pub dy: f64,
    /// Blur radius in world units; 0 is a hard shadow.
    #[serde(deserialize_with = "num")]
    pub blur: f64,
    /// Shadow colour, usually translucent.
    pub color: Color,
}

impl Default for Shadow {
    fn default() -> Self {
        Shadow { dx: 3.0, dy: 3.0, blur: 2.0, color: Color { r: 0, g: 0, b: 0, a: 0x55 } }
    }
}

impl Shadow {
    /// Largest offset kept either way.
    pub const MAX_OFFSET: f64 = 1000.0;
    /// Largest blur kept.
    pub const MAX_BLUR: f64 = 500.0;

    /// Clamp offsets to `-MAX_OFFSET..=MAX_OFFSET` and the blur to `0..=MAX_BLUR`.
    pub fn sanitize(&mut self) {
        let off = |v: f64| if v.is_finite() { v.clamp(-Self::MAX_OFFSET, Self::MAX_OFFSET) } else { 0.0 };
        self.dx = off(self.dx);
        self.dy = off(self.dy);
        self.blur = if self.blur.is_finite() { self.blur.clamp(0.0, Self::MAX_BLUR) } else { 0.0 };
    }
}

/// A soft coloured halo around a shape's outline.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Glow {
    /// How far the glow reaches past the outline, in world units.
    #[serde(deserialize_with = "num")]
    pub radius: f64,
    /// Glow colour, usually translucent.
    pub color: Color,
}

impl Default for Glow {
    fn default() -> Self {
        Glow { radius: 6.0, color: Color { r: 0xff, g: 0xc0, b: 0x00, a: 0x99 } }
    }
}

impl Glow {
    /// Largest radius kept.
    pub const MAX_RADIUS: f64 = 500.0;

    /// Clamp the radius to `0..=MAX_RADIUS`.
    pub fn sanitize(&mut self) {
        self.radius = if self.radius.is_finite() { self.radius.clamp(0.0, Self::MAX_RADIUS) } else { 0.0 };
    }
}
