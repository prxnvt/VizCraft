//! Shared geometry helpers and input sanitising.

use crate::{Hit, OWN_TOLERANCE, Obstacle, Options, Point, Rect};

/// Coordinates beyond this magnitude are treated as junk. Diagrams never get near it, and keeping
/// well inside f64 range means sums and midpoints cannot overflow to infinity.
pub(crate) const COORD_LIMIT: f64 = 1e9;

/// More obstacles than this and the router falls back rather than spend unbounded time.
pub(crate) const MAX_OBSTACLES: usize = 5000;

/// Upper bound on `Options::max_grid`, so a caller cannot ask for gigabytes of search state.
const HARD_MAX_GRID: usize = 1_000_000;

/// Largest margin or spacing accepted; anything bigger is a units mistake, not a diagram.
const MAX_DISTANCE: f64 = 1e6;

/// Options after sanitising: every number finite and non-negative.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Opts {
    pub margin: f64,
    pub bend: f64,
    /// Zero means crossings are not penalised.
    pub crossing: f64,
    /// Zero means running on top of earlier connectors is not penalised.
    pub overlap: f64,
    pub spacing: f64,
    pub max_grid: usize,
}

fn distance_or(v: f64, default: f64) -> f64 {
    if v.is_finite() && v >= 0.0 { v.min(MAX_DISTANCE) } else { default }
}

impl From<&Options> for Opts {
    fn from(o: &Options) -> Self {
        let d = Options::default();
        Self {
            margin: distance_or(o.margin, d.margin),
            bend: distance_or(o.bend_penalty, d.bend_penalty),
            crossing: o.crossing_penalty.map_or(0.0, |c| distance_or(c, 0.0)),
            overlap: o.overlap_penalty.map_or(0.0, |c| distance_or(c, 0.0)),
            spacing: distance_or(o.segment_spacing, d.segment_spacing),
            max_grid: o.max_grid.min(HARD_MAX_GRID),
        }
    }
}

/// Whether a coordinate is finite and within the accepted range.
pub(crate) fn coord_ok(v: f64) -> bool {
    v.is_finite() && v.abs() <= COORD_LIMIT
}

/// Whether a point is usable for routing.
pub(crate) fn point_ok(p: Point) -> bool {
    coord_ok(p.x) && coord_ok(p.y)
}

/// Normalises obstacles, keeping their indices so glued ends still refer to the right shape.
/// Unusable rectangles become `None` and are ignored.
pub(crate) fn prepare(obstacles: &[Obstacle]) -> Vec<Option<Rect>> {
    obstacles
        .iter()
        .map(|o| {
            let r = o.rect.normalized();
            (point_ok(r.min) && point_ok(r.max)).then_some(r)
        })
        .collect()
}

/// Tolerance for comparing coordinates of the given magnitude: f64 keeps about 16 digits, so the
/// tolerance scales with the size of the numbers.
pub(crate) fn eps_for(a: f64, b: f64) -> f64 {
    1e-9 * (1.0 + a.abs().max(b.abs()))
}

/// Whether two coordinates are equal within tolerance.
pub(crate) fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= eps_for(a, b)
}

/// Whether two points coincide within tolerance.
pub(crate) fn same_point(a: Point, b: Point) -> bool {
    near(a.x, b.x) && near(a.y, b.y)
}

/// Whether the closed segment `a`-`b` enters the open interior of `r`. Touching the boundary does
/// not count, which lets routes run along an expanded obstacle edge.
pub(crate) fn segment_hits_interior(a: Point, b: Point, r: &Rect) -> bool {
    let ex = eps_for(r.min.x, r.max.x);
    let ey = eps_for(r.min.y, r.max.y);
    let (x0, x1, y0, y1) = (r.min.x + ex, r.max.x - ex, r.min.y + ey, r.max.y - ey);
    if x0 >= x1 || y0 >= y1 {
        return false;
    }
    // Liang-Barsky clipping against the slightly shrunk closed rectangle.
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut t0 = 0.0_f64;
    let mut t1 = 1.0_f64;
    for (p, q) in [(-dx, a.x - x0), (dx, x1 - a.x), (-dy, a.y - y0), (dy, y1 - a.y)] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                if t > t1 {
                    return false;
                }
                t0 = t0.max(t);
            } else {
                if t < t0 {
                    return false;
                }
                t1 = t1.min(t);
            }
        }
    }
    t0 <= t1
}

/// Whether `p` lies strictly inside `r`.
fn strictly_inside(p: Point, r: &Rect) -> bool {
    p.x > r.min.x && p.x < r.max.x && p.y > r.min.y && p.y < r.max.y
}

/// How many leading points of `points` lie strictly inside `r`.
fn inside_run<'a>(points: impl Iterator<Item = &'a Point>, r: &Rect) -> usize {
    points.take_while(|p| strictly_inside(**p, r)).count()
}

/// See [`crate::crossings`].
pub(crate) fn crossings(
    points: &[Point],
    obstacles: &[Obstacle],
    ignore: &[usize],
    start_own: Option<usize>,
    end_own: Option<usize>,
    clearance: f64,
) -> Vec<Hit> {
    let clearance = if clearance.is_finite() { clearance.clamp(-MAX_DISTANCE, MAX_DISTANCE) } else { 0.0 };
    let segments: Vec<(usize, Point, Point)> = match points {
        [] => return Vec::new(),
        [p] => vec![(0, *p, *p)],
        pts => pts.windows(2).enumerate().filter_map(|(i, w)| if let [a, b] = w { Some((i, *a, *b)) } else { None }).collect(),
    };
    let segments: Vec<(usize, Point, Point)> = segments.into_iter().filter(|(_, a, b)| point_ok(*a) && point_ok(*b)).collect();
    let Some(bounds) = segments.iter().map(|(_, a, b)| Rect::new(a.x, a.y, b.x, b.y)).reduce(|u, r| Rect {
        min: Point::new(u.min.x.min(r.min.x), u.min.y.min(r.min.y)),
        max: Point::new(u.max.x.max(r.max.x), u.max.y.max(r.max.y)),
    }) else {
        return Vec::new();
    };
    let n = points.len();
    let mut out = Vec::new();
    for (k, o) in obstacles.iter().enumerate() {
        if ignore.contains(&k) || !o.rect.is_finite() {
            continue;
        }
        let own = Some(k) == start_own || Some(k) == end_own;
        let grow = if own { -OWN_TOLERANCE } else { clearance };
        let r = o.rect.normalized().expanded(grow);
        if r.min.x > bounds.max.x || r.max.x < bounds.min.x || r.min.y > bounds.max.y || r.max.y < bounds.min.y {
            continue;
        }
        // The stub inside an own obstacle: segments whose points so far all lie inside it.
        let lead = if Some(k) == start_own { inside_run(points.iter(), &r) } else { 0 };
        let trail = if Some(k) == end_own { inside_run(points.iter().rev(), &r) } else { 0 };
        for &(i, a, b) in &segments {
            // Segment i joins points i and i + 1: it is in the leading stub when point i is, and in
            // the trailing stub when point i + 1 is.
            let in_lead = i < lead;
            let in_trail = n >= 2 && i + 1 >= n.saturating_sub(trail) && trail > 0;
            if in_lead || in_trail {
                continue;
            }
            if segment_hits_interior(a, b, &r) {
                out.push(Hit { obstacle: k, segment: i });
            }
        }
    }
    out.sort_by(|x, y| x.segment.cmp(&y.segment).then(x.obstacle.cmp(&y.obstacle)));
    out.dedup();
    out
}

/// Removes repeated points and collinear interior points from an orthogonal polyline.
pub(crate) fn simplify(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        if out.last().is_some_and(|&l| same_point(l, p)) {
            continue;
        }
        while let [.., a, b] = out.as_slice() {
            let collinear = (near(a.x, b.x) && near(b.x, p.x)) || (near(a.y, b.y) && near(b.y, p.y));
            if !collinear {
                break;
            }
            out.pop();
        }
        if out.last().is_some_and(|&l| same_point(l, p)) {
            continue;
        }
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplify_drops_duplicates_and_collinear_points() {
        let pts = [Point::new(0.0, 0.0), Point::new(0.0, 0.0), Point::new(5.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 5.0)];
        assert_eq!(simplify(&pts), vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 5.0)]);
    }

    #[test]
    fn boundary_touch_is_not_a_hit() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(!segment_hits_interior(Point::new(-5.0, 0.0), Point::new(15.0, 0.0), &r));
        assert!(segment_hits_interior(Point::new(-5.0, 5.0), Point::new(15.0, 5.0), &r));
        assert!(segment_hits_interior(Point::new(5.0, 5.0), Point::new(5.0, 5.0), &r));
    }
}
