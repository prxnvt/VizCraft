//! Helpers shared by the graph and sequence mappers: warnings, unique ids, text sizing, Mermaid
//! style properties and the final page size.

use std::collections::{BTreeSet, HashSet};

use vizcraft_diagram::{Color, End, Page, Point, Shape, ShapeKind, Style};
use vizcraft_mermaid::StyleProp;

/// Where the drawing starts on the page, and the space kept to the right and below it.
pub(super) const MARGIN: f64 = 40.0;

/// Average glyph advance at the default 12 pt font, used to estimate text width.
const CHAR_W: f64 = 7.0;
/// Line height at the default font.
pub(super) const LINE_H: f64 = 16.0;
/// Horizontal room around the text inside a shape.
const PAD_X: f64 = 24.0;
/// Vertical room around the text inside a shape.
const PAD_Y: f64 = 16.0;
/// Largest width or height a node gets from its text, so a huge label cannot make a huge page.
const MAX_SIDE: f64 = 4000.0;
/// Most warnings this importer adds on top of the parser's.
const MAX_OWN_WARNINGS: usize = 200;

/// Warnings in order, without repeats.
#[derive(Default)]
pub(super) struct Warnings {
    list: Vec<String>,
    seen: HashSet<String>,
    own: usize,
    dropped: usize,
    unsupported_props: BTreeSet<String>,
}

impl Warnings {
    /// A parser warning, passed through as is.
    pub(super) fn push(&mut self, w: String) {
        self.list.push(w);
    }
    /// A warning of this importer; repeats are dropped and the total is capped.
    pub(super) fn add(&mut self, w: impl Into<String>) {
        let w = w.into();
        if self.seen.contains(&w) {
            return;
        }
        if self.own >= MAX_OWN_WARNINGS {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.own += 1;
        self.seen.insert(w.clone());
        self.list.push(w);
    }
    /// The warnings, with one summary line for style keys that had no mapping.
    pub(super) fn finish(mut self) -> Vec<String> {
        if !self.unsupported_props.is_empty() {
            let keys: Vec<&str> = self.unsupported_props.iter().map(String::as_str).take(20).collect();
            self.list.push(format!("style properties without a VizCraft equivalent were skipped: {}", keys.join(", ")));
        }
        if self.dropped > 0 {
            self.list.push(format!("{} more import warnings not shown", self.dropped));
        }
        self.list
    }
}

/// Hands out ids that are unique on the page, keeping the wanted one when it is free.
#[derive(Default)]
pub(super) struct Ids {
    used: HashSet<String>,
}

impl Ids {
    /// `wanted` when free, else `wanted_2`, `wanted_3` and so on.
    pub(super) fn claim(&mut self, wanted: &str) -> String {
        let base = if wanted.is_empty() { "item" } else { wanted };
        let mut id = base.to_string();
        let mut n: usize = 1;
        // Each failed try is an id already in the set, so this ends within `used.len()` tries.
        while self.used.contains(&id) {
            n = n.saturating_add(1);
            id = format!("{base}_{n}");
        }
        self.used.insert(id.clone());
        id
    }
    /// Reserve an id without handing it out (Mermaid ids that edges refer to).
    pub(super) fn reserve(&mut self, id: &str) {
        self.used.insert(id.to_string());
    }
}

/// The width and height `text` needs at the default font, padding included.
pub(super) fn text_size(text: &str) -> (f64, f64) {
    let mut lines = 0usize;
    let mut longest = 0usize;
    for line in text.split('\n') {
        lines = lines.saturating_add(1);
        longest = longest.max(line.chars().count());
    }
    let w = (longest as f64 * CHAR_W + PAD_X).min(MAX_SIDE);
    let h = (lines as f64 * LINE_H + PAD_Y).min(MAX_SIDE);
    (w, h)
}

/// The frame a shape of `kind` needs so `text` fits inside its outline, never smaller than
/// `min`. Slanted and round outlines leave less room for text than their frame, so they grow.
pub(super) fn fit(kind: ShapeKind, text: &str, min: (f64, f64)) -> (f64, f64) {
    if text.is_empty() {
        return min;
    }
    let (w, h) = text_size(text);
    let (fx, fy) = match kind {
        ShapeKind::Diamond => (1.6, 1.7),
        ShapeKind::Ellipse | ShapeKind::Cloud => (1.35, 1.4),
        ShapeKind::Triangle => (1.8, 1.7),
        ShapeKind::Hexagon | ShapeKind::Preparation | ShapeKind::Parallelogram | ShapeKind::Trapezoid | ShapeKind::ManualInput => (1.3, 1.0),
        ShapeKind::Terminator => (1.2, 1.0),
        ShapeKind::Document | ShapeKind::OffPageReference => (1.0, 1.25),
        ShapeKind::Cylinder => (1.0, 1.35),
        _ => (1.0, 1.0),
    };
    ((w * fx).max(min.0).min(MAX_SIDE), (h * fy).max(min.1).min(MAX_SIDE))
}

/// A shape from the stencil master `id` at the origin, falling back to a plain rectangle if the
/// master is missing (the stencil catalog is ours, so this is a guard, not an expected path).
pub(super) fn from_master(master: &str, id: &str) -> Shape {
    match vizcraft_stencils::master(master) {
        Some(m) => m.instantiate(id, 0.0, 0.0, None, None),
        None => {
            let mut s = Shape::new(id, ShapeKind::Rectangle, 0.0, 0.0, 120.0, 70.0);
            s.stencil = "basic.rectangle".into();
            s
        }
    }
}

/// Which parts of a style a Mermaid property list may set.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    /// A shape: fill, line and text.
    Shape,
    /// A connector: line and text only.
    Connector,
}

/// Apply Mermaid CSS-like properties (`fill`, `stroke`, `stroke-width`, `color`, `font-size`,
/// `font-weight`, `font-style`) to `style`. Returns true when `stroke-dasharray` asked for a
/// dashed line, which the model cannot draw yet, so the caller records it in `ext`.
pub(super) fn apply_props(style: &mut Style, props: &[StyleProp], target: Target, warnings: &mut Warnings) -> bool {
    let mut dashed = false;
    for p in props {
        let key = p.key.trim().to_ascii_lowercase();
        let value = p.value.trim().trim_end_matches("!important").trim();
        let ok = match key.as_str() {
            "fill" | "background" | "background-color" if target == Target::Shape => color(value).map(|c| style.fill = c).is_some(),
            "fill" | "background" | "background-color" => true,
            "stroke" | "border-color" => color(value).map(|c| style.line = c).is_some(),
            "color" => color(value).map(|c| style.font_color = c).is_some(),
            "stroke-width" | "border-width" => length(value).map(|v| style.line_width = v).is_some(),
            "font-size" => length(value).filter(|v| *v > 0.0).map(|v| style.font_size = v).is_some(),
            "font-weight" => {
                style.bold =
                    value.eq_ignore_ascii_case("bold") || value.eq_ignore_ascii_case("bolder") || value.parse::<u32>().is_ok_and(|w| w >= 600);
                true
            }
            "font-style" => {
                style.italic = value.eq_ignore_ascii_case("italic") || value.eq_ignore_ascii_case("oblique");
                true
            }
            "stroke-dasharray" => {
                dashed = !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("none");
                true
            }
            _ => {
                if warnings.unsupported_props.len() < 64 && key.len() <= 40 {
                    warnings.unsupported_props.insert(key.clone());
                }
                true
            }
        };
        if !ok {
            warnings.add(format!("style `{}: {}` was skipped: the value could not be read", short(&key), short(value)));
        }
    }
    style.sanitize();
    dashed
}

/// At most 40 characters of `s`, for warnings that quote input.
pub(super) fn short(s: &str) -> String {
    let mut out: String = s.chars().take(40).collect();
    if s.chars().nth(40).is_some() {
        out.push_str("...");
    }
    out
}

/// A CSS length in pixels or points (`4px`, `2`, `1.5pt`); the unit is ignored.
fn length(v: &str) -> Option<f64> {
    let num = v.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%').trim();
    num.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0)
}

/// A CSS colour: hex through [`Color::parse`], `rgb()` and `rgba()`, `none`, and a few common
/// names.
pub(super) fn color(v: &str) -> Option<Color> {
    let v = v.trim();
    if let Some(c) = Color::parse(v) {
        return Some(c);
    }
    let lower = v.to_ascii_lowercase();
    if let Some(inner) = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb(")).and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let channel =
            |i: usize| parts.get(i).and_then(|p| p.parse::<f64>().ok()).filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 255.0).round() as u8);
        let (r, g, b) = (channel(0)?, channel(1)?, channel(2)?);
        let a = match parts.get(3) {
            Some(a) => (a.parse::<f64>().ok().filter(|x| x.is_finite())?.clamp(0.0, 1.0) * 255.0).round() as u8,
            None => 255,
        };
        return Some(Color { r, g, b, a });
    }
    let named = match lower.as_str() {
        "none" | "transparent" => Color::TRANSPARENT,
        "black" => Color::BLACK,
        "white" => Color::WHITE,
        "red" => Color::rgb(255, 0, 0),
        "green" => Color::rgb(0, 128, 0),
        "lime" => Color::rgb(0, 255, 0),
        "blue" => Color::rgb(0, 0, 255),
        "yellow" => Color::rgb(255, 255, 0),
        "orange" => Color::rgb(255, 165, 0),
        "purple" => Color::rgb(128, 0, 128),
        "pink" => Color::rgb(255, 192, 203),
        "gray" | "grey" => Color::rgb(128, 128, 128),
        "lightgray" | "lightgrey" => Color::rgb(211, 211, 211),
        "darkgray" | "darkgrey" => Color::rgb(169, 169, 169),
        "brown" => Color::rgb(165, 42, 42),
        "cyan" | "aqua" => Color::rgb(0, 255, 255),
        "magenta" | "fuchsia" => Color::rgb(255, 0, 255),
        "navy" => Color::rgb(0, 0, 128),
        "teal" => Color::rgb(0, 128, 128),
        "olive" => Color::rgb(128, 128, 0),
        "maroon" => Color::rgb(128, 0, 0),
        "silver" => Color::rgb(192, 192, 192),
        "gold" => Color::rgb(255, 215, 0),
        "lightblue" => Color::rgb(173, 216, 230),
        "lightgreen" => Color::rgb(144, 238, 144),
        "lightyellow" => Color::rgb(255, 255, 224),
        _ => return None,
    };
    Some(named)
}

/// Grow the page past its default size when the drawing needs more room, keeping a margin.
pub(super) fn fit_page(page: &mut Page) {
    let mut right: f64 = 0.0;
    let mut bottom: f64 = 0.0;
    for s in &page.shapes {
        let b = s.bounds();
        right = right.max(b.x1);
        bottom = bottom.max(b.y1);
    }
    for c in &page.connectors {
        let free = [&c.start, &c.end].into_iter().filter_map(|e| match e {
            End::Free { x, y } => Some(Point::new(*x, *y)),
            End::Glued { .. } => None,
        });
        for p in free.chain(c.waypoints.iter().copied()) {
            right = right.max(p.x);
            bottom = bottom.max(p.y);
        }
    }
    page.width = page.width.max(right + MARGIN);
    page.height = page.height.max(bottom + MARGIN);
    page.sanitize();
}
