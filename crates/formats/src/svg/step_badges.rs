//! Step badges in SVG export: a connector with a `step` shows its number in a filled circle a
//! quarter of the way along its route, placed by `vizcraft_diagram::step_badge::badge` as the
//! canvas places it. PNG and PDF follow, since they render from the SVG.

use std::fmt::Write as _;

use vizcraft_diagram::{Color, Connector, ConnectorGeometry, Style};

use super::{Anchor, num, paint, text_svg};

/// The badge of `c`, inside its connector group; nothing when it shows no step.
pub(super) fn badge_svg(out: &mut String, c: &Connector, g: &ConnectorGeometry) {
    let Some(b) = vizcraft_diagram::step_badge::badge(c, &g.polyline) else { return };
    let line = c.style.effective_line();
    let _ = writeln!(
        out,
        "    <circle cx=\"{}\" cy=\"{}\" r=\"{}\"{}{} stroke-width=\"1\"/>",
        num(b.center.x),
        num(b.center.y),
        num(b.radius),
        paint("fill", line),
        paint("stroke", Color::WHITE)
    );
    let size = (b.radius * 1.1).clamp(6.0, 60.0);
    let st = Style { font_size: size, font_color: Color::WHITE, bold: true, ..Style::DEFAULT };
    text_svg(out, &[b.text], b.center.x, b.center.y + size * 0.35, Anchor::Middle, &st, "");
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::{Connector, Document, End, Point, Shape, ShapeKind, Style};

    use crate::svg::export_page;

    /// The badge placement reads its label box from `connector_look::label_size`; the SVG draws
    /// the box with its own copy of the formula, so the two must agree.
    #[test]
    fn the_svg_label_box_is_as_wide_as_label_size() {
        let mut doc = Document::new();
        let mut c = Connector::new("c", End::free(Point::new(0.0, 100.0)), End::free(Point::new(300.0, 100.0)));
        c.text = "gateway route".into();
        c.step = 1;
        if let Some(p) = doc.page_mut() {
            p.shapes.push(Shape::new("s", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
            p.connectors.push(c.clone());
        }
        doc.sanitize();
        let svg = String::from_utf8(export_page(&doc, 0).unwrap()).unwrap();
        let (w, _) = vizcraft_diagram::connector_look::label_size(&c.text, &Style::default());
        assert!(svg.contains(&format!("width=\"{}\" height=", super::num(w))), "{svg}");
    }
}
