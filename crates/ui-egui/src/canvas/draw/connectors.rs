//! Drawing connectors: the routed line (dash pattern, line cap, transparency and rounded bends,
//! as SVG export draws them), arrowheads, the label and the labels beside each end.

use egui::epaint::TextShape;
use egui::{Color32, Painter, Stroke};
use vizcraft_diagram::connector_look::LabelFrame;
use vizcraft_diagram::{Arrowhead, Connector, ConnectorGeometry, Style};

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
    if c.halves.is_plain() {
        let line = g.line.rounded_corners(c.style.corner_radius);
        outline(painter, vp, &c.style, &flatten(&line, tolerance(vp.zoom)), colour);
    } else {
        halves(painter, vp, c, g, colour);
    }
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
        if c.label_frame == LabelFrame::Box {
            let bg = galley.rect.translate(at.to_vec2()).expand(2.0 * vp.zoom);
            painter.rect_filled(bg, 2.0 * vp.zoom, PAGE_FILL);
        } else {
            label_frame(painter, vp, c, g, colour);
        }
        painter.add(TextShape::new(at, galley, color(st.font_color)));
    }
    end_labels(painter, vp, c, g);
    step_badge(painter, vp, c, g);
}

/// A line drawn half by half (`Connector.halves`): each half with its own dash, and as two
/// parallel lines where it is double, as the SVG export draws it.
fn halves(painter: &Painter, vp: &Viewport2D, c: &Connector, g: &ConnectorGeometry, colour: Color32) {
    let st = &c.style;
    let pts: Vec<vizcraft_diagram::Point> = g.line.flatten(0.5).into_iter().flat_map(|p| p.points).collect();
    let (a, b) = vizcraft_diagram::connector_look::split_half(&pts);
    for (half, look) in [(a, c.halves.start), (b, c.halves.end)] {
        let hs = Style { dash: look.dash.unwrap_or(st.dash), ..st.clone() };
        let lines = if look.double {
            let (l, r) = vizcraft_diagram::connector_look::parallel(&half, st.line_width.max(1.0) * 1.5);
            vec![l, r]
        } else {
            vec![half]
        };
        let lines: Vec<(Vec<egui::Pos2>, bool)> = lines.into_iter().filter(|l| l.len() >= 2).map(|l| (l.into_iter().map(pos).collect(), false)).collect();
        outline(painter, vp, &hs, &lines, colour);
    }
}

/// The diamond (or double diamond) around a label, white inside, in a solid line.
fn label_frame(painter: &Painter, vp: &Viewport2D, c: &Connector, g: &ConnectorGeometry, colour: Color32) {
    let (w, h) = vizcraft_diagram::connector_look::label_size(&c.text, &c.style);
    let frame = vizcraft_diagram::connector_look::label_frame_path(g.label_pos, w, h, c.label_frame);
    let rings = flatten(&frame, tolerance(vp.zoom));
    // The first ring is the outer diamond; it is convex, so it fills as one polygon.
    if let Some((outer, _)) = rings.first() {
        let tris = vec![triangulate(outer)];
        fill_rings(painter, vp, &rings[..1], &tris, PAGE_FILL);
    }
    let solid = Style { dash: vizcraft_diagram::Dash::Solid, ..c.style.clone() };
    outline(painter, vp, &solid, &rings, colour);
}

/// The numbered step badge: a filled circle in the line colour with a white outline and a bold
/// white number, placed by `step_badge::badge` as the SVG export places it.
fn step_badge(painter: &Painter, vp: &Viewport2D, c: &Connector, g: &ConnectorGeometry) {
    let Some(b) = vizcraft_diagram::step_badge::badge(c, &g.polyline) else { return };
    let centre = vp.world_to_screen(pos(b.center));
    let r = (b.radius as f32 * vp.zoom).max(1.0);
    painter.circle(centre, r, color(c.style.effective_line()), Stroke::new(1.0, Color32::WHITE));
    let st = Style { font_size: (b.radius * 1.1).clamp(6.0, 60.0), font_color: vizcraft_diagram::Color::WHITE, bold: true, ..Style::DEFAULT };
    let Some(font) = style_font(&st, vp.zoom) else { return };
    let galley = painter.layout_no_wrap(b.text, font, Color32::WHITE);
    painter.galley(centre - galley.size() / 2.0, galley, Color32::WHITE);
}

/// The labels beside each end, a little outside the line past the head, on a white box like
/// the main label (as SVG export places them).
fn end_labels(painter: &Painter, vp: &Viewport2D, c: &Connector, g: &ConnectorGeometry) {
    let st = &c.style;
    for (text, at_start) in [(&c.start_label, true), (&c.end_label, false)] {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let Some(p) = vizcraft_diagram::arrows::end_label_anchor(&g.polyline, at_start, st.font_size, st.line_width) else { continue };
        let Some(font) = style_font(st, vp.zoom) else { return };
        let galley = label_layout(painter, text, st, font, 200.0 * vp.zoom);
        let centre = vp.world_to_screen(pos(p));
        let at = centre - galley.rect.center().to_vec2();
        let bg = galley.rect.translate(at.to_vec2()).expand(2.0 * vp.zoom);
        painter.rect_filled(bg, 2.0 * vp.zoom, PAGE_FILL);
        painter.add(TextShape::new(at, galley, color(st.font_color)));
    }
}
