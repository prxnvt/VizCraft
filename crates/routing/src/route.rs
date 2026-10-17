//! Routing one connector: terminals and stubs, the search region, grid construction and fallback.

use crate::geom::{self, Opts};
use crate::search::{self, DOWN, Ends, Grid, LEFT, RIGHT, UP};
use crate::{End, Point, Rect, Request, Route, Side};

/// A routed connector plus what nudging needs to know about it.
pub(crate) struct Routed {
    pub route: Route,
    /// The clearance the route was found with (it may be smaller than requested after a retry),
    /// or `None` for a fallback route, which nudging leaves alone.
    pub margin: Option<f64>,
}

/// Why one routing attempt did not produce a route.
enum Miss {
    /// No path in this region at this margin; a larger region or smaller margin may work.
    NoPath,
    /// The grid would exceed `max_grid`; larger regions would too.
    TooBig,
}

/// The heading a side points in, away from its shape.
fn side_dir(side: Side) -> usize {
    match side {
        Side::Left => LEFT,
        Side::Right => RIGHT,
        Side::Top => UP,
        Side::Bottom => DOWN,
    }
}

/// One end resolved against the obstacles.
struct Terminal {
    point: Point,
    /// Where the route leaves the stub and joins the grid; equal to `point` without a side.
    stub: Point,
    /// Heading pointing out of the shape, if the side is fixed.
    dir: Option<usize>,
    glued: Option<usize>,
}

/// The side of `r` closest to `p`, used when a glued end names no side.
fn nearest_side(p: Point, r: &Rect) -> Side {
    let candidates = [
        ((p.x - r.min.x).abs(), Side::Left),
        ((p.x - r.max.x).abs(), Side::Right),
        ((p.y - r.min.y).abs(), Side::Top),
        ((p.y - r.max.y).abs(), Side::Bottom),
    ];
    let mut best = (f64::INFINITY, Side::Left);
    for (d, s) in candidates {
        if d < best.0 {
            best = (d, s);
        }
    }
    best.1
}

fn terminal(end: &End, obstacles: &[Option<Rect>], margin: f64) -> Terminal {
    let glued_rect = end.obstacle.and_then(|k| obstacles.get(k).copied().flatten());
    let glued = glued_rect.and(end.obstacle);
    let side = end.side.or_else(|| glued_rect.map(|r| nearest_side(end.point, &r)));
    let p = end.point;
    // The stub reaches one margin beyond the glued shape even when the point sits inside it, so
    // the grid search always starts outside every expanded obstacle.
    let stub = match (side, glued_rect) {
        (None, _) => p,
        (Some(Side::Left), g) => Point::new(g.map_or(p.x - margin, |g| (p.x - margin).min(g.min.x - margin)), p.y),
        (Some(Side::Right), g) => Point::new(g.map_or(p.x + margin, |g| (p.x + margin).max(g.max.x + margin)), p.y),
        (Some(Side::Top), g) => Point::new(p.x, g.map_or(p.y - margin, |g| (p.y - margin).min(g.min.y - margin))),
        (Some(Side::Bottom), g) => Point::new(p.x, g.map_or(p.y + margin, |g| (p.y + margin).max(g.max.y + margin))),
    };
    Terminal { point: p, stub, dir: side.map(side_dir), glued }
}

/// The simple orthogonal route used when routing around obstacles is impossible or unsafe.
fn fallback(start: &End, end: &End) -> Route {
    let (a, b) = (start.point, end.point);
    let horizontal = |s: Option<Side>| s.map(|s| matches!(s, Side::Left | Side::Right));
    let horizontal_first = horizontal(start.side).or_else(|| horizontal(end.side)).unwrap_or_else(|| (b.x - a.x).abs() >= (b.y - a.y).abs());
    // Halving before adding keeps the midpoint finite even for coordinates near f64::MAX.
    let points = if horizontal_first {
        let mx = a.x / 2.0 + b.x / 2.0;
        [a, Point::new(mx, a.y), Point::new(mx, b.y), b]
    } else {
        let my = a.y / 2.0 + b.y / 2.0;
        [a, Point::new(a.x, my), Point::new(b.x, my), b]
    };
    Route { points: geom::simplify(&points), fallback: true }
}

/// Routes one request. `existing` holds segments of connectors routed before, used only when
/// crossings are penalised.
pub(crate) fn route_one(obstacles: &[Option<Rect>], req: &Request, opts: &Opts, existing: &[(Point, Point)]) -> Routed {
    let (s, e) = (req.start.point, req.end.point);
    if !s.is_finite() || !e.is_finite() {
        let points = [s, e].into_iter().filter(Point::is_finite).collect();
        return Routed { route: Route { points, fallback: true }, margin: None };
    }
    if !geom::point_ok(s) || !geom::point_ok(e) || obstacles.len() > geom::MAX_OBSTACLES {
        return Routed { route: fallback(&req.start, &req.end), margin: None };
    }
    if geom::same_point(s, e) {
        return Routed { route: Route { points: vec![s], fallback: false }, margin: Some(opts.margin) };
    }
    // Tight layouts may leave no room at the full margin; trying smaller clearances before giving
    // up keeps routes obstacle-free where a fallback would cut through shapes.
    let mut margins = vec![opts.margin];
    if opts.margin > 0.0 {
        margins.extend([opts.margin / 2.0, 0.0]);
    }
    for m in margins {
        for level in 0..3 {
            match attempt(obstacles, req, opts, existing, m, level) {
                Ok(points) => return Routed { route: Route { points, fallback: false }, margin: Some(m) },
                Err(Miss::NoPath) => {}
                Err(Miss::TooBig) => break,
            }
        }
    }
    Routed { route: fallback(&req.start, &req.end), margin: None }
}

/// Whether a stub segment avoids every expanded obstacle except the one its end is glued to.
fn stub_clear(t: &Terminal, obstacles: &[Option<Rect>], m: f64) -> bool {
    if geom::same_point(t.point, t.stub) {
        return true;
    }
    obstacles
        .iter()
        .enumerate()
        .filter(|(k, _)| Some(*k) != t.glued)
        .filter_map(|(_, r)| r.as_ref())
        .all(|r| !geom::segment_hits_interior(t.point, t.stub, &r.expanded(m)))
}

fn bbox(points: &[Point]) -> Rect {
    let mut r = Rect { min: Point::new(f64::INFINITY, f64::INFINITY), max: Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY) };
    for p in points {
        r.min.x = r.min.x.min(p.x);
        r.min.y = r.min.y.min(p.y);
        r.max.x = r.max.x.max(p.x);
        r.max.y = r.max.y.max(p.y);
    }
    r
}

fn union(a: &Rect, b: &Rect) -> Rect {
    Rect { min: Point::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)), max: Point::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)) }
}

/// One search at margin `m` inside the region for `level` (0 and 1 are windows around the
/// terminals, 2 covers every obstacle). A route confined to the region cannot touch obstacles
/// outside it, so ignoring those is sound and keeps grids small for local connectors.
fn attempt(obstacles: &[Option<Rect>], req: &Request, opts: &Opts, existing: &[(Point, Point)], m: f64, level: u8) -> Result<Vec<Point>, Miss> {
    let a = terminal(&req.start, obstacles, m);
    let b = terminal(&req.end, obstacles, m);
    if !stub_clear(&a, obstacles, m) || !stub_clear(&b, obstacles, m) {
        return Err(Miss::NoPath);
    }
    let terms = bbox(&[a.point, a.stub, b.point, b.stub]);
    let span = (terms.max.x - terms.min.x).max(terms.max.y - terms.min.y);
    let expanded: Vec<Rect> = obstacles.iter().filter_map(|r| r.map(|r| r.expanded(m))).collect();
    let region = match level {
        0 => terms.expanded(span * 0.5 + 4.0 * m + 20.0),
        1 => terms.expanded(span * 2.0 + 8.0 * m + 100.0),
        _ => expanded.iter().fold(terms, |acc, r| union(&acc, r)).expanded(m + 1.0),
    };
    let inside: Vec<Rect> = expanded
        .into_iter()
        .filter(|r| r.min.x < region.max.x && r.max.x > region.min.x && r.min.y < region.max.y && r.max.y > region.min.y)
        .collect();

    let mut xs = vec![region.min.x, region.max.x, a.point.x, a.stub.x, b.point.x, b.stub.x];
    let mut ys = vec![region.min.y, region.max.y, a.point.y, a.stub.y, b.point.y, b.stub.y];
    for r in &inside {
        xs.extend([r.min.x, r.max.x].into_iter().filter(|&x| x > region.min.x && x < region.max.x));
        ys.extend([r.min.y, r.max.y].into_iter().filter(|&y| y > region.min.y && y < region.max.y));
    }
    for v in [&mut xs, &mut ys] {
        v.sort_by(f64::total_cmp);
        v.dedup_by(|p, q| p.total_cmp(q).is_eq());
    }
    if xs.len().saturating_mul(ys.len()) > opts.max_grid {
        return Err(Miss::TooBig);
    }
    let grid = build_grid(xs, ys, &inside, existing, opts.crossing > 0.0);
    let ends = Ends {
        start: grid.node_at(a.stub).ok_or(Miss::NoPath)?,
        start_dir: a.dir,
        end: grid.node_at(b.stub).ok_or(Miss::NoPath)?,
        // The route arrives heading into the shape, the opposite of the side's outward heading.
        end_dir: b.dir.map(|d| d ^ 1),
    };
    let nodes = search::search(&grid, &ends, opts.bend, opts.crossing).ok_or(Miss::NoPath)?;
    let mut points = Vec::with_capacity(nodes.len() + 2);
    points.push(a.point);
    points.extend(nodes.iter().filter_map(|&n| grid.point(n)));
    points.push(b.point);
    Ok(geom::simplify(&points))
}

/// Marks index rectangles in a 2D difference array; prefix sums then give per-cell cover counts in
/// time linear in the grid size, however large or overlapping the obstacles are.
struct Cover {
    w: usize,
    h: usize,
    diff: Vec<i32>,
}

impl Cover {
    fn new(w: usize, h: usize) -> Self {
        Self { w, h, diff: vec![0; (w + 1).saturating_mul(h + 1)] }
    }

    /// Covers columns `i0..i1` and rows `j0..j1`.
    fn add(&mut self, i0: usize, i1: usize, j0: usize, j1: usize) {
        if i0 >= i1 || j0 >= j1 {
            return;
        }
        let s = self.w + 1;
        for (i, j, v) in [(i0, j0, 1), (i1, j0, -1), (i0, j1, -1), (i1, j1, 1)] {
            if let Some(c) = self.diff.get_mut(j * s + i) {
                *c += v;
            }
        }
    }

    fn blocked(mut self) -> Vec<bool> {
        let s = self.w + 1;
        for j in 0..=self.h {
            for i in 0..=self.w {
                let left = if i > 0 { self.diff.get(j * s + i - 1).copied().unwrap_or(0) } else { 0 };
                let up = if j > 0 { self.diff.get((j - 1) * s + i).copied().unwrap_or(0) } else { 0 };
                let diag = if i > 0 && j > 0 { self.diff.get((j - 1) * s + i - 1).copied().unwrap_or(0) } else { 0 };
                if let Some(c) = self.diff.get_mut(j * s + i) {
                    *c += left + up - diag;
                }
            }
        }
        let mut out = Vec::with_capacity(self.w.saturating_mul(self.h));
        for j in 0..self.h {
            for i in 0..self.w {
                out.push(self.diff.get(j * s + i).is_some_and(|&c| c > 0));
            }
        }
        out
    }
}

/// Indices of the sorted values lying strictly between `lo` and `hi`.
fn strictly_inside(values: &[f64], lo: f64, hi: f64) -> (usize, usize) {
    (values.partition_point(|&v| v <= lo), values.partition_point(|&v| v < hi))
}

fn midpoints(v: &[f64]) -> Vec<f64> {
    v.windows(2).filter_map(|w| if let [a, b] = w { Some(a / 2.0 + b / 2.0) } else { None }).collect()
}

fn build_grid(xs: Vec<f64>, ys: Vec<f64>, obstacles: &[Rect], existing: &[(Point, Point)], crossings: bool) -> Grid {
    let (nx, ny) = (xs.len(), ys.len());
    let (mx, my) = (midpoints(&xs), midpoints(&ys));
    let mut nodes = Cover::new(nx, ny);
    let mut hs = Cover::new(mx.len(), ny);
    let mut vs = Cover::new(nx, my.len());
    // Grid lines include every obstacle edge, so an edge is blocked exactly when its midpoint is
    // strictly inside an obstacle, and a node when it is.
    for r in obstacles {
        let (xi, yj) = (strictly_inside(&xs, r.min.x, r.max.x), strictly_inside(&ys, r.min.y, r.max.y));
        let (mxi, myj) = (strictly_inside(&mx, r.min.x, r.max.x), strictly_inside(&my, r.min.y, r.max.y));
        nodes.add(xi.0, xi.1, yj.0, yj.1);
        hs.add(mxi.0, mxi.1, yj.0, yj.1);
        vs.add(xi.0, xi.1, myj.0, myj.1);
    }
    let mut h_cross = Vec::new();
    let mut v_cross = Vec::new();
    if crossings {
        h_cross = vec![0u32; mx.len().saturating_mul(ny)];
        v_cross = vec![0u32; nx.saturating_mul(my.len())];
        for &(a, b) in existing {
            if geom::near(a.x, b.x) && !geom::near(a.y, b.y) {
                // A vertical segment crosses the horizontal edges of the column pair around its x,
                // counted on the edge whose right end is at or past it so a straight pass counts once.
                let (lo, hi) = (a.y.min(b.y), a.y.max(b.y));
                let Some(i) = xs.partition_point(|&x| x < a.x).checked_sub(1) else { continue };
                if i + 1 >= nx {
                    continue;
                }
                let (j0, j1) = strictly_inside(&ys, lo, hi);
                for j in j0..j1 {
                    if let Some(c) = h_cross.get_mut(j * (nx - 1) + i) {
                        *c = c.saturating_add(1);
                    }
                }
            } else if geom::near(a.y, b.y) && !geom::near(a.x, b.x) {
                let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
                let Some(j) = ys.partition_point(|&y| y < a.y).checked_sub(1) else { continue };
                if j + 1 >= ny {
                    continue;
                }
                let (i0, i1) = strictly_inside(&xs, lo, hi);
                for i in i0..i1 {
                    if let Some(c) = v_cross.get_mut(j * nx + i) {
                        *c = c.saturating_add(1);
                    }
                }
            }
        }
    }
    Grid { xs, ys, node_blocked: nodes.blocked(), h_blocked: hs.blocked(), v_blocked: vs.blocked(), h_cross, v_cross }
}
