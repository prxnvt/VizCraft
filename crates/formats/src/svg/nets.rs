//! SVG export of what nets add to a page: the marks repeated along marked lines
//! (`vizcraft_diagram::line_marks`) and the wire numbers or names written beside segments
//! (`vizcraft_diagram::net::label_spots`). Junction dots need nothing here: a junction is an
//! ordinary shape. PDF follows, since it is built from the SVG.
//!
//! Drawn after the connectors in one `<g class="nets">`, left out entirely when the page has
//! neither, so pages without wiring export exactly as before.

use std::fmt::Write as _;

use vizcraft_diagram::line_marks::{LineMarker, marks};
use vizcraft_diagram::net::label_spots;
use vizcraft_diagram::{Document, Page, route_polyline};

use super::{Anchor, num, paint, path_data, text_svg};

/// The marks and labels of `page`, a page of `doc`.
pub(super) fn draw(out: &mut String, doc: &Document, page: &Page) {
    let mut body = String::new();
    for c in page.connectors.iter().filter(|c| c.marker != LineMarker::None) {
        let path = marks(&route_polyline(page, c), c.marker, c.style.line_width);
        if path.is_empty() {
            continue;
        }
        let _ = writeln!(
            body,
            "    <path data-connector=\"{}\" d=\"{}\" fill=\"none\"{} stroke-width=\"{}\" stroke-linecap=\"round\"/>",
            super::escape(&c.id),
            path_data(&path),
            paint("stroke", c.style.effective_line()),
            num(c.style.line_width.max(1.0))
        );
    }
    for spot in label_spots(doc, page) {
        let Some(c) = page.connector(&spot.connector) else { continue };
        let st = &c.style;
        let transform = if spot.angle == 0.0 {
            String::new()
        } else {
            format!(" transform=\"rotate({} {} {})\"", num(spot.angle), num(spot.pos.x), num(spot.pos.y))
        };
        text_svg(&mut body, std::slice::from_ref(&spot.text), spot.pos.x, spot.pos.y + st.font_size * 0.35, Anchor::Middle, st, &transform);
    }
    if !body.is_empty() {
        out.push_str("  <g class=\"nets\">\n");
        out.push_str(&body);
        out.push_str("  </g>\n");
    }
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::line_marks::LineMarker;
    use vizcraft_diagram::net::{junction, split_at};
    use vizcraft_diagram::vizcraft_model::{Net, NetKind};
    use vizcraft_diagram::{Arrow, Color, Connector, Document, End, Point, Route, Shape, ShapeKind};

    use crate::svg::export_page;

    fn wire(id: &str, a: (f64, f64), b: (f64, f64), net: &str) -> Connector {
        Connector {
            route: Route::Orthogonal,
            end_arrow: Arrow::None,
            net: Some(net.into()),
            ..Connector::new(id, End::free(Point::new(a.0, a.1)), End::free(Point::new(b.0, b.1)))
        }
    }

    fn svg(doc: &Document) -> String {
        String::from_utf8(export_page(doc, 0).unwrap()).unwrap()
    }

    fn nets_group(svg: &str) -> String {
        svg.split("<g class=\"nets\">\n").nth(1).and_then(|r| r.split("  </g>\n").next()).unwrap_or_default().to_string()
    }

    /// A page with a numbered vertical wire, a tee junction dot, and two marked lines.
    fn wired() -> Document {
        let mut doc = Document::new();
        doc.model.nets.push(Net { number: "2003".into(), ..Net::new("n1", NetKind::Wire) });
        doc.model.nets.push(Net::new("n2", NetKind::Signal));
        let page = &mut doc.pages[0];
        page.board = vizcraft_diagram::Board::Fixed;
        page.connectors.push(wire("v", (100.0, 100.0), (100.0, 300.0), "n1"));
        split_at(page, "v", Point::new(100.0, 200.0), "v2", "j1", true).unwrap();
        page.connectors.push(Connector { end: End::point("j1", 0), ..wire("b", (40.0, 200.0), (0.0, 0.0), "n1") });
        page.connectors.push(Connector { marker: LineMarker::DoubleSlash, ..wire("pn", (200.0, 100.0), (296.0, 100.0), "n2") });
        page.connectors.push(Connector { marker: LineMarker::Cross, ..wire("cap", (200.0, 200.0), (248.0, 200.0), "n2") });
        doc
    }

    #[test]
    fn golden_junction_dot_wire_number_and_markers() {
        let doc = wired();
        let out = svg(&doc);
        // The junction is an ordinary filled circle in the line colour, 3 line widths across.
        let dot = out.split("<g id=\"j1\">\n").nth(1).unwrap().lines().next().unwrap();
        let line = vizcraft_diagram::Style::DEFAULT.line.to_hex();
        assert!(dot.ends_with(&format!("fill=\"{line}\" stroke=\"none\"/>")), "{dot}");
        assert!(dot.starts_with("    <path d=\"M102.25 200 C"), "a 4.5 wide dot centred on the tee: {dot}");
        let nets = nets_group(&out);
        // Double slash on a 96 long line at width 1.5: a gap of 18, five marks of two strokes.
        let pn = nets.lines().find(|l| l.contains("data-connector=\"pn\"")).unwrap();
        assert_eq!(pn.matches('M').count(), 10, "{pn}");
        let cap = nets.lines().find(|l| l.contains("data-connector=\"cap\"")).unwrap();
        assert_eq!(cap.matches('M').count(), 4, "two crosses on 48 units: {cap}");
        // The number goes once, on the longest segment, read from the bottom beside the line.
        assert_eq!(nets.matches(">2003</tspan>").count(), 1, "{nets}");
        assert!(nets.contains("transform=\"rotate(-90 "), "{nets}");
        let golden = "<text text-anchor=\"middle\" font-family=\"Inter, sans-serif\" font-size=\"12\" fill=\"#1e2630\" transform=\"rotate(-90 90.85 150)\" xml:space=\"preserve\"><tspan x=\"90.85\" y=\"154.2\">2003</tspan></text>";
        assert!(nets.contains(golden), "{nets}");
    }

    #[test]
    fn pages_without_wiring_have_no_nets_group_and_off_labels_are_left_out() {
        let mut doc = Document::new();
        doc.pages[0].connectors.push(Connector::new("c", End::free(Point::new(0.0, 0.0)), End::free(Point::new(100.0, 0.0))));
        assert!(!svg(&doc).contains("class=\"nets\""));
        let mut doc = wired();
        doc.model.nets[0].label = vizcraft_diagram::vizcraft_model::NetLabel::Off;
        assert!(!nets_group(&svg(&doc)).contains("2003"));
    }

    #[test]
    fn golden_upright_label_on_a_turned_shape() {
        let mut doc = Document::new();
        let mut s = Shape::new("r", ShapeKind::Rectangle, 100.0, 100.0, 80.0, 40.0);
        s.text = "R1".into();
        s.text_block = Some(vizcraft_diagram::TextBlock { dx: 0.0, dy: -30.0, w: 40.0, h: 20.0, angle_deg: 0.0 });
        s.orient = vizcraft_diagram::Orient { turns: 1, mirror: false };
        doc.pages[0].shapes.push(s);
        doc.pages[0].shapes.push(junction("j", Point::new(0.0, 0.0), false, 1.0, Color::BLACK));
        let out = svg(&doc);
        let text = out.lines().find(|l| l.contains(">R1</tspan>")).unwrap();
        // The block above the frame turns to the frame's right (centre 140, 120): text upright,
        // no rotate in the transform.
        assert!(!text.contains("rotate("), "{text}");
        assert!(text.contains("<tspan x=\"170\" y=\""), "the block centre moved right of the turned body: {text}");
    }
}
