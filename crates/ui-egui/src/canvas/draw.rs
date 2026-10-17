//! Painting the page: the sheet, shapes (fill, outline, detail lines, text) and connectors (line,
//! arrowheads, label). Document colours come from the document; nothing here uses chrome
//! colours except the pasteboard around the sheet, which the caller fills.

use std::collections::HashMap;
use std::sync::Arc;

use egui::epaint::{Mesh, PathShape, TextShape};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Align2, Color32, Painter, Pos2, Rect, Shape as EShape, Stroke, pos2};
use vizcraft_diagram::{Arrowhead, Color, Connector, ConnectorGeometry, Page, Path, Point, Shape};

use super::Viewport2D;
use super::tess::triangulate;
use crate::theme::{Tokens, regular, semibold};

/// The page sheet. The page colour is not in the model yet; SVG export draws the same white, so
/// the canvas matches what an export looks like in both themes.
pub const PAGE_FILL: Color32 = Color32::WHITE;

/// Smallest on-screen font size worth laying out; below it text is unreadable and layout cost
/// dominates when zoomed far out.
const MIN_TEXT_PX: f32 = 3.0;

/// Inner padding between a shape's frame and its text, in world units.
pub const TEXT_PAD: f64 = 6.0;

/// A model colour as an egui colour.
pub fn color(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// A model point as an egui position (world space, f32).
pub fn pos(p: Point) -> Pos2 {
    pos2(p.x as f32, p.y as f32)
}

/// An egui position as a model point.
pub fn point(p: Pos2) -> Point {
    Point::new(f64::from(p.x), f64::from(p.y))
}

/// The world rectangle of a model rect, as an egui rect.
pub fn rect(r: vizcraft_diagram::Rect) -> Rect {
    Rect::from_min_max(pos2(r.x0 as f32, r.y0 as f32), pos2(r.x1 as f32, r.y1 as f32))
}

/// Flattening tolerance in world units for this zoom: about a third of a pixel, rounded to a
/// power of two so cached fills survive small zoom changes.
fn tolerance(zoom: f32) -> f64 {
    let t = (0.35 / f64::from(zoom.max(0.01))).clamp(0.01, 50.0);
    2f64.powf(t.log2().floor())
}

/// A shape's outline flattened and triangulated, in world space.
struct Fill {
    key: [u64; 7],
    rings: Vec<(Vec<Pos2>, bool)>,
    tris: Vec<Vec<[u32; 3]>>,
}

/// Fills and outlines by shape id, recomputed only when the shape's geometry or the flattening
/// tolerance changes, so a few hundred shapes stay cheap to draw every frame.
#[derive(Default)]
pub struct FillCache {
    map: HashMap<String, Fill>,
    frame_seen: Vec<String>,
}

impl FillCache {
    fn key(s: &Shape, tol: f64) -> [u64; 7] {
        let kind = s.kind as u64;
        [s.x.to_bits(), s.y.to_bits(), s.w.to_bits(), s.h.to_bits(), s.angle_deg.to_bits(), tol.to_bits(), kind]
    }

    fn get(&mut self, s: &Shape, tol: f64) -> &Fill {
        let key = Self::key(s, tol);
        self.frame_seen.push(s.id.clone());
        let fresh = self.map.get(&s.id).is_some_and(|f| f.key == key);
        if !fresh {
            let lines = s.outline_world().flatten(tol);
            let rings: Vec<(Vec<Pos2>, bool)> = lines.into_iter().map(|pl| (pl.points.into_iter().map(pos).collect(), pl.closed)).collect();
            let tris = rings.iter().map(|(pts, closed)| if *closed { triangulate(pts) } else { Vec::new() }).collect();
            self.map.insert(s.id.clone(), Fill { key, rings, tris });
        }
        // Just inserted or already fresh; an empty fill keeps the borrow simple if neither held.
        self.map.entry(s.id.clone()).or_insert(Fill { key, rings: Vec::new(), tris: Vec::new() })
    }

    /// Forget shapes not drawn this frame (deleted, or another document).
    pub fn end_frame(&mut self) {
        let seen = std::mem::take(&mut self.frame_seen);
        if self.map.len() > seen.len() {
            self.map.retain(|k, _| seen.contains(k));
        }
    }
}

/// The page sheet: shadow, white fill and a subtle border.
pub fn sheet(painter: &Painter, vp: &Viewport2D, page: &Page, t: &Tokens) {
    let r = Rect::from_min_max(vp.world_to_screen(pos2(0.0, 0.0)), vp.world_to_screen(pos2(page.width as f32, page.height as f32)));
    let shadow = egui::Shadow { offset: [0, 2], blur: 10, spread: 0, color: t.shadow };
    painter.add(shadow.as_shape(r, 0));
    painter.rect(r, 0.0, PAGE_FILL, Stroke::new(1.0, t.border_strong), egui::StrokeKind::Outside);
}

fn to_screen(vp: &Viewport2D, pts: &[Pos2]) -> Vec<Pos2> {
    pts.iter().map(|p| vp.world_to_screen(*p)).collect()
}

/// Stroke width on screen for a model line width; hairlines stay visible.
fn line_px(width: f64, zoom: f32) -> f32 {
    ((width.clamp(0.0, 100.0) as f32) * zoom).max(if width > 0.0 { 0.75 } else { 0.0 })
}

/// Fill polygons as meshes, `tris` indexing into `rings`.
fn fill_rings(painter: &Painter, vp: &Viewport2D, rings: &[(Vec<Pos2>, bool)], tris: &[Vec<[u32; 3]>], fill: Color32) {
    if fill.a() == 0 {
        return;
    }
    for ((pts, _), tri) in rings.iter().zip(tris) {
        if tri.is_empty() {
            continue;
        }
        let mut mesh = Mesh::default();
        mesh.reserve_vertices(pts.len());
        mesh.reserve_triangles(tri.len());
        for p in pts {
            mesh.colored_vertex(vp.world_to_screen(*p), fill);
        }
        for [a, b, c] in tri {
            mesh.add_triangle(*a, *b, *c);
        }
        painter.add(EShape::mesh(mesh));
    }
}

fn stroke_lines(painter: &Painter, vp: &Viewport2D, lines: &[(Vec<Pos2>, bool)], stroke: Stroke) {
    if stroke.width <= 0.0 || stroke.color.a() == 0 {
        return;
    }
    for (pts, closed) in lines {
        let s = to_screen(vp, pts);
        painter.add(if *closed { PathShape::closed_line(s, stroke) } else { PathShape::line(s, stroke) });
    }
}

fn flatten(path: &Path, tol: f64) -> Vec<(Vec<Pos2>, bool)> {
    path.flatten(tol).into_iter().map(|pl| (pl.points.into_iter().map(pos).collect(), pl.closed)).collect()
}

/// Draw one shape. `show_text` is false while the text is being edited in place.
pub fn shape(painter: &Painter, vp: &Viewport2D, s: &Shape, cache: &mut FillCache, show_text: bool) {
    let tol = tolerance(vp.zoom);
    let stroke = Stroke::new(line_px(s.style.line_width, vp.zoom), color(s.style.line));
    let fill = cache.get(s, tol);
    fill_rings(painter, vp, &fill.rings, &fill.tris, color(s.style.fill));
    stroke_lines(painter, vp, &fill.rings, stroke);
    if let Some(detail) = s.detail_world() {
        stroke_lines(painter, vp, &flatten(&detail, tol), stroke);
    }
    if show_text && !s.text.trim().is_empty() {
        shape_text(painter, vp, s);
    }
}

/// The font for a style at this zoom, or `None` when too small to read.
pub fn font_for(size: f64, bold: bool, zoom: f32) -> Option<egui::FontId> {
    let size = if size.is_finite() { size.clamp(1.0, 400.0) } else { 12.0 };
    let px = (size as f32) * zoom;
    // Half-point steps keep the glyph atlas from filling with one size per zoom step.
    let px = (px * 2.0).round() / 2.0;
    if px < MIN_TEXT_PX {
        return None;
    }
    Some(if bold { semibold(px) } else { regular(px) })
}

/// Lay out text centred, wrapped to `wrap` screen points.
fn layout(painter: &Painter, text: &str, font: egui::FontId, colour: Color32, italic: bool, wrap: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat { font_id: font, color: colour, italics: italic, ..Default::default() });
    job.wrap.max_width = wrap.max(1.0);
    job.halign = Align::Center;
    painter.layout_job(job)
}

/// Shape text: centred in the frame, wrapped to its width, rotated with the shape.
fn shape_text(painter: &Painter, vp: &Viewport2D, s: &Shape) {
    let st = &s.style;
    let Some(font) = font_for(st.font_size, st.bold, vp.zoom) else { return };
    let wrap = ((s.w - 2.0 * TEXT_PAD).max(1.0) as f32) * vp.zoom;
    let galley = layout(painter, &s.text, font, color(st.font_color), st.italic, wrap);
    let centre = vp.world_to_screen(pos(s.center()));
    let at = centre - galley.rect.center().to_vec2();
    let angle = (s.angle_deg as f32).to_radians();
    let text = TextShape::new(at, galley, color(st.font_color));
    painter.add(if angle.abs() > 1e-4 { text.with_angle_and_anchor(angle, Align2::CENTER_CENTER) } else { text });
}

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
    let colour = color(c.style.line);
    let width = line_px(c.style.line_width, vp.zoom);
    stroke_lines(painter, vp, &flatten(&g.line, tolerance(vp.zoom)), Stroke::new(width, colour));
    for h in [&g.start_head, &g.end_head].into_iter().flatten() {
        head(painter, vp, h, colour, width);
    }
    if show_text && !c.text.trim().is_empty() {
        let st = &c.style;
        let Some(font) = font_for(st.font_size, st.bold, vp.zoom) else { return };
        let galley = layout(painter, &c.text, font, color(st.font_color), st.italic, 200.0 * vp.zoom);
        let centre = vp.world_to_screen(pos(g.label_pos));
        let at = centre - galley.rect.center().to_vec2();
        let bg = galley.rect.translate(at.to_vec2()).expand(2.0 * vp.zoom);
        painter.rect_filled(bg, 2.0 * vp.zoom, PAGE_FILL);
        painter.add(TextShape::new(at, galley, color(st.font_color)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerance_steps_in_powers_of_two() {
        assert_eq!(tolerance(1.0), 0.25);
        assert_eq!(tolerance(1.1), tolerance(1.2));
        assert!(tolerance(10.0) < tolerance(1.0));
        assert!(tolerance(f32::NAN).is_finite());
    }

    #[test]
    fn fonts_vanish_when_tiny_and_snap_to_half_points() {
        assert!(font_for(12.0, false, 0.1).is_none());
        assert_eq!(font_for(12.0, false, 1.04).map(|f| f.size), Some(12.5));
        assert!(font_for(f64::NAN, true, 1.0).is_some());
    }
}
