//! SVG export of a shape's compartments (see `vizcraft_diagram::compartment`), from the same
//! [`layout`] the canvas draws and hit testing reads.
//!
//! Inside the shape's `<g>`, after the outline and before the main text: each band's fill
//! override (inset by half the line width so the outline stays whole), the separator line at the
//! top of each band in the outline's stroke, then each band's title (bold, centred), its paragraph
//! lines or one line per list row, aligned left, centre or right with the band's padding. A
//! rotated shape turns the whole set about its centre. Rows past a squeezed band's bottom are
//! left out, as on the canvas.

use std::fmt::Write as _;

use vizcraft_diagram::compartment::{CompartmentKind, LINE_HEIGHT, PAD_X, layout};
use vizcraft_diagram::{HAlign, Point, Rect, Shape, Style};

use super::{Anchor, num, paint, stroke, text_svg};

/// The compartments of `s`, if it has any.
pub(super) fn compartments_svg(out: &mut String, s: &Shape) {
    let bands = layout(s);
    if bands.len() < 2 {
        return;
    }
    let c = s.center();
    let transform = if s.angle_deg != 0.0 { format!(" transform=\"rotate({} {} {})\"", num(s.angle_deg), num(c.x), num(c.y)) } else { String::new() };
    // Positions follow the glyph's quarter turns and mirror; the text drawn at them stays upright
    // (see `Shape::text_frame`). The default orientation maps nothing, as before.
    let orient = (!s.orient.is_default()).then(|| s.orient_transform());
    let world = |p: Point| {
        let p = orient.as_ref().map_or(p, |t| t.apply(p));
        Point::new(p.x + s.x, p.y + s.y)
    };
    let half = (s.style.line_width / 2.0).max(0.0);
    for b in &bands {
        let Some(comp) = b.compartment.and_then(|i| s.compartments.get(i)) else { continue };
        if let Some(fill) = comp.fill.filter(|f| f.a > 0) {
            let bottom = if b.rect.y1 >= s.h - 1e-9 { b.rect.y1 - half } else { b.rect.y1 };
            let r = Rect::new(b.rect.x0 + half, b.rect.y0, b.rect.x1 - half, bottom.max(b.rect.y0));
            let r = match &orient {
                Some(t) => Rect::from_points(t.apply(Point::new(r.x0, r.y0)), t.apply(Point::new(r.x1, r.y1))),
                None => r,
            };
            let _ = writeln!(
                out,
                "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}{transform}/>",
                num(r.x0 + s.x),
                num(r.y0 + s.y),
                num(r.width().max(0.0)),
                num(r.height().max(0.0)),
                paint("fill", fill)
            );
        }
        let (a, z) = (world(Point::new(b.rect.x0, b.rect.y0)), world(Point::new(b.rect.x1, b.rect.y0)));
        let _ = writeln!(out, "    <path d=\"M{} {} L{} {}\" fill=\"none\"{}{transform}/>", num(a.x), num(a.y), num(z.x), num(z.y), stroke(&s.style));
    }
    for b in &bands {
        let Some(comp) = b.compartment.and_then(|i| s.compartments.get(i)) else { continue };
        let st = comp.text_style(&s.style);
        let lh = st.font_size * LINE_HEIGHT;
        if let (Some(title), Some(r)) = (comp.title.as_deref().filter(|t| !t.trim().is_empty()), b.title) {
            let bold = Style { bold: true, ..st.clone() };
            let w = world(r.center());
            text_svg(out, &[title.to_string()], w.x, w.y + st.font_size * 0.35, Anchor::Middle, &bold, &transform);
        }
        let (x, anchor) = match comp.align {
            HAlign::Left | HAlign::Justify => (b.body.x0 + PAD_X, Anchor::Start),
            HAlign::Center => (b.body.center().x, Anchor::Middle),
            HAlign::Right => (b.body.x1 - PAD_X, Anchor::End),
        };
        match comp.kind {
            CompartmentKind::Paragraph if !comp.text.trim().is_empty() => {
                // Lines that fit the band, from its top.
                let fit = ((b.body.height() + 0.5) / lh).floor().max(1.0) as usize;
                let lines: Vec<String> = comp.text.lines().take(fit).map(str::to_string).collect();
                let w = world(Point::new(x, b.body.y0 + lh / 2.0));
                text_svg(out, &lines, w.x, w.y + st.font_size * 0.35, anchor, &st, &transform);
            }
            CompartmentKind::List => {
                for (row, r) in comp.rows.iter().zip(&b.rows) {
                    if row.trim().is_empty() {
                        continue;
                    }
                    let w = world(Point::new(x, r.center().y));
                    text_svg(out, std::slice::from_ref(row), w.x, w.y + st.font_size * 0.35, anchor, &st, &transform);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::compartment::{Compartment, CompartmentKind};
    use vizcraft_diagram::{Color, Document, Shape, ShapeKind};

    use crate::svg::export_page;

    /// A UML-style class: name, attributes, operations.
    pub(crate) fn class_doc() -> Document {
        let mut doc = Document::new();
        let mut s = Shape::new("k", ShapeKind::Rectangle, 10.0, 20.0, 160.0, 100.0);
        s.text = "Order".into();
        let mut attrs = Compartment::new("attrs", CompartmentKind::List);
        attrs.rows = vec!["+ id: int".into(), "+ total: money".into()];
        let mut ops = Compartment::new("ops", CompartmentKind::List);
        ops.rows = vec!["+ pay()".into()];
        ops.fill = Some(Color::rgb(0xee, 0xf2, 0xf7));
        s.compartments = vec![attrs, ops];
        if let Some(p) = doc.page_mut() {
            p.shapes.push(s);
        }
        doc.sanitize();
        doc
    }

    fn body(svg: &str) -> String {
        let start = svg.find("  <g id=").unwrap_or(0);
        svg[start..].trim_end().to_string()
    }

    #[test]
    fn golden_uml_class() {
        let svg = String::from_utf8(export_page(&class_doc(), 0).unwrap()).unwrap();
        let want = r##"  <g id="k">
    <path d="M10 20 L170 20 L170 120 L10 120 Z" fill="#ffffff" stroke="#3a4a5c" stroke-width="1.5"/>
    <path d="M10 60.8 L170 60.8" fill="none" stroke="#3a4a5c" stroke-width="1.5"/>
    <rect x="10.75" y="97.6" width="158.5" height="21.65" fill="#eef2f7"/>
    <path d="M10 97.6 L170 97.6" fill="none" stroke="#3a4a5c" stroke-width="1.5"/>
    <text text-anchor="start" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="16" y="76.2">+ id: int</tspan></text>
    <text text-anchor="start" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="16" y="90.6">+ total: money</tspan></text>
    <text text-anchor="start" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="16" y="113">+ pay()</tspan></text>
    <text text-anchor="middle" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="90" y="44.6">Order</tspan></text>
  </g>
</svg>"##;
        assert_eq!(body(&svg), want);
    }

    #[test]
    fn golden_equipment_tag_and_rotation() {
        let mut doc = Document::new();
        let mut s = Shape::new("t", ShapeKind::Rectangle, 0.0, 0.0, 120.0, 90.0);
        s.text = "P-101".into();
        let mut desc = Compartment::new("desc", CompartmentKind::Paragraph);
        desc.text = "Feed pump".into();
        desc.align = vizcraft_diagram::HAlign::Center;
        let mut fields = Compartment::new("fields", CompartmentKind::List);
        fields.title = Some("Fields".into());
        fields.rows = vec!["Flow: 12 m3/h".into()];
        fields.bold = Some(false);
        fields.font_size = Some(10.0);
        s.compartments = vec![desc, fields];
        s.angle_deg = 90.0;
        if let Some(p) = doc.page_mut() {
            p.shapes.push(s);
        }
        doc.sanitize();
        let svg = String::from_utf8(export_page(&doc, 0).unwrap()).unwrap();
        assert!(svg.contains(r#"<tspan x="60" y="51">Feed pump</tspan>"#), "{svg}");
        assert!(svg.contains(r##"font-size="10" font-weight="bold" fill="#1e2630" transform="rotate(90 60 45)" xml:space="preserve"><tspan x="60" y="71.5">Fields</tspan>"##), "{svg}");
        assert!(svg.contains(r#"<tspan x="6" y="83.5">Flow: 12 m3/h</tspan>"#), "{svg}");
        assert_eq!(svg.matches("transform=\"rotate(90 60 45)\"").count(), 6, "{svg}");
    }
}
