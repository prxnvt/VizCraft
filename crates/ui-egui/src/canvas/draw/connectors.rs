//! Drawing connectors: the routed line (dash pattern, line cap, transparency and rounded bends,
//! as SVG export draws them), arrowheads and the label.

use egui::epaint::TextShape;
use egui::{Color32, Painter, Stroke};
use vizcraft_diagram::{Arrowhead, Connector, ConnectorGeometry};

use super::shapes::outline;
use super::text::{label_layout, style_font};
use super::{PAGE_FILL, color, fill_rings, flatten, line_px, pos, stroke_lines, tolerance};
use crate::canvas::Viewport2D;
use crate::canvas::tess::triangulate;

/// Screen-space bounds of a connector's route, for culling.
pub fn connector_bounds(g: &ConnectorGeometry) -> Option<vizcraft_diagram::Rect> {
    vizcraft_diagram::Rect::bounding(g.polyline.iter().copied()).map(|r| r.inflate(20.0))
}

fn head(painter: &Painter, vp: &Viewport2D, h: &Arrowhead, colour: Color32, width: f32) {
    let rings = flatten(&h.path, tolerance(vp.zoom));
    if h.filled {
        let tris: Vec<Vec<[u32; 3]>> = rings.iter().map(|(p, _)| triangulate(p)).collect();
        fill_rings(painter, vp, &rings, &tris, colour);
        stroke_lines(painter, vp, &rings, Stroke::new(width.min(1.0), colour));
    } else {
        stroke_lines(painter, vp, &rings, Stroke::new(width, colour));
    }
}

/// Draw one connector from its geometry. `show_text` is false while its label is edited.
pub fn connector(painter: &Painter, vp: &Viewport2D, c: &Connector, g: &ConnectorGeometry, show_text: bool) {
    let colour = color(c.style.effective_line());
    let line = g.line.rounded_corners(c.style.corner_radius);
    outline(painter, vp, &c.style, &flatten(&line, tolerance(vp.zoom)), colour);
    // Heads keep a visible outline even on a zero-width line (as in SVG export), and are never
    // dashed.
    let width = line_px(c.style.line_width.max(1.0), vp.zoom);
    for h in [&g.start_head, &g.end_head].into_iter().flatten() {
        head(painter, vp, h, colour, width);
    }
    if show_text && !c.text.trim().is_empty() {
        let st = &c.style;
        let Some(font) = style_font(st, vp.zoom) else { return };
        let galley = label_layout(painter, &c.text, st, font, 200.0 * vp.zoom);
        let centre = vp.world_to_screen(pos(g.label_pos));
        let at = centre - galley.rect.center().to_vec2();
        let bg = galley.rect.translate(at.to_vec2()).expand(2.0 * vp.zoom);
        painter.rect_filled(bg, 2.0 * vp.zoom, PAGE_FILL);
        painter.add(TextShape::new(at, galley, color(st.font_color)));
    }
}
