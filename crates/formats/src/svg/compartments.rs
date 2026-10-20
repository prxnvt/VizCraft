//! SVG export of a shape's compartments (see `vizcraft_diagram::compartment`), from the same
//! [`layout`] the canvas draws and hit testing reads.
//!
//! Inside the shape's `<g>`, after the outline and before the main text: each band's fill
//! override (inset by half the line width so the outline stays whole), the separator line at the
//! top of each band in the outline's stroke, then each band's title (bold, centred), its paragraph
//! lines or one line per list row, aligned left, centre or right with the band's padding. A
//! rotated shape turns the whole set about its centre. Rows past a squeezed band's bottom are
//! left out, as on the canvas.
//!
//! A table (a list with columns) draws a rule under its header row and between its columns,
//! the column titles bold, and each row's tab-separated cells left-aligned in their columns, cut
//! to the column's width (`fit_cell`: a per-character estimate, ending in an ellipsis when cut). A
//! table whose titles are all blank has no header row.

use std::fmt::Write as _;

use vizcraft_diagram::compartment::{Band, Compartment, CompartmentKind, LINE_HEIGHT, PAD_X, cells, fit_cell, layout};
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
        if let Some(h) = b.header {
            let (a, z) = (world(Point::new(h.x0, h.y1)), world(Point::new(h.x1, h.y1)));
            let _ =
                writeln!(out, "    <path d=\"M{} {} L{} {}\" fill=\"none\"{}{transform}/>", num(a.x), num(a.y), num(z.x), num(z.y), stroke(&s.style));
        }
        if !b.columns.is_empty() {
            // The column rules run the whole band, so no gap shows between a header and the rule above.
            for x in b.columns.iter().skip(1).take(b.columns.len().saturating_sub(2)) {
                let (a, z) = (world(Point::new(*x, b.rect.y0)), world(Point::new(*x, b.rect.y1)));
                let _ = writeln!(
                    out,
                    "    <path d=\"M{} {} L{} {}\" fill=\"none\"{}{transform}/>",
                    num(a.x),
                    num(a.y),
                    num(z.x),
                    num(z.y),
                    stroke(&s.style)
                );
            }
        }
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
            CompartmentKind::List if !b.columns.is_empty() => table_svg(out, s, comp, b, &st, &transform),
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

/// A table band's header titles and cells.
fn table_svg(out: &mut String, s: &Shape, comp: &Compartment, b: &Band, st: &Style, transform: &str) {
    let world = |p: Point| Point::new(p.x + s.x, p.y + s.y);
    let n = comp.columns.len();
    let spans: Vec<(f64, f64)> = b.columns.windows(2).map(|w| (w.first().copied().unwrap_or(0.0), w.get(1).copied().unwrap_or(0.0))).collect();
    let mut line = |y: f64, texts: Vec<&str>, style: &Style| {
        for ((x0, x1), t) in spans.iter().zip(texts) {
            let t = fit_cell(t, x1 - x0 - 2.0 * PAD_X, style.font_size);
            if t.trim().is_empty() {
                continue;
            }
            let w = world(Point::new(x0 + PAD_X, y));
            text_svg(out, &[t], w.x, w.y + style.font_size * 0.35, Anchor::Start, style, transform);
        }
    };
    if let Some(h) = b.header {
        let bold = Style { bold: true, ..st.clone() };
        line(h.center().y, comp.columns.iter().map(|c| c.title.as_str()).collect(), &bold);
    }
    for (row, r) in comp.rows.iter().zip(&b.rows) {
        line(r.center().y, cells(row, n), st);
    }
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::compartment::{Compartment, CompartmentKind};
    use vizcraft_diagram::{Color, Document, Shape, ShapeKind};

    use crate::svg::export_page;

    /// An OMG UML-style class: name, attributes, operations.
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
    fn golden_table_with_columns() {
        use vizcraft_diagram::compartment::Column;
        let mut doc = Document::new();
        let mut s = Shape::new("t", ShapeKind::Rectangle, 0.0, 0.0, 200.0, 70.0);
        s.text = "Parts".into();
        let mut rows = Compartment::new("rows", CompartmentKind::List);
        rows.columns = vec![Column { title: "ITEM".into(), width: 1.0 }, Column { title: "DESCRIPTION".into(), width: 3.0 }];
        rows.rows = vec!["1\tHousing".into(), "2\tA description far too long to fit its column".into()];
        s.compartments = vec![rows];
        if let Some(p) = doc.page_mut() {
            p.shapes.push(s);
        }
        doc.sanitize();
        let svg = String::from_utf8(export_page(&doc, 0).unwrap()).unwrap();
        let want = r##"  <g id="t">
    <path d="M0 0 L200 0 L200 70 L0 70 Z" fill="#ffffff" stroke="#3a4a5c" stroke-width="1.5"/>
    <path d="M0 22.4 L200 22.4" fill="none" stroke="#3a4a5c" stroke-width="1.5"/>
    <path d="M0 40.8 L200 40.8" fill="none" stroke="#3a4a5c" stroke-width="1.5"/>
    <path d="M50 22.4 L50 70" fill="none" stroke="#3a4a5c" stroke-width="1.5"/>
    <text text-anchor="start" font-family="Inter, sans-serif" font-size="12" font-weight="bold" fill="#1e2630" xml:space="preserve"><tspan x="6" y="37.8">ITEM</tspan></text>
    <text text-anchor="start" font-family="Inter, sans-serif" font-size="12" font-weight="bold" fill="#1e2630" xml:space="preserve"><tspan x="56" y="37.8">DESCRIPTION</tspan></text>
    <text text-anchor="start" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="6" y="52.2">1</tspan></text>
    <text text-anchor="start" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="56" y="52.2">Housing</tspan></text>
    <text text-anchor="start" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="6" y="66.6">2</tspan></text>
    <text text-anchor="start" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="56" y="66.6">A description far too lon…</tspan></text>
    <text text-anchor="middle" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="100" y="15.4">Parts</tspan></text>
  </g>
</svg>"##;
        assert_eq!(body(&svg), want);
    }

    #[test]
    fn golden_headerless_table() {
        use vizcraft_diagram::compartment::Column;
        let mut doc = Document::new();
        let mut s = Shape::new("t", ShapeKind::Rectangle, 0.0, 0.0, 200.0, 70.0);
        s.text = "Parts".into();
        let mut rows = Compartment::new("rows", CompartmentKind::List);
        rows.columns = vec![Column { title: String::new(), width: 1.0 }, Column { title: " ".into(), width: 3.0 }];
        rows.rows = vec!["1\tHousing".into(), "2\tvarchar(255)".into()];
        s.compartments = vec![rows];
        if let Some(p) = doc.page_mut() {
            p.shapes.push(s);
        }
        doc.sanitize();
        let svg = String::from_utf8(export_page(&doc, 0).unwrap()).unwrap();
        let b = body(&svg);
        // One rule at the band's top, one column rule over the whole band, no rule under a header.
        assert_eq!(b.matches("<path d=\"M").count(), 3, "{b}");
        assert!(b.contains("L50 70\"") && !b.contains("font-weight=\"bold\""), "{b}");
        assert!(b.contains(">varchar(255)</tspan>"), "{b}");
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
