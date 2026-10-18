//! SVG export of data graphics (see `vizcraft_diagram::data_graphic`), from the same
//! [`render`] the canvas draws.
//!
//! A shape with a data graphic is written with its colour by value applied to a copy (the
//! document is not changed), then its items follow in a `<g class="data-graphic">` right after
//! the shape's own group, so they sit above it and below the shapes drawn later, as on the
//! canvas.

use std::fmt::Write as _;

use vizcraft_diagram::data_graphic::{Mark, recolored, render};
use vizcraft_diagram::{Document, Page, Point, Shape};

use super::{Ctx, FONT_FAMILY, escape, num, paint};

/// Write `s` with its data graphic, if it shows one.
pub(super) fn shape_svg(out: &mut String, cx: &mut Ctx, doc: &Document, page: &Page, s: &Shape) {
    let Some(r) = render(doc, page, s) else {
        super::shape_svg(out, cx, s);
        return;
    };
    super::shape_svg(out, cx, &recolored(s, &r));
    let marks: Vec<&Mark> = r.items.iter().flat_map(|i| &i.marks).collect();
    if marks.is_empty() {
        return;
    }
    let _ = writeln!(out, "  <g class=\"data-graphic\" data-shape=\"{}\">", escape(&s.id));
    for m in marks {
        mark_svg(out, m);
    }
    out.push_str("  </g>\n");
}

fn points(pts: &[Point]) -> String {
    pts.iter().map(|p| format!("{},{}", num(p.x), num(p.y))).collect::<Vec<_>>().join(" ")
}

fn mark_svg(out: &mut String, m: &Mark) {
    let _ = match m {
        Mark::Rect { rect, fill, line, width } => {
            let fill = fill.map_or_else(|| " fill=\"none\"".to_string(), |c| paint("fill", c));
            let line = match line {
                Some(c) if *width > 0.0 => format!("{} stroke-width=\"{}\"", paint("stroke", *c), num(*width)),
                _ => String::new(),
            };
            writeln!(
                out,
                "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{fill}{line}/>",
                num(rect.x0),
                num(rect.y0),
                num(rect.width()),
                num(rect.height())
            )
        }
        Mark::Circle { center, radius, fill } => {
            writeln!(out, "    <circle cx=\"{}\" cy=\"{}\" r=\"{}\"{}/>", num(center.x), num(center.y), num(*radius), paint("fill", *fill))
        }
        Mark::Polygon { points: pts, fill } => writeln!(out, "    <polygon points=\"{}\"{}/>", points(pts), paint("fill", *fill)),
        Mark::Polyline { points: pts, line, width } => writeln!(
            out,
            "    <polyline points=\"{}\" fill=\"none\"{} stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
            points(pts),
            paint("stroke", *line),
            num(*width)
        ),
        Mark::Text { rect, text, size, color } => {
            let c = rect.center();
            writeln!(
                out,
                "    <text x=\"{}\" y=\"{}\" font-family=\"{FONT_FAMILY}\" font-size=\"{}\" text-anchor=\"middle\"{}>{}</text>",
                num(c.x),
                num(c.y + size * 0.35),
                num(*size),
                paint("fill", *color),
                escape(text)
            )
        }
    };
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::data::{DataRow, DataType};
    use vizcraft_diagram::data_graphic::{DataGraphic, DataItem, ItemKind, Op, Position, Rule, layout};
    use vizcraft_diagram::{Color, ShapeKind};

    use super::*;

    /// A fixed page with one shape holding `cost = 75`, showing a graphic with one item of each
    /// kind: a callout below, a bar on the right, an icon in the top right corner and a colour
    /// by value.
    pub(crate) fn four_kinds() -> Document {
        let mut doc = Document::new();
        let mut s = Shape::new("s1", ShapeKind::Rectangle, 40.0, 40.0, 120.0, 60.0);
        s.text = "Server".into();
        s.data.rows.push(DataRow { key: "cost".into(), kind: DataType::Number, value: "75".into(), ..DataRow::default() });
        s.data_graphic = Some("g1".into());
        let items = vec![
            DataItem { kind: ItemKind::Text, key: "cost".into(), label: "Cost".into(), format: "$#,##0.00".into(), ..DataItem::default() },
            DataItem {
                kind: ItemKind::Bar,
                key: "cost".into(),
                position: Position::Right,
                color: Color::rgb(0x16, 0xa3, 0x4a),
                ..DataItem::default()
            },
            DataItem {
                kind: ItemKind::Icons,
                key: "cost".into(),
                position: Position::TopRight,
                rules: vec![
                    Rule { op: Op::Gt, value: "50".into(), icon: "check".into(), ..Rule::default() },
                    Rule { op: Op::Any, icon: "close".into(), ..Rule::default() },
                ],
                ..DataItem::default()
            },
            DataItem {
                kind: ItemKind::Color,
                key: "cost".into(),
                rules: vec![Rule {
                    op: Op::Between,
                    value: "50".into(),
                    to: "100".into(),
                    fill: Some(Color::rgb(0xfe, 0xf3, 0xc7)),
                    ..Rule::default()
                }],
                ..DataItem::default()
            },
        ];
        doc.data_graphics.push(DataGraphic { id: "g1".into(), name: "Cost".into(), items });
        if let Some(p) = doc.page_mut() {
            p.board = vizcraft_diagram::Board::Fixed;
            p.width = 300.0;
            p.height = 160.0;
            p.shapes.push(s);
        }
        doc
    }

    #[test]
    fn golden_four_kinds_at_the_layout_rectangles() {
        let doc = four_kinds();
        let svg = String::from_utf8(super::super::export_page(&doc, 0).unwrap()).unwrap();
        let body: String = svg.lines().skip(4).collect::<Vec<_>>().join("\n");
        let want = r##"  <g id="s1">
    <path d="M40 40 L160 40 L160 100 L40 100 Z" fill="#fef3c7" stroke="#3a4a5c" stroke-width="1.5"/>
    <text text-anchor="middle" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="100" y="74.2">Server</tspan></text>
  </g>
  <g class="data-graphic" data-shape="s1">
    <rect x="61.2" y="104" width="77.6" height="16" fill="#ffffff" stroke="#9aa5b1" stroke-width="0.75"/>
    <text x="100" y="115.5" font-family="Inter, sans-serif" font-size="10" text-anchor="middle" fill="#1f2933">Cost: $75.00</text>
    <rect x="164" y="65" width="60" height="10" fill="#e4e7eb"/>
    <rect x="164" y="65" width="45" height="10" fill="#16a34a"/>
    <rect x="164" y="65" width="60" height="10" fill="none" stroke="#9aa5b1" stroke-width="0.75"/>
    <circle cx="150" cy="50" r="8" fill="#16a34a"/>
    <polyline points="146.5,50.5 149,53 153.5,47.5" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
  </g>
</svg>"##;
        assert_eq!(body, want, "{body}");
        // Every drawn item's box is the one the layout function gives.
        let page = doc.page().unwrap();
        let s = page.shape("s1").unwrap();
        let rects = layout(&doc, page, s, &doc.data_graphics[0]);
        assert_eq!(rects.len(), 4);
        for (_, kind, r) in rects.iter().filter(|(_, k, _)| matches!(k, ItemKind::Text | ItemKind::Bar)) {
            let tag = format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"", num(r.x0), num(r.y0), num(r.width()), num(r.height()));
            assert!(svg.contains(&tag), "{kind:?} at {tag}");
        }
        let (_, _, icon) = rects.iter().find(|(_, k, _)| *k == ItemKind::Icons).unwrap();
        assert!(svg.contains(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\"", num(icon.center().x), num(icon.center().y), num(icon.width() / 2.0))));
        assert_eq!(doc.page().unwrap().shape("s1").unwrap().style.fill, Shape::default().style.fill, "the export changes no style");
    }
}
