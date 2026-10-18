//! Rounding the bends of a polyline with arcs that stay clear of obstacles, for curved
//! connectors drawn over an obstacle-avoiding route.

use crate::geom::{self, point_ok};
use crate::{Bend, Obstacle, Point, Rect};

/// Share of an interior segment one arc may use: half, so the arcs at its two ends never meet.
const INTERIOR_SHARE: f64 = 0.5;

/// Share of the first or last segment an arc may use, leaving a straight piece at the end so the
/// line leaves its shape (and meets its arrowhead) straight.
const END_SHARE: f64 = 0.75;

/// Longest tangent an arc may have, so a huge diagram does not make a curve of absurd size.
const MAX_TANGENT: f64 = 1.0e5;

/// Smallest tangent worth drawing as a curve; below it the corner stays sharp.
const MIN_TANGENT: f64 = 1.0;

/// Each failed try shrinks the arc by this factor.
const SHRINK: f64 = 0.7;

/// Samples taken along each arc when checking it.
const SAMPLES: usize = 16;

fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}

fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}

fn scale(a: Point, k: f64) -> Point {
    Point::new(a.x * k, a.y * k)
}

fn length(a: Point) -> f64 {
    a.x.hypot(a.y)
}

/// The point at `t` on the cubic `a`, `b`, `c`, `d`.
fn cubic_at(a: Point, b: Point, c: Point, d: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Point::new(w0 * a.x + w1 * b.x + w2 * c.x + w3 * d.x, w0 * a.y + w1 * b.y + w2 * c.y + w3 * d.y)
}

/// The circular arc replacing the corner `c` between unit directions `u` (in) and `v` (out), with
/// tangent length `t`: it touches the segments `t` before and after the corner. `cos` is `u . v`.
fn arc(c: Point, u: Point, v: Point, t: f64, cos: f64) -> Bend {
    // Turn angle theta between the headings; the arc's radius is t / tan(theta / 2) and the
    // handle of a cubic matching a circular arc of angle theta is 4/3 tan(theta / 4) radii.
    let theta = cos.clamp(-1.0, 1.0).acos();
    let half = (theta / 2.0).tan();
    let radius = if half > 1e-12 { t / half } else { 0.0 };
    let handle = 4.0 / 3.0 * (theta / 4.0).tan() * radius;
    let from = sub(c, scale(u, t));
    let to = add(c, scale(v, t));
    Bend { from, c1: add(from, scale(u, handle)), c2: sub(to, scale(v, handle)), to }
}

fn clear(b: &Bend, rects: &[Rect]) -> bool {
    let mut prev = b.from;
    for i in 1..=SAMPLES {
        let p = cubic_at(b.from, b.c1, b.c2, b.to, i as f64 / SAMPLES as f64);
        if !point_ok(p) || rects.iter().any(|r| geom::segment_hits_interior(prev, p, r)) {
            return false;
        }
        prev = p;
    }
    true
}

pub(crate) fn round_bends(points: &[Point], obstacles: &[Obstacle], ignore: &[usize], clearance: f64) -> Vec<Option<Bend>> {
    let n = points.len();
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    if n < 3 || points.iter().any(|p| !point_ok(*p)) {
        out.resize(n.saturating_sub(2), None);
        return out;
    }
    let clearance = if clearance.is_finite() { clearance.clamp(0.0, 1.0e6) } else { 0.0 };
    let rects: Vec<Rect> = obstacles
        .iter()
        .enumerate()
        .filter(|(k, o)| !ignore.contains(k) && o.rect.is_finite())
        .map(|(_, o)| o.rect.normalized().expanded(clearance))
        .collect();
    for i in 1..n - 1 {
        let (Some(&a), Some(&c), Some(&b)) = (points.get(i - 1), points.get(i), points.get(i + 1)) else {
            out.push(None);
            continue;
        };
        let (din, dout) = (sub(c, a), sub(b, c));
        let (lin, lout) = (length(din), length(dout));
        if !(lin > 1e-9 && lout > 1e-9) {
            out.push(None);
            continue;
        }
        let (u, v) = (scale(din, 1.0 / lin), scale(dout, 1.0 / lout));
        let cos = u.x * v.x + u.y * v.y;
        // A straight run or a reversal has no corner to round.
        if !(-0.999..=0.999).contains(&cos) {
            out.push(None);
            continue;
        }
        let share_in = if i == 1 { END_SHARE } else { INTERIOR_SHARE };
        let share_out = if i + 2 == n { END_SHARE } else { INTERIOR_SHARE };
        // Only obstacles near the corner can meet its arc.
        let mut t = (lin * share_in).min(lout * share_out).min(MAX_TANGENT);
        let reach = t * 2.0 + clearance + 1.0;
        let near: Vec<Rect> = rects
            .iter()
            .filter(|r| r.min.x <= c.x + reach && r.max.x >= c.x - reach && r.min.y <= c.y + reach && r.max.y >= c.y - reach)
            .copied()
            .collect();
        let mut found = None;
        while t >= MIN_TANGENT {
            let bend = arc(c, u, v, t, cos);
            if clear(&bend, &near) {
                found = Some(bend);
                break;
            }
            t *= SHRINK;
        }
        out.push(found);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turn_matches_a_circle() {
        let b = arc(Point::new(10.0, 0.0), Point::new(1.0, 0.0), Point::new(0.0, 1.0), 10.0, 0.0);
        assert_eq!(b.from, Point::new(0.0, 0.0));
        assert_eq!(b.to, Point::new(10.0, 10.0));
        // The midpoint of a quarter circle of radius 10 centred at (0, 10).
        let mid = cubic_at(b.from, b.c1, b.c2, b.to, 0.5);
        let r = (mid.x - 0.0).hypot(mid.y - 10.0);
        assert!((r - 10.0).abs() < 0.05, "{r}");
    }
}
