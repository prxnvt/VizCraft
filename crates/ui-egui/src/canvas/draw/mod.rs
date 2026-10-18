//! Painting the page: the sheet, shapes (fill, outline, detail lines, text) and connectors (line,
//! arrowheads, label). Document colours come from the document; nothing here uses chrome
//! colours except the pasteboard around the sheet, which the caller fills.
//!
//! Layout: this module holds the model-to-egui conversions, the sheet and the fill and stroke
//! helpers; [`shapes`] draws shapes, [`text`] lays out text, [`connectors`] draws connectors and
//! [`overlays`] draws the selection chrome on top.

pub mod connectors;
pub mod overlays;
pub mod shapes;
pub mod text;

use egui::epaint::{Mesh, PathShape};
use egui::{Color32, Painter, Pos2, Rect, Shape as EShape, Stroke, pos2};
use vizcraft_diagram::{Color, Page, Path, Point};

use super::Viewport2D;
use crate::theme::Tokens;
pub use connectors::{connector, connector_bounds};
pub use shapes::{FillCache, shape};
pub use text::{TEXT_PAD, font_for};

/// The page sheet. The page colour is not in the model yet; SVG export draws the same white, so
/// the canvas matches what an export looks like in both themes.
pub const PAGE_FILL: Color32 = Color32::WHITE;

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
}
