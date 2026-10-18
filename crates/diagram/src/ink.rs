//! Ink: freehand strokes drawn with the Pen and Highlighter tools, and the recognition that turns a
//! stroke into an ellipse, a rectangle or a line.
//!
//! A stroke is an open [`ShapeKind::Freeform`] shape with round caps whose `ext` holds
//! [`INK_KEY`] set to `pen` or `highlighter`; nothing else marks it, so a file written before ink
//! existed reads the same and a stroke survives every command that keeps a freeform shape.

use crate::geom::{Point, Rect, distance_to_segment};
use crate::shape_fields::Freeform;
use crate::{Color, LineCap, Shape, ShapeKind};

/// The `ext` key that marks a shape as an ink stroke; the value is `pen` or `highlighter`.
pub const INK_KEY: &str = "vizcraft.ink";
/// How far (world units) a simplified stroke may stray from the drawn points.
pub const SIMPLIFY_TOLERANCE: f64 = 1.5;
/// Most points one stroke keeps after simplifying.
pub const MAX_POINTS: usize = 4096;
/// Most points `ink.add` accepts before simplifying.
pub const MAX_INPUT_POINTS: usize = 50_000;
/// Line width of a highlighter stroke.
pub const HIGHLIGHTER_WIDTH: f64 = 12.0;
/// Line transparency of a highlighter stroke.
pub const HIGHLIGHTER_TRANSPARENCY: f64 = 0.5;
/// Default line width of a pen stroke.
pub const PEN_WIDTH: f64 = 2.0;
/// How far, as a fraction of the radius, a closed stroke may stray from its bounding ellipse and
/// still read as an ellipse.
pub const ELLIPSE_FIT: f64 = 0.15;

/// Which tool drew a stroke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pen {
    /// A thin opaque line.
    Pen,
    /// A wide half-transparent line drawn behind other shapes.
    Highlighter,
}

impl Pen {
    /// The `ext` value.
    pub fn name(self) -> &'static str {
        match self {
            Pen::Pen => "pen",
            Pen::Highlighter => "highlighter",
        }
    }
}

/// Which tool drew `s`, when it is an ink stroke.
pub fn stroke_kind(s: &Shape) -> Option<Pen> {
    if s.kind != ShapeKind::Freeform {
        return None;
    }
    match s.ext.get(INK_KEY).map(String::as_str) {
        Some("pen") => Some(Pen::Pen),
        Some("highlighter") => Some(Pen::Highlighter),
        _ => None,
    }
}

/// Whether `s` is an ink stroke.
pub fn is_stroke(s: &Shape) -> bool {
    stroke_kind(s).is_some()
}

/// Points simplified in one run of [`simplify`]; a longer stroke is simplified a run at a time
/// (each run keeps its ends), so the cost stays linear in the stroke's length.
const RUN: usize = 512;

/// Ramer-Douglas-Peucker simplification: the fewest of `pts` (first and last always kept) such
/// that no dropped point lies further than `tolerance` from the kept line. Iterative, so a long
/// stroke cannot overflow the stack, and done in runs of [`RUN`] points.
pub fn simplify(pts: &[Point], tolerance: f64) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    let mut start = 0usize;
    while start + 1 < pts.len() {
        let end = (start + RUN).min(pts.len() - 1);
        let run = simplify_run(pts.get(start..=end).unwrap_or_default(), tolerance);
        // Each run starts where the last one ended.
        let skip = usize::from(!out.is_empty());
        out.extend(run.into_iter().skip(skip));
        start = end;
    }
    if out.is_empty() {
        out.extend(pts.first().copied());
    }
    out
}

fn simplify_run(pts: &[Point], tolerance: f64) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let last = pts.len() - 1;
    let mut keep = vec![false; pts.len()];
    for i in [0, last] {
        if let Some(k) = keep.get_mut(i) {
            *k = true;
        }
    }
    let mut stack = vec![(0usize, last)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (Some(&pa), Some(&pb)) = (pts.get(a), pts.get(b)) else { continue };
        let mut far = (0.0, a);
        for (i, p) in pts.iter().enumerate().take(b).skip(a + 1) {
            let d = distance_to_segment(*p, pa, pb);
            if d > far.0 {
                far = (d, i);
            }
        }
        if far.0 > tolerance {
            if let Some(k) = keep.get_mut(far.1) {
                *k = true;
            }
            stack.push((a, far.1));
            stack.push((far.1, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// Simplify `pts` with [`SIMPLIFY_TOLERANCE`] and cap the result at [`MAX_POINTS`] (by raising the
/// tolerance, then by taking every n-th point), dropping non-finite points first.
pub fn prepare(pts: &[Point]) -> Vec<Point> {
    let clean: Vec<Point> = pts.iter().copied().filter(|p| p.x.is_finite() && p.y.is_finite()).take(MAX_INPUT_POINTS).collect();
    let mut tol = SIMPLIFY_TOLERANCE;
    let mut out = simplify(&clean, tol);
    for _ in 0..8 {
        if out.len() <= MAX_POINTS {
            break;
        }
        tol *= 2.0;
        out = simplify(&clean, tol);
    }
    if out.len() > MAX_POINTS {
        let step = out.len().div_ceil(MAX_POINTS).max(1);
        let last = out.last().copied();
        out = out.into_iter().step_by(step).collect();
        if let (Some(l), Some(end)) = (last, out.last())
            && *end != l
            && out.len() < MAX_POINTS
        {
            out.push(l);
        }
    }
    out
}

/// A stroke shape through `pts` (world coordinates, already prepared): its frame fitted to the
/// points, the polyline stored in that frame, round caps, no fill. `None` for no points.
pub fn stroke_shape(id: String, pts: &[Point], pen: Pen, width: f64, color: Color) -> Option<Shape> {
    let first = *pts.first()?;
    let pts: Vec<Point> = if pts.len() == 1 { vec![first, first] } else { pts.to_vec() };
    let b = bounds(&pts)?;
    let local: Vec<Point> = pts.iter().map(|p| Point::new(p.x - b.x0, p.y - b.y0)).collect();
    let path = Freeform::new(&crate::geom::Path::polyline(&local), false);
    let mut s = Shape::new(id, ShapeKind::Freeform, b.x0, b.y0, b.width(), b.height());
    s.path = Some(path);
    s.style.fill = Color::TRANSPARENT;
    s.style.gradient = None;
    s.style.line = color;
    s.style.line_cap = LineCap::Round;
    match pen {
        Pen::Pen => s.style.line_width = width,
        Pen::Highlighter => {
            s.style.line_width = HIGHLIGHTER_WIDTH;
            s.style.line_transparency = HIGHLIGHTER_TRANSPARENCY;
        }
    }
    s.ext.insert(INK_KEY.into(), pen.name().into());
    s.connection_points.clear();
    s.sanitize();
    Some(s)
}

/// The bounding box of `pts`.
pub fn bounds(pts: &[Point]) -> Option<Rect> {
    let first = pts.first()?;
    let mut r = Rect { x0: first.x, y0: first.y, x1: first.x, y1: first.y };
    for p in pts {
        r.x0 = r.x0.min(p.x);
        r.y0 = r.y0.min(p.y);
        r.x1 = r.x1.max(p.x);
        r.y1 = r.y1.max(p.y);
    }
    Some(r)
}

/// The points a stroke draws through, in world coordinates.
pub fn stroke_points(s: &Shape) -> Vec<Point> {
    let t = s.transform();
    s.local_outline().flatten(0.5).into_iter().flat_map(|pl| pl.points).map(|p| t.apply(p)).collect()
}

/// What a stroke reads as.
#[derive(Clone, Debug, PartialEq)]
pub enum Recognized {
    /// A closed stroke close to its bounding ellipse.
    Ellipse(Rect),
    /// A closed stroke with four corners.
    Rectangle(Rect),
    /// A nearly straight stroke, from its first to its last point.
    Line(Point, Point),
}

/// Recognise a stroke drawn through `pts` (world coordinates): a closed one (its ends within a
/// fifth of its size of each other) that stays within [`ELLIPSE_FIT`] of its bounding ellipse is
/// an ellipse, one that simplifies to four corners a rectangle; an open one whose points all lie
/// within 6 percent of its length from the line between its ends is a line. `None` for the rest.
pub fn recognize(pts: &[Point]) -> Option<Recognized> {
    let b = bounds(pts)?;
    let (first, last) = (*pts.first()?, *pts.last()?);
    let size = b.width().hypot(b.height());
    if size < 4.0 || pts.len() < 2 {
        return None;
    }
    let closed = pts.len() >= 4 && first.distance(last) <= size * 0.2;
    if !closed {
        let len = first.distance(last);
        let straight = len > 4.0 && pts.iter().all(|p| distance_to_segment(*p, first, last) <= len * 0.06);
        return straight.then_some(Recognized::Line(first, last));
    }
    let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    let (rx, ry) = (b.width() / 2.0, b.height() / 2.0);
    if rx > 1.0 && ry > 1.0 {
        let fits = pts.iter().all(|p| {
            let r = ((p.x - cx) / rx).hypot((p.y - cy) / ry);
            (r - 1.0).abs() <= ELLIPSE_FIT
        });
        if fits {
            return Some(Recognized::Ellipse(b));
        }
    }
    // Corners: the closed stroke simplified coarsely, its two ends merged into one corner.
    let mut ring: Vec<Point> = pts.to_vec();
    ring.push(first);
    let coarse = simplify(&ring, size * 0.06);
    let mut corners: Vec<Point> = coarse.iter().copied().take(coarse.len().saturating_sub(1)).collect();
    // The start may lie partway along a side; drop it when it sits on the line through its
    // neighbours.
    if corners.len() == 5
        && let (Some(&p), Some(&n), Some(&f)) = (corners.last(), corners.get(1), corners.first())
        && distance_to_segment(f, p, n) <= size * 0.06
    {
        corners.remove(0);
    }
    (corners.len() == 4).then_some(Recognized::Rectangle(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(n: usize) -> Vec<Point> {
        let mut v = Vec::new();
        let corners = [Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0), Point::new(0.0, 100.0), Point::new(0.0, 0.0)];
        for w in corners.windows(2) {
            for i in 0..n {
                let t = i as f64 / n as f64;
                // A little wobble, as a hand draws.
                let j = if i % 2 == 0 { 0.6 } else { -0.6 };
                v.push(Point::new(w[0].x + (w[1].x - w[0].x) * t + j, w[0].y + (w[1].y - w[0].y) * t - j));
            }
        }
        v.push(Point::new(1.0, 2.0));
        v
    }

    fn circle(n: usize) -> Vec<Point> {
        (0..=n)
            .map(|i| {
                let a = i as f64 / n as f64 * std::f64::consts::TAU;
                Point::new(200.0 + 50.0 * a.cos(), 100.0 + 40.0 * a.sin())
            })
            .collect()
    }

    #[test]
    fn simplify_keeps_corners_and_drops_collinear_points() {
        let pts: Vec<Point> = (0..=100).map(|i| Point::new(f64::from(i), 0.0)).chain((1..=100).map(|i| Point::new(100.0, f64::from(i)))).collect();
        let s = simplify(&pts, 1.5);
        assert_eq!(s, vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0)]);
        assert!(simplify(&[], 1.0).is_empty());
    }

    #[test]
    fn prepare_caps_the_point_count() {
        let pts: Vec<Point> = (0..20_000).map(|i| Point::new(f64::from(i), if i % 2 == 0 { 0.0 } else { 40.0 })).collect();
        assert!(prepare(&pts).len() <= MAX_POINTS);
        assert_eq!(prepare(&[Point::new(f64::NAN, 0.0)]).len(), 0);
    }

    #[test]
    fn recognition_reads_squares_circles_and_lines() {
        assert!(matches!(recognize(&square(20)), Some(Recognized::Rectangle(_))));
        assert!(matches!(recognize(&circle(64)), Some(Recognized::Ellipse(_))));
        let line: Vec<Point> = (0..30).map(|i| Point::new(f64::from(i) * 5.0, f64::from(i % 3))).collect();
        assert!(matches!(recognize(&line), Some(Recognized::Line(..))));
        let zigzag: Vec<Point> = (0..30).map(|i| Point::new(f64::from(i) * 5.0, if i % 2 == 0 { 0.0 } else { 40.0 })).collect();
        assert_eq!(recognize(&zigzag), None);
    }
}
