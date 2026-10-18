//! Drawing shapes: cached fills and outlines, effects, detail lines and the shape's text.
//!
//! The order and the approximations follow SVG export (`vizcraft_formats::svg`) so the canvas
//! looks like an exported page: glow, then shadow, then the fill (solid or gradient, with soft
//! edges), then the outline (dash pattern, line cap) and detail lines, then the text. Glow,
//! shadow blur and soft edges are drawn as strips that fade from the outline outwards, standing in
//! for SVG's Gaussian blur. One difference: a gradient here turns with a rotated shape (it is laid
//! out in the shape's own frame), where SVG export lays it out in the page's bounding box; the two
//! agree for unrotated shapes.

use std::collections::HashMap;

use egui::epaint::{Mesh, PathShape};
use egui::{Color32, Painter, Pos2, Stroke};
use vizcraft_diagram::text_fields::{FieldCtx, expand_shape};
use vizcraft_diagram::{Color, GradientKind, LineCap, Point, Shape, Style};

use super::text::shape_text;
use super::{color, fill_rings, flatten, line_px, pos, stroke_lines, tolerance};
use crate::canvas::Viewport2D;
use crate::canvas::tess::{self, triangulate};

/// A shape's outline flattened and triangulated, in world space.
struct Fill {
    key: [u64; 8],
    rings: Vec<(Vec<Pos2>, bool)>,
    tris: Vec<Vec<[u32; 3]>>,
}

/// Fills and outlines by shape id, recomputed only when the shape's geometry, its corner
/// rounding or the flattening tolerance changes, so a few hundred shapes stay cheap to draw every
/// frame.
#[derive(Default)]
pub struct FillCache {
    map: HashMap<String, Fill>,
    frame_seen: Vec<String>,
}

impl FillCache {
    fn key(s: &Shape, tol: f64) -> [u64; 8] {
        let kind = s.kind as u64;
        [s.x.to_bits(), s.y.to_bits(), s.w.to_bits(), s.h.to_bits(), s.angle_deg.to_bits(), tol.to_bits(), kind, s.style.corner_radius.to_bits()]
    }

    fn get(&mut self, s: &Shape, tol: f64) -> &Fill {
        let key = Self::key(s, tol);
        self.frame_seen.push(s.id.clone());
        let fresh = self.map.get(&s.id).is_some_and(|f| f.key == key);
        if !fresh {
            let lines = s.outline_world().rounded_corners(s.style.corner_radius).flatten(tol);
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

/// Draw one shape. `show_text` is false while the text is being edited in place.
pub fn shape(painter: &Painter, vp: &Viewport2D, s: &Shape, cache: &mut FillCache, show_text: bool) {
    let tol = tolerance(vp.zoom);
    let st = &s.style;
    // The colours every part below is drawn in, transparency applied.
    let fill_c = color(st.effective_fill());
    let line_c = color(st.effective_line());
    let fill = cache.get(s, tol);
    let filled = fill_c.a() > 0 || st.gradient.as_ref().is_some_and(|g| g.is_drawable());
    glow(painter, vp, st, &fill.rings);
    shadow(painter, vp, st, fill, filled);
    paint_fill(painter, vp, s, fill, fill_c);
    // Soft edges blur the outline too (SVG blurs the whole shape): draw it lighter.
    let line_c = if st.soft_edges > 0.0 { line_c.gamma_multiply(0.5) } else { line_c };
    outline(painter, vp, st, &fill.rings, line_c);
    if let Some(detail) = s.detail_world() {
        outline(painter, vp, st, &flatten(&detail, tol), line_c);
    }
    if show_text && !s.text.trim().is_empty() {
        shape_text(painter, vp, s);
    }
}

/// Draw the background pages under page `index`, deepest first: their visible shapes, then their
/// connectors. Generated decorations (backgrounds, borders and titles) live there.
/// Fields in their text name the foreground page `fields` is for.
pub fn background_pages(painter: &Painter, vp: &Viewport2D, fields: &FieldCtx<'_>, cache: &mut FillCache) {
    let visible = vp.visible_world_rect().inflate(4.0);
    for bg in fields.doc.background_chain(fields.page_index) {
        for s in bg.shapes.iter().filter(|s| bg.shape_visible(s)) {
            if s.bounds().intersects(&visible) {
                shape(painter, vp, &expand_shape(s, fields), cache, true);
            }
        }
        for (c, g) in bg.connectors.iter().zip(vizcraft_diagram::jumps::page_geometry(bg)) {
            super::connector(painter, vp, c, &g, true);
        }
    }
}

/// A world-space mesh moved to the screen and painted.
fn paint_mesh(painter: &Painter, vp: &Viewport2D, mut mesh: Mesh) {
    if mesh.indices.is_empty() {
        return;
    }
    for v in &mut mesh.vertices {
        v.pos = vp.world_to_screen(v.pos);
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn translate(pts: &[Pos2], dx: f32, dy: f32) -> Vec<Pos2> {
    pts.iter().map(|p| Pos2::new(p.x + dx, p.y + dy)).collect()
}

/// A halo around the closed outlines: a strip from the outline out to the glow radius, fading
/// from the glow colour to nothing (SVG: a wide blurred stroke under the shape).
fn glow(painter: &Painter, vp: &Viewport2D, st: &Style, rings: &[(Vec<Pos2>, bool)]) {
    let Some(g) = st.glow.filter(|g| g.radius > 0.0 && g.color.a > 0) else { return };
    let c = color(g.color);
    let r = g.radius as f32;
    for (pts, closed) in rings {
        if *closed {
            // Starts a little inside so the halo meets the outline without a seam.
            let inner = tess::offset_ring(pts, -r * 0.25);
            paint_mesh(painter, vp, tess::strip(&inner, &tess::offset_ring(pts, r), c, Color32::TRANSPARENT));
        } else {
            let s: Vec<Pos2> = pts.iter().map(|p| vp.world_to_screen(*p)).collect();
            painter.add(PathShape::line(s, Stroke::new(line_px(g.radius, vp.zoom), c.gamma_multiply(0.5))));
        }
    }
}

/// A drop shadow: the filled outline (or the outline alone, for an unfilled shape) offset by
/// `dx, dy` in the shadow colour, with a fading strip around it for the blur.
fn shadow(painter: &Painter, vp: &Viewport2D, st: &Style, fill: &Fill, filled: bool) {
    let Some(sh) = st.shadow.filter(|sh| sh.color.a > 0) else { return };
    let (dx, dy, blur) = (sh.dx as f32, sh.dy as f32, sh.blur as f32);
    let c = color(sh.color);
    for ((pts, closed), tri) in fill.rings.iter().zip(&fill.tris) {
        let moved = translate(pts, dx, dy);
        if filled && *closed {
            if blur > 0.0 {
                // The core is inset by half the blur and the strip spans the blur, as a Gaussian
                // spreads half inside and half outside the edge.
                let inner = tess::offset_ring(&moved, -blur / 2.0);
                let core = if tess::inset_ok(&moved, &inner) { inner } else { moved.clone() };
                paint_mesh(painter, vp, tess::solid(&core, &triangulate(&core), c));
                paint_mesh(painter, vp, tess::strip(&core, &tess::offset_ring(&moved, blur / 2.0), c, Color32::TRANSPARENT));
            } else {
                paint_mesh(painter, vp, tess::solid(&moved, tri, c));
            }
        } else {
            let width = st.line_width.max(1.0);
            stroke_lines(painter, vp, &[(moved, *closed)], Stroke::new(line_px(width, vp.zoom), c));
        }
    }
}

/// The gradient parameter of world point `p` for shape `s`, laid out in the shape's own frame
/// (`u, v` from 0 to 1 across it) the way SVG lays out an object-bounding-box gradient.
fn gradient_t(s: &Shape, inv: &vizcraft_diagram::Transform, kind: GradientKind, angle_deg: f64, p: Pos2) -> f32 {
    let local = inv.apply(Point::new(f64::from(p.x), f64::from(p.y)));
    let u = if s.w > 0.0 { local.x / s.w } else { 0.5 };
    let v = if s.h > 0.0 { local.y / s.h } else { 0.5 };
    let t = match kind {
        GradientKind::Linear => {
            let (sin, cos) = angle_deg.to_radians().sin_cos();
            (u - 0.5) * cos + (v - 0.5) * sin + 0.5
        }
        GradientKind::Radial => 2.0 * ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt(),
    };
    t as f32
}

fn rgba(c: Color, keep: f64) -> [u8; 4] {
    [c.r, c.g, c.b, (f64::from(c.a) * keep).round().clamp(0.0, 255.0) as u8]
}

/// The fill: a gradient when the style has a drawable one, else the solid colour; soft edges
/// make the outline region fade out.
fn paint_fill(painter: &Painter, vp: &Viewport2D, s: &Shape, fill: &Fill, fill_c: Color32) {
    let st = &s.style;
    let soft = st.soft_edges as f32;
    let gradient = st.gradient.as_ref().filter(|g| g.is_drawable());
    if let Some(g) = gradient {
        let keep = 1.0 - st.fill_transparency.clamp(0.0, 1.0);
        let stops: Vec<tess::Stop> = g.stops.iter().map(|s| (s.offset as f32, rgba(s.color, keep))).collect();
        let Some(inv) = s.transform().inverse() else { return };
        // The edge colour soft edges fade from: the middle stop's.
        let edge = g.stops.get(g.stops.len() / 2).map_or(fill_c, |s| color(Color { a: rgba(s.color, keep)[3], ..s.color }));
        for ((pts, closed), tri) in fill.rings.iter().zip(&fill.tris) {
            if !*closed {
                continue;
            }
            let t = |p: Pos2| gradient_t(s, &inv, g.kind, g.angle_deg, p);
            let mesh = match g.kind {
                GradientKind::Linear => tess::linear_gradient(pts, tri, t, &stops),
                GradientKind::Radial => tess::curved_gradient(pts, tri, t, &stops),
            };
            paint_mesh(painter, vp, mesh);
            if soft > 0.0 {
                paint_mesh(painter, vp, tess::strip(pts, &tess::offset_ring(pts, soft / 2.0), edge, Color32::TRANSPARENT));
            }
        }
        return;
    }
    if soft <= 0.0 || fill_c.a() == 0 {
        fill_rings(painter, vp, &fill.rings, &fill.tris, fill_c);
        return;
    }
    for ((pts, closed), tri) in fill.rings.iter().zip(&fill.tris) {
        if !*closed {
            continue;
        }
        // A solid core inset by half the distance, and a strip fading across the outline.
        let inner = tess::offset_ring(pts, -soft / 2.0);
        if tess::inset_ok(pts, &inner) {
            paint_mesh(painter, vp, tess::solid(&inner, &triangulate(&inner), fill_c));
            paint_mesh(painter, vp, tess::strip(&inner, &tess::offset_ring(pts, soft / 2.0), fill_c, Color32::TRANSPARENT));
        } else {
            // Too small to inset: the whole shape lighter, still fading outwards.
            let light = fill_c.gamma_multiply(0.6);
            paint_mesh(painter, vp, tess::solid(pts, tri, light));
            paint_mesh(painter, vp, tess::strip(pts, &tess::offset_ring(pts, soft / 2.0), light, Color32::TRANSPARENT));
        }
    }
}

/// The outline with the style's dash pattern (dash and gap lengths scale with the line width, at
/// least one unit) and line cap.
fn outline(painter: &Painter, vp: &Viewport2D, st: &Style, lines: &[(Vec<Pos2>, bool)], c: Color32) {
    let width = line_px(st.line_width, vp.zoom);
    if width <= 0.0 || c.a() == 0 {
        return;
    }
    let stroke = Stroke::new(width, c);
    let pattern = st.dash.pattern();
    if pattern.is_empty() && st.line_cap == LineCap::Butt {
        stroke_lines(painter, vp, lines, stroke);
        return;
    }
    let unit = st.line_width.max(1.0);
    let pattern: Vec<f32> = pattern.iter().map(|v| (v * unit) as f32).collect();
    let half = (st.line_width / 2.0) as f32;
    for (pts, closed) in lines {
        let pieces = if pattern.is_empty() {
            // A solid line: caps matter only at the ends of an open one.
            if *closed {
                stroke_lines(painter, vp, &[(pts.clone(), true)], stroke);
                continue;
            }
            vec![pts.clone()]
        } else {
            tess::dashes(pts, *closed, &pattern)
        };
        for mut piece in pieces {
            if piece.len() < 2 {
                continue;
            }
            if st.line_cap == LineCap::Square {
                tess::extend_ends(&mut piece, half);
            }
            let screen: Vec<Pos2> = piece.iter().map(|p| vp.world_to_screen(*p)).collect();
            if st.line_cap == LineCap::Round {
                for end in [screen.first(), screen.last()].into_iter().flatten() {
                    painter.circle_filled(*end, width / 2.0, c);
                }
            }
            painter.add(PathShape::line(screen, stroke));
        }
    }
}
