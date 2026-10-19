//! Text slots of shapes drawn by glyph builders (`Shape.glyph.labels`): terminal numbers,
//! device tags, the letters and number of an instrument bubble. Each is written after the
//! outline, upright whatever the shape's rotation, in the shape's font colour.

use vizcraft_diagram::geom::Point;
use vizcraft_diagram::glyph_state::GlyphLabel;
use vizcraft_diagram::{Shape, Style};

use super::{Anchor, text_svg};

/// Where a slot sits in world coordinates: its point in the shape's frame carried by the
/// shape's transform. The text itself stays upright.
///
/// This is the one place that maps a slot onto the page; the shape orientation
/// (`Shape.orient`) from the substrate branch changes only this function.
pub fn anchor(shape: &Shape, label: &GlyphLabel) -> Point {
    shape.transform().apply(Point::new(label.x * shape.w, label.y * shape.h))
}

/// Write the slots of `s`, if it has any.
pub(super) fn labels_svg(out: &mut String, s: &Shape) {
    let Some(g) = &s.glyph else { return };
    for l in g.labels.iter().filter(|l| !l.text.trim().is_empty()) {
        let at = anchor(s, l);
        if !at.is_finite() {
            continue;
        }
        let anchor = match l.anchor.as_str() {
            "start" => Anchor::Start,
            "end" => Anchor::End,
            _ => Anchor::Middle,
        };
        let st = Style { font_size: l.size, font_color: s.style.font_color, ..Style::DEFAULT };
        // The slot's y is the middle of the text; the baseline sits a third of the size below.
        text_svg(out, std::slice::from_ref(&l.text), at.x, at.y + l.size * 0.35, anchor, &st, "");
    }
}
