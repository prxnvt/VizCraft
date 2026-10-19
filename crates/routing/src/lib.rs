#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Model-independent connector routing for diagrams: orthogonal routes that go around shapes,
//! nudging of overlapping parallel segments (and fanning out of ends that share a side), the
//! crossing oracle ([`crossings`]), clear rounded bends for curved lines ([`round_bends`]), and
//! line-jump positions where connectors cross.
//!
//! The crate knows nothing about pages, shapes or styles. Callers describe shapes as axis-aligned
//! [`Obstacle`] rectangles (rotated shapes as their bounding boxes) and connectors as [`Request`]s
//! between two [`End`]s, and get back plain polylines.
//!
//! Coordinates follow screen conventions: x grows to the right and y grows downward, so
//! [`Side::Top`] is the side with the smaller y.
//!
//! The router builds a sparse orthogonal visibility grid from the obstacle edges (expanded by the
//! clearance margin) and the connector ends, then runs an A* search over (grid point, heading)
//! states that minimises length plus a penalty per bend. An end may offer several candidate points
//! (every connection point of a dynamically glued shape): the search starts from all of them and
//! finishes at any of the other end's, so the cheapest clear route also picks the sides. Every
//! entry point accepts hostile input:
//! non-finite or absurd coordinates, inverted rectangles and oversized problems produce a simple
//! fallback route (marked [`Route::fallback`]) instead of a panic or an unbounded computation.

mod curve;
mod geom;
mod jumps;
mod nudge;
mod route;
mod search;

use serde::{Deserialize, Serialize};

/// A point in diagram coordinates (x to the right, y downward).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate, growing downward.
    pub y: f64,
}

impl Point {
    /// Creates a point.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Whether both coordinates are finite numbers.
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// An axis-aligned rectangle given by its minimum and maximum corners.
///
/// Rectangles built by hand may be inverted (min greater than max); every routine normalises
/// them before use, so callers do not have to.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Corner with the smallest coordinates (top left).
    pub min: Point,
    /// Corner with the largest coordinates (bottom right).
    pub max: Point,
}

impl Rect {
    /// Creates a rectangle from two opposite corners in any order.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { min: Point::new(x0, y0), max: Point::new(x1, y1) }.normalized()
    }

    /// Returns the same rectangle with min and max swapped where they were inverted.
    pub fn normalized(&self) -> Self {
        Self {
            min: Point::new(self.min.x.min(self.max.x), self.min.y.min(self.max.y)),
            max: Point::new(self.min.x.max(self.max.x), self.min.y.max(self.max.y)),
        }
    }

    /// Returns the rectangle grown by `by` on every side.
    pub fn expanded(&self, by: f64) -> Self {
        Self { min: Point::new(self.min.x - by, self.min.y - by), max: Point::new(self.max.x + by, self.max.y + by) }
    }

    /// Whether all four coordinates are finite numbers.
    pub fn is_finite(&self) -> bool {
        self.min.is_finite() && self.max.is_finite()
    }
}

/// One side of a shape, naming the direction a connector leaves or enters it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    /// The side with the smallest x: the connector leaves toward negative x.
    Left,
    /// The side with the largest x: the connector leaves toward positive x.
    Right,
    /// The side with the smallest y: the connector leaves upward (negative y).
    Top,
    /// The side with the largest y: the connector leaves downward (positive y).
    Bottom,
}

/// A shape the router must keep clear of.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Obstacle {
    /// The shape's axis-aligned extent; rotated shapes pass their bounding box.
    pub rect: Rect,
}

impl Obstacle {
    /// Creates an obstacle from two opposite corners in any order.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { rect: Rect::new(x0, y0, x1, y1) }
    }
}

/// One end of a connector.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct End {
    /// Where the connector starts or finishes. With [`End::candidates`] set, this is only the
    /// position the fallback route uses.
    pub point: Point,
    /// The direction the connector must leave (at the start) or enter (at the end) through, or
    /// `None` for any direction. When the end is glued and this is `None`, the side of the glued
    /// shape nearest to `point` is used.
    pub side: Option<Side>,
    /// Index into the obstacle list of the shape this end is glued to. The route may cross that
    /// obstacle only on its first (or last) segment, the stub that leaves the shape. An index out
    /// of range is treated as `None`.
    pub obstacle: Option<usize>,
    /// Alternative places the end may use, each leaving (or entering) through its side: every
    /// connection point of a dynamically glued shape. When this is not empty the search starts
    /// (or finishes) at whichever candidate gives the cheapest clear route, and [`Route`] reports
    /// which one it chose; `point` and `side` then serve only the fallback. At most
    /// [`MAX_CANDIDATES`] are used; candidates with unusable coordinates are skipped.
    pub candidates: Vec<(Point, Side)>,
    /// How far nudging may slide this end along its side to spread connectors that leave the same
    /// side apart; zero (the default) keeps the end where the route put it. The slide never takes
    /// the end past the glued obstacle's side.
    pub spread: f64,
    /// How far the end may slide along each side (left, right, top, bottom), for a shape whose
    /// sides allow different slides (a long bar, an outline straight on some sides only); `None`
    /// uses [`End::spread`] on every side. The side is the one the route leaves (or enters)
    /// through.
    pub side_spread: Option<[f64; 4]>,
    /// The length of the segment that leaves (or enters) the shape, for an end whose arrowhead
    /// needs more room than [`Options::margin`] gives; `None` uses the margin. A value below the
    /// margin is raised to it, so the search still starts outside the expanded obstacles.
    pub stub: Option<f64>,
}

/// Most candidates one end offers to the search; more are ignored.
pub const MAX_CANDIDATES: usize = 64;

impl End {
    /// A free end at `point` that may leave in any direction.
    pub fn at(point: Point) -> Self {
        Self { point, ..Self::default() }
    }

    /// Returns this end with a required side.
    pub fn with_side(mut self, side: Side) -> Self {
        self.side = Some(side);
        self
    }

    /// Returns this end glued to obstacle `index`.
    pub fn glued_to(mut self, index: usize) -> Self {
        self.obstacle = Some(index);
        self
    }

    /// Returns this end with candidate positions for the search to choose from.
    pub fn with_candidates(mut self, candidates: Vec<(Point, Side)>) -> Self {
        self.candidates = candidates;
        self
    }

    /// Returns this end allowed to slide `by` along its side when nudging spreads connectors.
    pub fn with_spread(mut self, by: f64) -> Self {
        self.spread = by;
        self
    }

    /// Returns this end allowed to slide a different amount along each side (left, right, top,
    /// bottom).
    pub fn with_side_spread(mut self, by: [f64; 4]) -> Self {
        self.side_spread = Some(by);
        self
    }

    /// How far the end may slide along `side` (see [`End::side_spread`]).
    pub fn spread_on(&self, side: Option<Side>) -> f64 {
        match (self.side_spread, side) {
            (Some(by), Some(side)) => {
                let i = match side {
                    Side::Left => 0,
                    Side::Right => 1,
                    Side::Top => 2,
                    Side::Bottom => 3,
                };
                by.get(i).copied().unwrap_or(0.0)
            }
            (Some(_), None) => 0.0,
            (None, _) => self.spread,
        }
    }
}

/// A connector to route, from `start` to `end`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Request {
    /// Where the connector begins.
    pub start: End,
    /// Where the connector finishes.
    pub end: End,
    /// Indices of obstacles this connector may pass through, such as a container that encloses
    /// one of its ends. Out-of-range indices are ignored.
    pub ignore: Vec<usize>,
    /// Width and height of the label drawn at the middle of the route (half its length along
    /// it), if any. Nudging keeps the parallel neighbours of the segment holding it far enough
    /// apart that the label does not lie on them, nor on a label of theirs.
    pub label: Option<(f64, f64)>,
}

impl Request {
    /// Creates a request between two ends.
    pub fn new(start: End, end: End) -> Self {
        Self { start, end, ignore: Vec::new(), label: None }
    }

    /// Returns this request allowed to pass through the obstacles in `ignore`.
    pub fn ignoring(mut self, ignore: Vec<usize>) -> Self {
        self.ignore = ignore;
        self
    }

    /// Returns this request with a label of `width` by `height` at the middle of its route.
    pub fn labelled(mut self, width: f64, height: f64) -> Self {
        self.label = Some((width, height));
        self
    }
}

/// Tuning knobs for the router. Non-finite or negative values are replaced by the defaults.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Clearance kept between routes and obstacles, and the length of the stub that leaves a side.
    pub margin: f64,
    /// Cost of one bend, in the same units as length.
    pub bend_penalty: f64,
    /// Extra cost per crossing with a connector routed earlier by [`route_all`]; `None` ignores
    /// crossings while searching.
    pub crossing_penalty: Option<f64>,
    /// Extra cost per unit of length a route runs on top of a connector routed earlier by
    /// [`route_all`] (on the same line, overlapping); `None` lets routes share runs, leaving
    /// them to nudging.
    #[serde(default)]
    pub overlap_penalty: Option<f64>,
    /// Distance between parallel segments that [`route_all`] pushes apart.
    pub segment_spacing: f64,
    /// Largest number of grid points one search may use; bigger problems fall back.
    pub max_grid: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { margin: 10.0, bend_penalty: 20.0, crossing_penalty: None, overlap_penalty: None, segment_spacing: 6.0, max_grid: 200_000 }
    }
}

/// A routed connector.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Route {
    /// The polyline from start to end. Normal routes are axis-aligned, with no zero-length
    /// segments and no collinear interior points. A start equal to the end gives one point; an end
    /// with non-finite coordinates gives only the finite ends (possibly none).
    pub points: Vec<Point>,
    /// True when no obstacle-avoiding route was found (or the input was unusable) and `points` is
    /// a simple orthogonal route that may cross obstacles.
    pub fallback: bool,
    /// Index into the start's [`End::candidates`] of the candidate the route leaves from, or
    /// `None` when the start has no candidates or the route is a fallback.
    pub start_candidate: Option<usize>,
    /// Index into the end's [`End::candidates`] of the candidate the route arrives at, or `None`
    /// when the end has no candidates or the route is a fallback.
    pub end_candidate: Option<usize>,
}

/// Which connector draws the hop where two connectors cross.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum JumpOrder {
    /// A connector hops over every connector with a smaller index.
    #[default]
    LaterOverEarlier,
    /// A connector hops over every connector with a larger index.
    EarlierOverLater,
}

/// A place where a polyline should draw a hop over another one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Jump {
    /// Index of the segment (from point `segment` to point `segment + 1`) that hops.
    pub segment: usize,
    /// Distance from the segment's first point to the crossing.
    pub distance: f64,
    /// The crossing point.
    pub point: Point,
    /// Index of the polyline being hopped over.
    pub over: usize,
}

/// Routes one connector orthogonally around the obstacles.
///
/// The route keeps `options.margin` clear of every obstacle except the ones its ends are glued to
/// (which it crosses only on its stub) and the ones the request ignores, honours the side
/// directions of both ends (or picks the cheapest candidates) and minimises length plus
/// `bend_penalty` per bend. When no such route exists or the problem is too large or malformed,
/// it returns a three-segment orthogonal route marked `fallback`.
pub fn route_orthogonal(obstacles: &[Obstacle], request: &Request, options: &Options) -> Route {
    let opts = geom::Opts::from(options);
    let obs = geom::prepare(obstacles);
    route::route_one(&obs, request, &opts, &[]).route
}

/// Routes several connectors, then pushes overlapping parallel segments apart.
///
/// Connectors are routed in order; with `crossing_penalty` set, each one avoids crossing the ones
/// before it where that is cheap. Afterwards segments of different connectors that lie on the
/// same line and overlap are spread `segment_spacing` apart (see [`nudge_routes`]).
pub fn route_all(obstacles: &[Obstacle], requests: &[Request], options: &Options) -> Vec<Route> {
    route_all_around(obstacles, requests, options, &[])
}

/// [`route_all`] with `fixed` lines already drawn (connectors routed elsewhere, kept as they
/// are): with `crossing_penalty` or `overlap_penalty` set, the routes avoid crossing or running
/// on them as they avoid each other. Non-finite points in `fixed` are skipped.
pub fn route_all_around(obstacles: &[Obstacle], requests: &[Request], options: &Options, fixed: &[Vec<Point>]) -> Vec<Route> {
    let opts = geom::Opts::from(options);
    let obs = geom::prepare(obstacles);
    let mut routed: Vec<route::Routed> = Vec::with_capacity(requests.len());
    let mut existing: Vec<(Point, Point)> = Vec::new();
    if opts.crossing > 0.0 || opts.overlap > 0.0 {
        for line in fixed {
            existing.extend(line.windows(2).filter_map(|w| match w {
                [a, b] if geom::point_ok(*a) && geom::point_ok(*b) => Some((*a, *b)),
                _ => None,
            }));
        }
    }
    for req in requests {
        let r = route::route_one(&obs, req, &opts, &existing);
        if (opts.crossing > 0.0 || opts.overlap > 0.0) && !r.route.fallback {
            existing.extend(r.route.points.windows(2).filter_map(|w| match w {
                [a, b] => Some((*a, *b)),
                _ => None,
            }));
        }
        routed.push(r);
    }
    if opts.spacing > 0.0 {
        nudge::nudge(&mut routed, &obs, opts.spacing);
    }
    routed.into_iter().map(|r| r.route).collect()
}

/// Pushes overlapping parallel segments of `routes` apart in place, as [`route_all`] does after
/// routing; `requests[i]` describes `routes[i]` (its ignored obstacles, glued shapes and how far
/// its ends may slide). Use it to spread routes that came from elsewhere, or a mix of routed and
/// precomputed ones, in one pass.
///
/// Interior segments (both ends are bends) move freely within the channel that stays
/// `options.margin` clear of the obstacles. First and last segments carry the ends: they move only
/// when their end has a [`End::spread`], sliding the end along its side by at most that much, so
/// connectors that leave the same side fan out instead of running on top of each other; otherwise
/// they stay put and interior segments lying on them move aside. Fallback routes and routes
/// without a request are left alone.
pub fn nudge_routes(obstacles: &[Obstacle], requests: &[Request], routes: &mut [Route], options: &Options) {
    let opts = geom::Opts::from(options);
    if opts.spacing <= 0.0 {
        return;
    }
    let obs = geom::prepare(obstacles);
    let mut routed: Vec<route::Routed> = routes
        .iter()
        .enumerate()
        .map(|(i, r)| match requests.get(i) {
            Some(req) if !r.fallback && r.points.iter().all(|p| geom::point_ok(*p)) => route::Routed::preset(r.clone(), req, &obs, opts.margin),
            _ => route::Routed { route: r.clone(), margin: None, ..route::Routed::default() },
        })
        .collect();
    nudge::nudge(&mut routed, &obs, opts.spacing);
    for (dst, src) in routes.iter_mut().zip(routed) {
        *dst = src.route;
    }
}

/// For each polyline, the crossings where it should draw a hop over another polyline.
///
/// Only proper crossings count: two segments that cross in both of their interiors. Touching at
/// shared endpoints, T-junctions, crossings exactly at a bend and collinear overlaps are ignored.
/// Each list is sorted by segment and distance along it. Very large inputs are checked up to a
/// fixed work budget, so the result for them may be incomplete.
pub fn jumps(polylines: &[Vec<Point>], order: JumpOrder) -> Vec<Vec<Jump>> {
    jumps::jumps(polylines, order)
}

/// Whether every segment of `route` stays out of the interior of every obstacle grown by `margin`,
/// except the obstacles whose indices are in `ignore`. Touching an expanded boundary is allowed.
/// A route with non-finite points is never clear.
pub fn clear_of_obstacles(route: &Route, obstacles: &[Obstacle], ignore: &[usize], margin: f64) -> bool {
    let margin = if margin.is_finite() && margin > 0.0 { margin } else { 0.0 };
    if route.points.iter().any(|p| !p.is_finite()) {
        return false;
    }
    let segments: Vec<(Point, Point)> = match route.points.as_slice() {
        [] => return true,
        [p] => vec![(*p, *p)],
        pts => pts.windows(2).filter_map(|w| if let [a, b] = w { Some((*a, *b)) } else { None }).collect(),
    };
    obstacles.iter().enumerate().filter(|(k, o)| !ignore.contains(k) && o.rect.is_finite()).all(|(_, o)| {
        let r = o.rect.normalized().expanded(margin);
        segments.iter().all(|(a, b)| !geom::segment_hits_interior(*a, *b, &r))
    })
}

/// One place where a polyline enters an obstacle, from [`crossings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hit {
    /// Index of the obstacle entered.
    pub obstacle: usize,
    /// Index of the segment (from point `segment` to point `segment + 1`) that enters it.
    pub segment: usize,
}

/// How far a polyline may sink into its own end obstacles' outlines before [`crossings`] counts
/// it, so a line that starts on a shape's edge is not reported for touching it.
pub const OWN_TOLERANCE: f64 = 0.5;

/// Every place where the polyline `points` enters an obstacle: the oracle for "this connector
/// crosses a shape". Each obstacle is grown by `clearance` first (a negative clearance shrinks it,
/// a tolerance for drawing error). Obstacles in `ignore` never count.
///
/// `start_own` and `end_own` name the obstacles the polyline's ends belong to. Those are checked
/// shrunk by [`OWN_TOLERANCE`] whatever the clearance, and the stub inside them is allowed: the
/// leading segments whose points all lie strictly inside the start's obstacle (a connection
/// point inside a rotated shape's bounding box, say), and likewise the trailing ones of the end's.
/// Once the line is out, it must not come back in. Segments with non-finite points are skipped.
/// The result is sorted by segment, then obstacle.
pub fn crossings(
    points: &[Point],
    obstacles: &[Obstacle],
    ignore: &[usize],
    start_own: Option<usize>,
    end_own: Option<usize>,
    clearance: f64,
) -> Vec<Hit> {
    geom::crossings(points, obstacles, ignore, start_own, end_own, clearance)
}

/// A rounded bend from [`round_bends`]: the cubic from `from` (on the incoming segment) to `to`
/// (on the outgoing one) with control points `c1` and `c2`, drawn instead of the sharp corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bend {
    /// Where the curve leaves the incoming segment.
    pub from: Point,
    /// First control point.
    pub c1: Point,
    /// Second control point.
    pub c2: Point,
    /// Where the curve joins the outgoing segment.
    pub to: Point,
}

/// Rounds the bends of a polyline with the largest circular arc that stays clear: for each
/// interior point of `points` (index 1 to `len - 2`), `Some(bend)` with the arc to draw, or `None`
/// to keep the corner sharp (no room, a straight run or a reversal).
///
/// An arc may use up to half of each neighbouring interior segment and three quarters of the
/// first and last segments, so arcs never overlap and the line still leaves its ends straight.
/// Each arc is checked by sampling it against the obstacles grown by `clearance`, except those in
/// `ignore`; one that hits is shrunk until it clears or becomes too small to see.
pub fn round_bends(points: &[Point], obstacles: &[Obstacle], ignore: &[usize], clearance: f64) -> Vec<Option<Bend>> {
    curve::round_bends(points, obstacles, ignore, clearance)
}
