//! Routing one connector: terminals and stubs, the search region, grid construction and fallback.

use crate::geom::{self, Opts};
use crate::search::{self, DOWN, Grid, LEFT, RIGHT, Source, Target, UP};
use crate::{End, MAX_CANDIDATES, Point, Rect, Request, Route, Side};

/// How far one end of a routed connector may slide along its side when nudging spreads
/// connectors apart.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Slide {
    /// Largest slide either way.
    pub limit: f64,
    /// The glued obstacle the end sits on: the slide stays within its side, and the stub may
    /// overlap it.
    pub rect: Option<Rect>,
    /// That obstacle's index, which nudging must not treat as blocking the stub.
    pub index: Option<usize>,
}

/// A routed connector plus what nudging needs to know about it.
#[derive(Default)]
pub(crate) struct Routed {
    pub route: Route,
    /// The clearance the route was found with (it may be smaller than requested after a retry),
    /// or `None` for a fallback route, which nudging leaves alone.
    pub margin: Option<f64>,
    /// Obstacles the route may pass through.
    pub ignore: Vec<usize>,
    /// How the first point may slide, if at all.
    pub start_slide: Option<Slide>,
    /// How the last point may slide, if at all.
    pub end_slide: Option<Slide>,
    /// The size of the label at the middle of the route, if any.
    pub label: Option<(f64, f64)>,
}

/// The side a route leaves its end `at` through, from the direction of its segment from `at` to
/// `next` (`None` for a segment that is not axis-aligned or has no length).
fn side_toward(at: Point, next: Point) -> Option<Side> {
    let (dx, dy) = (next.x - at.x, next.y - at.y);
    if geom::near(dy, 0.0) && !geom::near(dx, 0.0) {
        Some(if dx > 0.0 { Side::Right } else { Side::Left })
    } else if geom::near(dx, 0.0) && !geom::near(dy, 0.0) {
        Some(if dy > 0.0 { Side::Bottom } else { Side::Top })
    } else {
        None
    }
}

/// The slide an end allows along `side`, given the rectangle of the obstacle it is glued to.
fn slide_of(end: &End, obstacles: &[Option<Rect>], side: Option<Side>) -> Option<Slide> {
    let spread = end.spread_on(side);
    let limit = if spread.is_finite() { spread.clamp(0.0, geom::COORD_LIMIT) } else { 0.0 };
    if limit <= 0.0 {
        return None;
    }
    let rect = end.obstacle.and_then(|k| obstacles.get(k).copied().flatten());
    Some(Slide { limit, rect, index: rect.and(end.obstacle) })
}

impl Routed {
    /// A route made elsewhere, described by `req`, for nudging; `margin` is the clearance assumed.
    pub fn preset(route: Route, req: &Request, obstacles: &[Option<Rect>], margin: f64) -> Self {
        Self::with(route, Some(margin), req, obstacles)
    }

    fn with(route: Route, margin: Option<f64>, req: &Request, obstacles: &[Option<Rect>]) -> Self {
        let pts = &route.points;
        let first = match pts.as_slice() {
            [a, b, ..] => side_toward(*a, *b),
            _ => None,
        };
        let last = match pts.as_slice() {
            [.., b, a] => side_toward(*a, *b),
            _ => None,
        };
        let (start_slide, end_slide) =
            if margin.is_some() { (slide_of(&req.start, obstacles, first), slide_of(&req.end, obstacles, last)) } else { (None, None) };
        let label = req
            .label
            .filter(|(w, h)| w.is_finite() && h.is_finite() && *w >= 0.0 && *h >= 0.0)
            .map(|(w, h)| (w.min(geom::COORD_LIMIT), h.min(geom::COORD_LIMIT)));
        Self { route, margin, ignore: req.ignore.clone(), start_slide, end_slide, label }
    }
}

/// Least distance between an earlier run and the lane offered beside it.
const MIN_LANE: f64 = 6.0;

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
    /// Length of the stub, which the search counts as already travelled.
    cost: f64,
    /// Which of the end's candidates this is, if it came from one.
    index: Option<usize>,
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

fn terminal_at(p: Point, side: Option<Side>, glued_index: Option<usize>, obstacles: &[Option<Rect>], margin: f64, index: Option<usize>) -> Terminal {
    let glued_rect = glued_index.and_then(|k| obstacles.get(k).copied().flatten());
    let glued = glued_rect.and(glued_index);
    let side = side.or_else(|| glued_rect.map(|r| nearest_side(p, &r)));
    // The stub reaches one margin beyond the glued shape even when the point sits inside it, so
    // the grid search always starts outside every expanded obstacle.
    let stub = match (side, glued_rect) {
        (None, _) => p,
        (Some(Side::Left), g) => Point::new(g.map_or(p.x - margin, |g| (p.x - margin).min(g.min.x - margin)), p.y),
        (Some(Side::Right), g) => Point::new(g.map_or(p.x + margin, |g| (p.x + margin).max(g.max.x + margin)), p.y),
        (Some(Side::Top), g) => Point::new(p.x, g.map_or(p.y - margin, |g| (p.y - margin).min(g.min.y - margin))),
        (Some(Side::Bottom), g) => Point::new(p.x, g.map_or(p.y + margin, |g| (p.y + margin).max(g.max.y + margin))),
    };
    let cost = (stub.x - p.x).abs() + (stub.y - p.y).abs();
    Terminal { point: p, stub, dir: side.map(side_dir), glued, cost, index }
}

/// Every place an end may start or finish: its candidates, or its one point.
fn terminals(end: &End, obstacles: &[Option<Rect>], margin: f64) -> Vec<Terminal> {
    // An end's own stub length (an arrowhead's trim) wins when it is longer than the clearance margin.
    let margin = end.stub.filter(|s| s.is_finite()).map_or(margin, |s| s.max(margin));
    if end.candidates.is_empty() {
        return vec![terminal_at(end.point, end.side, end.obstacle, obstacles, margin, None)];
    }
    end.candidates
        .iter()
        .take(MAX_CANDIDATES)
        .enumerate()
        .filter(|(_, (p, _))| geom::point_ok(*p))
        .map(|(i, (p, side))| terminal_at(*p, Some(*side), end.obstacle, obstacles, margin, Some(i)))
        .collect()
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
    Route { points: geom::simplify(&points), fallback: true, ..Route::default() }
}

/// Routes one request. `existing` holds segments of connectors routed before, used only when
/// crossings are penalised.
pub(crate) fn route_one(obstacles: &[Option<Rect>], req: &Request, opts: &Opts, existing: &[(Point, Point)]) -> Routed {
    let (s, e) = (req.start.point, req.end.point);
    let fail = |route: Route| Routed { route, margin: None, ignore: req.ignore.clone(), ..Routed::default() };
    if !s.is_finite() || !e.is_finite() {
        let points = [s, e].into_iter().filter(Point::is_finite).collect();
        return fail(Route { points, fallback: true, ..Route::default() });
    }
    if !geom::point_ok(s) || !geom::point_ok(e) || obstacles.len() > geom::MAX_OBSTACLES {
        return fail(fallback(&req.start, &req.end));
    }
    if req.start.candidates.is_empty() && req.end.candidates.is_empty() && geom::same_point(s, e) {
        return Routed::with(Route { points: vec![s], ..Route::default() }, Some(opts.margin), req, obstacles);
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
                Ok((points, start_candidate, end_candidate)) => {
                    let route = Route { points, fallback: false, start_candidate, end_candidate };
                    return Routed::with(route, Some(m), req, obstacles);
                }
                Err(Miss::NoPath) => {}
                Err(Miss::TooBig) => break,
            }
        }
    }
    fail(fallback(&req.start, &req.end))
}

/// Whether a stub segment avoids every expanded obstacle except the one its end is glued to.
fn stub_clear(t: &Terminal, obstacles: &[Option<Rect>], ignore: &[usize], m: f64) -> bool {
    if geom::same_point(t.point, t.stub) {
        return true;
    }
    obstacles
        .iter()
        .enumerate()
        .filter(|(k, _)| Some(*k) != t.glued && !ignore.contains(k))
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

/// A polyline and the candidate indices of the start and end it joins.
type Found = (Vec<Point>, Option<usize>, Option<usize>);

/// One search at margin `m` inside the region for `level` (0 and 1 are windows around the
/// terminals, 2 covers every obstacle). A route confined to the region cannot touch obstacles
/// outside it, so ignoring those is sound and keeps grids small for local connectors.
fn attempt(obstacles: &[Option<Rect>], req: &Request, opts: &Opts, existing: &[(Point, Point)], m: f64, level: u8) -> Result<Found, Miss> {
    let ignore = &req.ignore;
    let starts: Vec<Terminal> = terminals(&req.start, obstacles, m).into_iter().filter(|t| stub_clear(t, obstacles, ignore, m)).collect();
    let ends: Vec<Terminal> = terminals(&req.end, obstacles, m).into_iter().filter(|t| stub_clear(t, obstacles, ignore, m)).collect();
    if starts.is_empty() || ends.is_empty() {
        return Err(Miss::NoPath);
    }
    let all: Vec<Point> = starts.iter().chain(&ends).flat_map(|t| [t.point, t.stub]).collect();
    let terms = bbox(&all);
    let span = (terms.max.x - terms.min.x).max(terms.max.y - terms.min.y);
    let expanded: Vec<Rect> =
        obstacles.iter().enumerate().filter(|(k, _)| !ignore.contains(k)).filter_map(|(_, r)| r.map(|r| r.expanded(m))).collect();
    let region = match level {
        0 => terms.expanded(span * 0.5 + 4.0 * m + 20.0),
        1 => terms.expanded(span * 2.0 + 8.0 * m + 100.0),
        _ => expanded.iter().fold(terms, |acc, r| union(&acc, r)).expanded(m + 1.0),
    };
    let inside: Vec<Rect> = expanded
        .into_iter()
        .filter(|r| r.min.x < region.max.x && r.max.x > region.min.x && r.min.y < region.max.y && r.max.y > region.min.y)
        .collect();

    let mut xs = vec![region.min.x, region.max.x];
    let mut ys = vec![region.min.y, region.max.y];
    for p in &all {
        xs.push(p.x);
        ys.push(p.y);
    }
    for r in &inside {
        xs.extend([r.min.x, r.max.x].into_iter().filter(|&x| x > region.min.x && x < region.max.x));
        ys.extend([r.min.y, r.max.y].into_iter().filter(|&y| y > region.min.y && y < region.max.y));
    }
    if opts.overlap > 0.0 {
        // Lanes beside each earlier run, so a route that would lie on one has a parallel line to
        // take instead (an open area offers no other grid line).
        let lane = m.max(MIN_LANE);
        for &(a, b) in existing {
            if geom::near(a.x, b.x) && !geom::near(a.y, b.y) {
                xs.extend([a.x - lane, a.x + lane].into_iter().filter(|&w| w > region.min.x && w < region.max.x));
            } else if geom::near(a.y, b.y) && !geom::near(a.x, b.x) {
                ys.extend([a.y - lane, a.y + lane].into_iter().filter(|&w| w > region.min.y && w < region.max.y));
            }
        }
    }
    for v in [&mut xs, &mut ys] {
        v.sort_by(f64::total_cmp);
        v.dedup_by(|p, q| p.total_cmp(q).is_eq());
    }
    if xs.len().saturating_mul(ys.len()) > opts.max_grid {
        return Err(Miss::TooBig);
    }
    let grid = build_grid(xs, ys, &inside, existing, opts.crossing > 0.0, opts.overlap > 0.0);
    let sources: Vec<Source> = starts.iter().filter_map(|t| Some(Source { node: grid.node_at(t.stub)?, dir: t.dir, cost: t.cost })).collect();
    // The route arrives heading into the shape, the opposite of the side's outward heading.
    let targets: Vec<Target> =
        ends.iter().filter_map(|t| Some(Target { node: grid.node_at(t.stub)?, dir: t.dir.map(|d| d ^ 1), cost: t.cost })).collect();
    if sources.len() != starts.len() || targets.len() != ends.len() {
        return Err(Miss::NoPath);
    }
    let found = search::search(&grid, &sources, &targets, opts.bend, opts.crossing, opts.overlap).ok_or(Miss::NoPath)?;
    let (Some(a), Some(b)) = (starts.get(found.source), ends.get(found.target)) else { return Err(Miss::NoPath) };
    let mut points = Vec::with_capacity(found.nodes.len() + 2);
    points.push(a.point);
    points.extend(found.nodes.iter().filter_map(|&n| grid.point(n)));
    points.push(b.point);
    Ok((geom::simplify(&points), a.index, b.index))
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

fn build_grid(xs: Vec<f64>, ys: Vec<f64>, obstacles: &[Rect], existing: &[(Point, Point)], crossings: bool, overlaps: bool) -> Grid {
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
    // An edge lies on an earlier run when the run is on its grid line and covers its middle.
    let mut h_along = Vec::new();
    let mut v_along = Vec::new();
    if overlaps {
        h_along = vec![false; mx.len().saturating_mul(ny)];
        v_along = vec![false; nx.saturating_mul(my.len())];
        // Lines closer to a run than a lane would read as the same run.
        let close = MIN_LANE - 0.5;
        for &(a, b) in existing {
            if geom::near(a.y, b.y) && !geom::near(a.x, b.x) {
                let (r0, r1) = strictly_inside(&ys, a.y - close, a.y + close);
                let (i0, i1) = strictly_inside(&mx, a.x.min(b.x), a.x.max(b.x));
                for j in r0..r1 {
                    for i in i0..i1 {
                        if let Some(e) = h_along.get_mut(j * mx.len() + i) {
                            *e = true;
                        }
                    }
                }
            } else if geom::near(a.x, b.x) && !geom::near(a.y, b.y) {
                let (c0, c1) = strictly_inside(&xs, a.x - close, a.x + close);
                let (j0, j1) = strictly_inside(&my, a.y.min(b.y), a.y.max(b.y));
                for i in c0..c1 {
                    for j in j0..j1 {
                        if let Some(e) = v_along.get_mut(j * nx + i) {
                            *e = true;
                        }
                    }
                }
            }
        }
    }
    Grid { xs, ys, node_blocked: nodes.blocked(), h_blocked: hs.blocked(), v_blocked: vs.blocked(), h_cross, v_cross, h_along, v_along }
}
