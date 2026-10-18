//! Drawing shapes: cached fills and outlines, detail lines and the shape's text.

use std::collections::HashMap;

use egui::{Painter, Pos2, Stroke};
use vizcraft_diagram::Shape;

use super::text::shape_text;
use super::{color, fill_rings, flatten, line_px, pos, stroke_lines, tolerance};
use crate::canvas::Viewport2D;
use crate::canvas::tess::triangulate;

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
