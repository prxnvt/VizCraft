//! What nets add to the canvas, drawn as the SVG export draws it (`vizcraft_formats::svg::nets`):
//! the marks repeated along marked lines (`line_marks::marks`) and the wire numbers or names
//! beside segments (`net::label_spots`), in the connector's own colours. Junction dots need
//! nothing here: a junction is an ordinary shape.
//!
//! Also the net highlight (chrome, in `theme::Tokens` colours): the net under the pointer, or the
//! one a click holds, drawn over every segment, pin and junction of it on the page, from
//! `Session.nets`.

use egui::epaint::TextShape;
use egui::{Painter, Stroke, vec2};
use vizcraft_diagram::line_marks::{LineMarker, marks};
use vizcraft_diagram::net::{NetIndex, label_spots};
use vizcraft_diagram::{Document, Page, route_polyline};

use super::text::{style_font, text_format};
use super::{color, flatten, line_px, pos, stroke_lines, tolerance};
use crate::canvas::Viewport2D;
use crate::theme::Tokens;

/// The line marks and net labels of `page`, a page of `doc` (or a drag's preview of one).
pub fn draw(painter: &Painter, vp: &Viewport2D, doc: &Document, page: &Page) {
    let visible = vp.visible_world_rect().inflate(40.0);
    for c in page.connectors.iter().filter(|c| c.marker != LineMarker::None) {
        let poly = route_polyline(page, c);
        if !vizcraft_diagram::Rect::bounding(poly.iter().copied()).is_some_and(|b| b.inflate(20.0).intersects(&visible)) {
            continue;
        }
        let path = marks(&poly, c.marker, c.style.line_width);
        if path.is_empty() {
            continue;
        }
        let stroke = Stroke::new(line_px(c.style.line_width.max(1.0), vp.zoom), color(c.style.effective_line()));
        stroke_lines(painter, vp, &flatten(&path, tolerance(vp.zoom)), stroke);
    }
    for spot in label_spots(doc, page) {
        if !visible.contains(spot.pos) {
            continue;
        }
        let Some(c) = page.connector(&spot.connector) else { continue };
        let st = &c.style;
        let Some(font) = style_font(st, vp.zoom) else { continue };
        let galley = painter.layout_job(egui::text::LayoutJob::single_section(spot.text.clone(), text_format(st, font)));
        // The spot is the text's centre; a turned label turns about it.
        let angle = (spot.angle as f32).to_radians();
        let centre = vp.world_to_screen(pos(spot.pos));
        let half = galley.size() / 2.0;
        let at = centre - egui::emath::Rot2::from_angle(angle) * vec2(half.x, half.y);
        let text = TextShape::new(at, galley, color(st.font_color));
        painter.add(if angle.abs() > 1e-4 { text.with_angle(angle) } else { text });
    }
}

/// The net to highlight: the one under the pointer, else the held one. Hover wins so that, with
/// a net held, pointing at another wire still shows that wire's net; the held net comes back
/// when the pointer leaves the wires.
pub fn highlighted<'a>(held: Option<&'a str>, hover: Option<&'a str>) -> Option<&'a str> {
    hover.or(held)
}

/// Highlight net `net` on `page`: a wide translucent band over each of its segments, a ring on
/// each pin and junction.
pub fn highlight(painter: &Painter, vp: &Viewport2D, nets: &NetIndex, page: &Page, net: &str, t: &Tokens) {
    let Some(entry) = nets.nets.get(net) else { return };
    let band = Stroke::new(6.0, t.accent.gamma_multiply(0.35));
    for c in entry.segments.iter().filter_map(|id| page.connector(id)) {
        let pts: Vec<egui::Pos2> = route_polyline(page, c).into_iter().map(|p| vp.world_to_screen(pos(p))).collect();
        if pts.len() > 1 {
            painter.add(egui::Shape::line(pts, band));
        }
    }
    let ring = Stroke::new(2.0, t.accent);
    for (shape, i) in &entry.pins {
        if let Some(p) = page.shape(shape).and_then(|s| s.connection_point_world(*i)) {
            painter.circle_stroke(vp.world_to_screen(pos(p)), 5.0, ring);
        }
    }
    for j in entry.junctions.iter().filter_map(|id| page.shape(id)) {
        painter.circle_stroke(vp.world_to_screen(pos(j.center())), 6.0, ring);
    }
}
