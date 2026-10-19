//! Port marks in SVG export: the ports of each typed shape's element type, drawn as the canvas
//! draws them (`vizcraft_stencils::port_marks`): a filled square for `in`, a hollow one for
//! `out`, a half-filled one for `inOut`, in the shape's line colour, with the label beside it.

use std::fmt::Write as _;

use vizcraft_diagram::{Document, Page, Style};
use vizcraft_stencils::ports::PortDirection;

use super::{Anchor, num, paint, text_svg};

/// The port marks of every drawn typed shape on `page`, in one group after the shapes.
pub(super) fn ports_svg(out: &mut String, doc: &Document, page: &Page, print: bool) {
    let mut body = String::new();
    for s in page.shapes.iter().filter(|s| super::drawn(page, s, print)) {
        let Some(t) = s.element.as_deref().and_then(|e| doc.model.element(e)).map(|e| e.type_id.as_str()) else { continue };
        let colour = s.style.effective_line();
        let width = num(s.style.line_width.clamp(0.5, 4.0));
        for m in vizcraft_stencils::port_marks(s, t) {
            let r = m.rect;
            let (x, y, w, h) = (num(r.x0), num(r.y0), num(r.width()), num(r.height()));
            let fill = match m.direction {
                PortDirection::In => paint("fill", colour),
                PortDirection::Out | PortDirection::InOut => " fill=\"#ffffff\"".to_string(),
            };
            let _ = writeln!(
                body,
                "    <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"{fill}{} stroke-width=\"{width}\"/>",
                paint("stroke", colour)
            );
            if m.direction == PortDirection::InOut {
                let _ = writeln!(body, "    <rect x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"{h}\"{}/>", num(r.width() / 2.0), paint("fill", colour));
            }
            if !m.label.trim().is_empty() {
                let st = Style { font_size: (r.height() * 1.2).clamp(6.0, 14.0), font_color: colour, ..Style::DEFAULT };
                text_svg(&mut body, &[m.label.trim().to_string()], r.x1 + 2.0, r.y1, Anchor::Start, &st, "");
            }
        }
    }
    if !body.is_empty() {
        out.push_str("  <g class=\"ports\">\n");
        out.push_str(&body);
        out.push_str("  </g>\n");
    }
}
