//! Finding where connectors cross, so the renderer can draw line jumps.

use crate::geom::eps_for;
use crate::{Jump, JumpOrder, Point};

/// Upper bound on segment pairs examined, so a pathological page cannot stall the caller. Typical
/// pages need a tiny fraction of it after the bounding-box rejection.
const MAX_PAIR_CHECKS: usize = 20_000_000;

struct Seg {
    /// Index of the segment in its polyline; degenerate segments are skipped, so it can differ
    /// from the position in the filtered list.
    index: usize,
    a: Point,
    b: Point,
    min: Point,
    max: Point,
}

fn segments(points: &[Point]) -> Vec<Seg> {
    points
        .windows(2)
        .enumerate()
        .filter_map(|(index, w)| match w {
            [a, b] if a.is_finite() && b.is_finite() && a != b => {
                Some(Seg { index, a: *a, b: *b, min: Point::new(a.x.min(b.x), a.y.min(b.y)), max: Point::new(a.x.max(b.x), a.y.max(b.y)) })
            }
            _ => None,
        })
        .collect()
}

fn overlaps(a_min: Point, a_max: Point, b_min: Point, b_max: Point) -> bool {
    a_min.x <= b_max.x && b_min.x <= a_max.x && a_min.y <= b_max.y && b_min.y <= a_max.y
}

fn cross(u: Point, v: Point) -> f64 {
    u.x * v.y - u.y * v.x
}

/// The distance along `s` at which it properly crosses `o`, if it does: strictly inside both
/// segments and not parallel.
fn crossing(s: &Seg, o: &Seg) -> Option<(f64, Point)> {
    let r = Point::new(s.b.x - s.a.x, s.b.y - s.a.y);
    let q = Point::new(o.b.x - o.a.x, o.b.y - o.a.y);
    let (lr, lq) = (r.x.hypot(r.y), q.x.hypot(q.y));
    let denom = cross(r, q);
    if !(lr > 0.0 && lq > 0.0) || denom.abs() <= 1e-12 * lr * lq {
        return None;
    }
    let w = Point::new(o.a.x - s.a.x, o.a.y - s.a.y);
    let t = cross(w, q) / denom;
    let u = cross(w, r) / denom;
    let scale = s.max.x.abs().max(s.max.y.abs()).max(s.min.x.abs()).max(s.min.y.abs());
    let tol = eps_for(scale, 0.0) * 10.0;
    let (dt, du) = (t * lr, u * lq);
    let inside = dt > tol && dt < lr - tol && du > tol && du < lq - tol;
    (inside && dt.is_finite()).then(|| (dt, Point::new(s.a.x + r.x * t, s.a.y + r.y * t)))
}

pub(crate) fn jumps(polylines: &[Vec<Point>], order: JumpOrder) -> Vec<Vec<Jump>> {
    let segs: Vec<Vec<Seg>> = polylines.iter().map(|p| segments(p)).collect();
    let boxes: Vec<Option<(Point, Point)>> = segs
        .iter()
        .map(|ss| {
            ss.iter().fold(None, |acc: Option<(Point, Point)>, s| {
                Some(acc.map_or((s.min, s.max), |(lo, hi)| {
                    (Point::new(lo.x.min(s.min.x), lo.y.min(s.min.y)), Point::new(hi.x.max(s.max.x), hi.y.max(s.max.y)))
                }))
            })
        })
        .collect();
    let mut budget = MAX_PAIR_CHECKS;
    let mut out = Vec::with_capacity(polylines.len());
    for (ai, a_segs) in segs.iter().enumerate() {
        let mut found: Vec<Jump> = Vec::new();
        let others = match order {
            JumpOrder::LaterOverEarlier => 0..ai,
            JumpOrder::EarlierOverLater => ai.saturating_add(1)..segs.len(),
        };
        for bi in others {
            let (Some(Some(abox)), Some(Some(bbox)), Some(b_segs)) = (boxes.get(ai), boxes.get(bi), segs.get(bi)) else { continue };
            if !overlaps(abox.0, abox.1, bbox.0, bbox.1) {
                continue;
            }
            for s in a_segs {
                for o in b_segs {
                    if budget == 0 {
                        break;
                    }
                    budget -= 1;
                    if !overlaps(s.min, s.max, o.min, o.max) {
                        continue;
                    }
                    if let Some((distance, point)) = crossing(s, o) {
                        found.push(Jump { segment: s.index, distance, point, over: bi });
                    }
                }
            }
        }
        found.sort_by(|x, y| x.segment.cmp(&y.segment).then(x.distance.total_cmp(&y.distance)).then(x.over.cmp(&y.over)));
        // Two earlier connectors crossing at the same spot need only one hop.
        found.dedup_by(|later, kept| later.segment == kept.segment && (later.distance - kept.distance).abs() <= eps_for(later.distance, 0.0));
        out.push(found);
    }
    out
}
