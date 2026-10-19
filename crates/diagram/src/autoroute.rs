//! Connector routes that never cross shapes, built on `vizcraft-routing`.
//!
//! The invariant: no segment of a connector's drawn polyline enters the outline (the bounding
//! box) of a solid shape, except the stub inside its own end shapes. [`crossings`] checks it and is
//! the oracle every test, the drawing rule and the conformance check use.
//!
//! Which connectors are routed is the page policy ([`crate::routing_policy`]): every connector by
//! default, or only those with their own `avoid` flag. A routed connector has one route that
//! every style draws:
//!
//! - When its plain route (the automatic route for its style, with dynamic glue facing the other
//!   end) is already clear, it keeps that route and stores no bends, so a page whose connectors
//!   cross nothing looks and saves exactly as it always did.
//! - Otherwise the router searches once with every connection point of a dynamically glued end as
//!   a candidate, so the cheapest clear route also decides which side the line leaves and enters.
//!   The whole polyline, end points included, is stored in `waypoints`; the stored first and last
//!   points are how the chosen connection points are remembered without turning dynamic glue into
//!   point glue ([`crate::route::resolve_ends`] picks the connection point at them).
//! - A straight connector first tries every pair of connection points, nearest first, and keeps
//!   the first straight line that is clear; with none it draws the orthogonal detour, unless the
//!   page says to keep the straight line (then [`crossings`] reports it).
//! - A curved connector draws the orthogonal polyline with its bends rounded as far as stays
//!   clear (see [`crate::route`]).
//! - A connector with no clear route even after the router's margin retry and wider searches keeps
//!   the router's fallback bends, and [`crossings`] reports it: failure is never silent.
//! - Bends a person or a file put on a connector (they do not start and finish on its ends, as a
//!   stored route does) are kept by [`refresh`] while the line through them stays clear;
//!   [`reroute`] (`layout.route`, `connector.reroute`) replaces them.
//!
//! The obstacle set is membership-aware: a shape that encloses either end (a container, lane set or
//! lane that holds it, or any shape it sits on) is not an obstacle, but a container's heading band
//! still is; lanes of a lane set that encloses an end are not obstacles either; groups never are;
//! shapes on hidden layers are not; the end shapes are, except for the stub that leaves them.
//! Rotated and curved shapes count as their bounding boxes.
//!
//! Rerouting is incremental: [`refresh`] reroutes only connectors that a change can affect, and
//! [`preview`] reroutes the connectors around shapes being dragged with a small search budget.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

use vizcraft_routing::{End as REnd, Obstacle, Options, Point as RPoint, Request, Route as RRoute, Side};

use crate::container::{self, Role};
use crate::geom::{Point, Rect, Vec2};
use crate::kind::ShapeKind;
use crate::model::{Connector, Document, End, Page, Route, Shape};
use crate::route::{EndPoint, resolve_through};
use crate::routing_policy::{StraightFallback, routes};

/// Clearance the router keeps between routes and shapes.
pub const MARGIN: f64 = 10.0;

/// How close a plain or straight route may pass a shape and still be kept instead of rerouted.
const KEEP_CLEARANCE: f64 = 4.0;

/// How deep a drawn line may sink into a shape before [`crossings`] reports it: flattening a
/// curve and rounding coordinates cost a fraction of a unit, which nobody can see.
pub const CROSSING_TOLERANCE: f64 = 0.5;

/// Clearance a rounded bend of a curved connector keeps from shapes; at least the flattening
/// tolerance, so the drawn chords stay out too.
pub const ARC_CLEARANCE: f64 = 0.5;

/// Farthest the nudge pass slides a dynamically glued end along its side to fan connectors out.
pub const FAN_MAX: f64 = 12.0;

/// The grid budget for one search while dragging: small enough for every frame.
pub const PREVIEW_BUDGET: usize = 20_000;

/// Deepest parent chain followed when deciding whether a shape encloses another.
const MAX_DEPTH: usize = 64;

/// Most candidate pairs a straight connector tries.
const MAX_PAIRS: usize = 4096;

/// One place where a connector's drawn line enters a shape it must stay out of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crossing {
    /// The connector's id.
    pub connector: String,
    /// The id of the shape entered (for a heading band, the container's id).
    pub shape: String,
    /// The index of the segment of the connector's drawn polyline
    /// ([`crate::route::route_polyline`]) that enters it.
    pub segment: usize,
}

// ---------------------------------------------------------------------------------------------
// The obstacle field

/// The page's solid shapes as router obstacles: one per visible non-group shape (its bounds), plus
/// one per container heading band.
pub(crate) struct Field<'a> {
    pub obstacles: Vec<Obstacle>,
    /// The shape each obstacle belongs to.
    owner: Vec<&'a Shape>,
    /// Whether each obstacle is a heading band (else the shape's body).
    band: Vec<bool>,
    /// Every shape by id, for parent chains.
    by_id: HashMap<&'a str, &'a Shape>,
    /// Every shape's bounds by id, worked out once.
    bounds: HashMap<&'a str, Rect>,
}

/// What routing needs to know about one connector in a field: the obstacles it may pass through
/// and the obstacles of its own end shapes.
pub(crate) struct Ctx {
    pub ignore: Vec<usize>,
    pub start_own: Option<usize>,
    pub end_own: Option<usize>,
}

/// One end's footprint, for enclosure tests.
struct Foot<'a> {
    shape: Option<&'a Shape>,
    rect: Option<Rect>,
}

fn finite_rect(r: &Rect) -> bool {
    [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    let e = 1e-6;
    outer.x0 <= inner.x0 + e && outer.y0 <= inner.y0 + e && outer.x1 >= inner.x1 - e && outer.y1 >= inner.y1 - e
}

/// A rectangle in a shape's local frame as a world-space bounding box.
fn world_box(s: &Shape, r: Rect) -> Option<Rect> {
    let t = s.transform();
    Rect::bounding([Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)].map(|p| t.apply(p)))
}

impl<'a> Field<'a> {
    pub fn new(page: &'a Page) -> Self {
        let mut f = Field { obstacles: Vec::new(), owner: Vec::new(), band: Vec::new(), by_id: HashMap::new(), bounds: HashMap::new() };
        for s in &page.shapes {
            let b = s.bounds();
            f.by_id.entry(s.id.as_str()).or_insert(s);
            f.bounds.entry(s.id.as_str()).or_insert(b);
            // Junctions are points on wires, never something to route around.
            if crate::arrange::is_group(s) || crate::net::is_junction(s) || !crate::layer::visible(page, s) {
                continue;
            }
            f.obstacles.push(Obstacle::new(b.x0, b.y0, b.x1, b.y1));
            f.owner.push(s);
            f.band.push(false);
            if container::is_container(s)
                && let Some(h) = container::heading_rect(s).and_then(|r| world_box(s, r))
            {
                f.obstacles.push(Obstacle::new(h.x0, h.y0, h.x1, h.y1));
                f.owner.push(s);
                f.band.push(true);
            }
        }
        f
    }

    /// The id of the shape obstacle `k` belongs to.
    pub fn shape_id(&self, k: usize) -> Option<&'a str> {
        self.owner.get(k).map(|s| s.id.as_str())
    }

    fn foot(&self, e: &End) -> Foot<'a> {
        match e {
            End::Free { x, y } => Foot { shape: None, rect: Some(Rect::new(*x, *y, *x, *y)) },
            End::Glued { shape, .. } => match self.by_id.get(shape.as_str()) {
                Some(s) => Foot { shape: Some(*s), rect: self.bounds.get(shape.as_str()).copied() },
                None => Foot { shape: None, rect: None },
            },
        }
    }

    /// Whether `s` holds `end`: an ancestor through `parent`, or a frame it lies within.
    fn encloses(&self, s: &Shape, end: &Foot) -> bool {
        if end.shape.is_some_and(|e| e.id == s.id) {
            return false;
        }
        let mut cur = end.shape.and_then(|e| e.parent.as_deref());
        for _ in 0..MAX_DEPTH {
            let Some(id) = cur else { break };
            if id == s.id {
                return true;
            }
            cur = self.by_id.get(id).and_then(|p| p.parent.as_deref());
        }
        let outer = self.bounds.get(s.id.as_str()).copied().unwrap_or_else(|| s.bounds());
        end.rect.is_some_and(|r| finite_rect(&r) && contains(&outer, &r))
    }

    pub fn context(&self, c: &Connector) -> Ctx {
        let (a, b) = (self.foot(&c.start), self.foot(&c.end));
        let mut ctx = Ctx { ignore: Vec::new(), start_own: None, end_own: None };
        for (k, (s, band)) in self.owner.iter().zip(&self.band).enumerate() {
            let is_a = a.shape.is_some_and(|x| x.id == s.id);
            let is_b = b.shape.is_some_and(|x| x.id == s.id);
            if is_a || is_b {
                // An end shape's own heading band is part of it; an end shape that holds the other
                // end is passed through to reach it.
                let holds_other = (is_a && self.encloses(s, &b)) || (is_b && self.encloses(s, &a));
                if *band || holds_other {
                    ctx.ignore.push(k);
                } else {
                    if is_a {
                        ctx.start_own = Some(k);
                    }
                    if is_b {
                        ctx.end_own = Some(k);
                    }
                }
                continue;
            }
            let holds = self.encloses(s, &a) || self.encloses(s, &b);
            let in_held_set = container::role(s) == Some(Role::Lane)
                && s.parent.as_deref().and_then(|p| self.by_id.get(p)).is_some_and(|set| self.encloses(set, &a) || self.encloses(set, &b));
            if *band {
                // A heading band stays an obstacle, unless an end sits in it.
                let band_rect = self.obstacles.get(k).map(|o| Rect::new(o.rect.min.x, o.rect.min.y, o.rect.max.x, o.rect.max.y));
                let in_band = |f: &Foot| f.rect.zip(band_rect).is_some_and(|(r, br)| br.contains(r.center()));
                if in_band(&a) || in_band(&b) {
                    ctx.ignore.push(k);
                }
            } else if holds || in_held_set {
                ctx.ignore.push(k);
            }
        }
        ctx
    }

    /// The obstacles a rounded bend may overlap: the ignored ones, and an end shape whose line
    /// starts inside it (a rotated shape's bounding box), since its stub runs through it.
    pub fn rounding_ignore(&self, ctx: &Ctx, pts: &[Point]) -> Vec<usize> {
        let mut out = ctx.ignore.clone();
        for (own, p) in [(ctx.start_own, pts.first()), (ctx.end_own, pts.last())] {
            if let (Some(k), Some(p)) = (own, p)
                && let Some(o) = self.obstacles.get(k)
            {
                let r = o.rect;
                if p.x > r.min.x && p.x < r.max.x && p.y > r.min.y && p.y < r.max.y {
                    out.push(k);
                }
            }
        }
        out
    }

    fn hits(&self, ctx: &Ctx, pts: &[Point], clearance: f64) -> Vec<vizcraft_routing::Hit> {
        let r: Vec<RPoint> = pts.iter().map(|p| RPoint::new(p.x, p.y)).collect();
        vizcraft_routing::crossings(&r, &self.obstacles, &ctx.ignore, ctx.start_own, ctx.end_own, clearance)
    }

    fn clear(&self, ctx: &Ctx, pts: &[Point], clearance: f64) -> bool {
        self.hits(ctx, pts, clearance).is_empty()
    }
}

/// A [`Field`] built on first use, so drawing pays for it only when a curved connector needs it.
pub(crate) struct LazyField<'a> {
    page: &'a Page,
    cell: OnceCell<Field<'a>>,
}

impl<'a> LazyField<'a> {
    pub fn new(page: &'a Page) -> Self {
        Self { page, cell: OnceCell::new() }
    }

    pub fn get(&self) -> &Field<'a> {
        self.cell.get_or_init(|| Field::new(self.page))
    }
}

// ---------------------------------------------------------------------------------------------
// The oracle

/// Every place where a connector's drawn line enters a shape it must stay out of, in connector
/// order: the invariant check. Empty means no connector on the page crosses a shape. A connector
/// the router could not route, and a straight line the page policy keeps, show up here.
pub fn crossings(page: &Page) -> Vec<Crossing> {
    let lazy = LazyField::new(page);
    let field = lazy.get();
    let mut out = Vec::new();
    for c in &page.connectors {
        let pts = crate::route::polyline_in(page, c, &lazy);
        let ctx = field.context(c);
        let mut seen: HashSet<(usize, &str)> = HashSet::new();
        for h in field.hits(&ctx, &pts, -CROSSING_TOLERANCE) {
            let Some(shape) = field.shape_id(h.obstacle) else { continue };
            // A body and its heading band are one shape.
            if seen.insert((h.segment, shape)) {
                out.push(Crossing { connector: c.id.clone(), shape: shape.to_string(), segment: h.segment });
            }
        }
    }
    out
}

/// Whether the drawn route of `c` stays out of every shape it must avoid on the page (touching an
/// edge is allowed): [`crossings`] for one connector.
pub fn clear_of_shapes(page: &Page, c: &Connector) -> bool {
    let lazy = LazyField::new(page);
    let field = lazy.get();
    let pts = crate::route::polyline_in(page, c, &lazy);
    field.clear(&field.context(c), &pts, -CROSSING_TOLERANCE)
}

/// The ids of the routed connectors on `page` that have no clear route at all (the router fell
/// back), found by routing a copy of the page. Every connector in [`crossings`] should be here,
/// unless the page keeps straight lines.
pub fn unroutable(page: &Page) -> Vec<String> {
    let mut copy = page.clone();
    let ids: Vec<String> = page.connectors.iter().filter(|c| routes(page, c)).map(|c| c.id.clone()).collect();
    reroute(&mut copy, Some(&ids))
}

// ---------------------------------------------------------------------------------------------
// Rerouting

/// Reroute what changed between `before` and `doc`, on every page. The engine calls this after
/// each command that changes the document, inside the same undo step.
///
/// On each page, the routed connectors rerouted are the ones that are new or whose ends, style or
/// `avoid` flag changed, the ones glued to a shape whose bounds (or visibility, parent or heading)
/// changed, and the ones whose drawn line runs within reach of such a shape's old or new bounds.
/// A page whose routing policy changed, whose shape count changed or that `before` lacks has all
/// its routed connectors rerouted. A page with no routed connector, or no change, costs one
/// comparison.
pub fn refresh(doc: &mut Document, before: &Document) {
    for page in &mut doc.pages {
        let old = before.pages.iter().find(|p| p.id == page.id);
        let ids = stale(page, old);
        if ids.is_empty() {
            continue;
        }
        // Bends a person or a file chose may stay; a route the router stored before the change
        // is the router's to redo. Judged on the page before, where the ends were.
        let keep: HashSet<String> = ids
            .iter()
            .filter(|id| {
                let was = old.and_then(|o| Some((o, o.connector(id)?)));
                match (was, page.connector(id)) {
                    (Some((o, c)), Some(now)) if c.waypoints == now.waypoints => !crate::route::stored_route(o, c),
                    (_, Some(now)) => !crate::route::stored_route(page, now),
                    _ => false,
                }
            })
            .cloned()
            .collect();
        route_with(page, Some(&ids), &Options::default(), &keep);
    }
}

/// What about a shape matters to routing.
#[derive(PartialEq)]
struct Sig<'a> {
    bounds: Rect,
    solid: bool,
    parent: Option<&'a str>,
    heading: Option<Rect>,
    points: Vec<Point>,
}

fn sig<'a>(page: &Page, s: &'a Shape) -> Sig<'a> {
    Sig {
        bounds: s.bounds(),
        solid: !crate::arrange::is_group(s) && !crate::net::is_junction(s) && crate::layer::visible(page, s),
        parent: s.parent.as_deref(),
        heading: container::heading_rect(s).and_then(|r| world_box(s, r)),
        points: s.connection_points_world(),
    }
}

/// Whether some segment of `pts` comes within `reach` of `r`.
fn near_rect(pts: &[Point], r: &Rect, reach: f64) -> bool {
    let g = r.inflate(reach);
    if !finite_rect(&g) {
        return false;
    }
    let seg_hits = |a: Point, b: Point| {
        let bb = Rect::new(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y));
        bb.x0 <= g.x1 && bb.x1 >= g.x0 && bb.y0 <= g.y1 && bb.y1 >= g.y0
    };
    match pts {
        [p] => seg_hits(*p, *p),
        _ => pts.windows(2).any(|w| matches!(w, [a, b] if seg_hits(*a, *b))),
    }
}

fn stale(page: &Page, old: Option<&Page>) -> Vec<String> {
    let routed: Vec<&Connector> = page.connectors.iter().filter(|c| routes(page, c)).collect();
    if routed.is_empty() {
        return Vec::new();
    }
    let all = || routed.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
    let Some(old) = old else { return all() };
    if old == page {
        return Vec::new();
    }
    if old.routing != page.routing || old.shapes.len() != page.shapes.len() {
        return all();
    }
    let before: HashMap<&str, Sig> = old.shapes.iter().map(|s| (s.id.as_str(), sig(old, s))).collect();
    let mut changed: HashSet<&str> = HashSet::new();
    let mut rects: Vec<Rect> = Vec::new();
    for s in &page.shapes {
        let now = sig(page, s);
        match before.get(s.id.as_str()) {
            None => return all(),
            Some(was) if *was != now => {
                changed.insert(s.id.as_str());
                rects.push(was.bounds);
                rects.push(now.bounds);
                rects.extend(was.heading.iter().chain(now.heading.iter()).copied());
            }
            Some(_) => {}
        }
    }
    let old_conns: HashMap<&str, &Connector> = old.connectors.iter().map(|c| (c.id.as_str(), c)).collect();
    let lazy = LazyField::new(page);
    routed
        .into_iter()
        .filter(|c| {
            let edited =
                old_conns.get(c.id.as_str()).is_none_or(|o| o.start != c.start || o.end != c.end || o.route != c.route || o.avoid != c.avoid);
            let glued = [c.start.shape(), c.end.shape()].into_iter().flatten().any(|id| changed.contains(id));
            edited || glued || {
                let pts = crate::route::polyline_in(page, c, &lazy);
                rects.iter().any(|r| near_rect(&pts, r, 2.0 * MARGIN))
            }
        })
        .map(|c| c.id.clone())
        .collect()
}

/// Route the connectors named by `ids` (every connector on the page when `None`) now, storing
/// their routes in `waypoints` (see the module notes), and return the ids of those with no clear
/// route (the router's fallback). The connectors are routed together, so overlapping parallel runs
/// are spread apart and connectors leaving the same side fan out. Only routes that changed are
/// written. Unknown ids are ignored.
pub fn reroute(page: &mut Page, ids: Option<&[String]>) -> Vec<String> {
    route_with(page, ids, &Options::default(), &HashSet::new())
}

/// Reroute, for a drag preview, the routed connectors that `moving` can affect: connectors named
/// in `moving`, connectors glued to a shape in `moving` (or inside one), and connectors whose line
/// runs over one. Each search uses at most `budget` grid points, so a frame stays fast; a
/// connector that does not fit keeps a fallback until the drag commits and the real pass runs.
pub fn preview(page: &mut Page, moving: &[String], budget: usize) {
    let mut shapes: HashSet<String> = moving.iter().filter(|id| page.shape(id).is_some()).cloned().collect();
    // Members follow their group or container: they move too.
    for _ in 0..MAX_DEPTH {
        let more: Vec<String> = page
            .shapes
            .iter()
            .filter(|s| !shapes.contains(&s.id) && s.parent.as_ref().is_some_and(|p| shapes.contains(p)))
            .map(|s| s.id.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        shapes.extend(more);
    }
    let rects: Vec<Rect> = page.shapes.iter().filter(|s| shapes.contains(&s.id)).map(Shape::bounds).collect();
    let lazy = LazyField::new(page);
    let ids: Vec<String> = page
        .connectors
        .iter()
        .filter(|c| routes(page, c))
        .filter(|c| {
            moving.contains(&c.id)
                || [c.start.shape(), c.end.shape()].into_iter().flatten().any(|id| shapes.contains(id))
                || (!rects.is_empty() && {
                    let pts = crate::route::polyline_in(page, c, &lazy);
                    rects.iter().any(|r| near_rect(&pts, r, MARGIN))
                })
        })
        .map(|c| c.id.clone())
        .collect();
    // A connector glued to what moves cannot tell its own bends from a stale stored route here;
    // the commit decides. One that only runs over a moving shape still can.
    let keep: HashSet<String> = page
        .connectors
        .iter()
        .filter(|c| ids.contains(&c.id) && ![c.start.shape(), c.end.shape()].into_iter().flatten().any(|id| shapes.contains(id)))
        .map(|c| c.id.clone())
        .collect();
    drop(lazy);
    if !ids.is_empty() {
        let opts = Options { max_grid: budget.clamp(64, 1_000_000), ..Options::default() };
        route_with(page, Some(&ids), &opts, &keep);
    }
}

/// What routing decided for one connector before the search runs.
enum Plan {
    /// The plain route is kept; `points` is its polyline (nudging may still move it).
    Plain { points: Vec<RPoint>, nudge: bool },
    /// A straight line between two chosen points.
    Straight(Vec<RPoint>),
    /// Needs the obstacle-avoiding search.
    Search,
    /// Bends a person or a file chose, still clear: left exactly as they are.
    Keep,
}

fn rp(p: Point) -> RPoint {
    RPoint::new(p.x, p.y)
}

/// The side a direction leaves through, snapped to the nearest axis (y grows downward).
fn side_of(d: Vec2) -> Option<Side> {
    if !d.is_finite() || (d.x == 0.0 && d.y == 0.0) {
        return None;
    }
    Some(if d.x.abs() >= d.y.abs() {
        if d.x > 0.0 { Side::Right } else { Side::Left }
    } else if d.y > 0.0 {
        Side::Bottom
    } else {
        Side::Top
    })
}

/// How far a dynamically glued end on `s` may slide along a side: only on outlines whose sides are
/// straight at their connection points and that are not turned off the axes.
fn spread_of(s: &Shape) -> f64 {
    let square = (s.angle_deg / 90.0 - (s.angle_deg / 90.0).round()).abs() < 1e-9;
    let straight = matches!(s.kind, ShapeKind::Rectangle | ShapeKind::RoundedRectangle | ShapeKind::PredefinedProcess);
    if square && straight { FAN_MAX.min(s.w.min(s.h) / 4.0).max(0.0) } else { 0.0 }
}

/// The router's view of one end: the plain resolved point and side, the own obstacle, and for
/// dynamic glue every connection point as a candidate.
fn router_end(page: &Page, e: &End, plain: &EndPoint, own: Option<usize>) -> REnd {
    let mut out = REnd::at(rp(plain.pos));
    out.side = plain.dir.and_then(side_of);
    out.obstacle = own;
    // A port names one band of a compartment, so a port-glued end is fixed like point glue;
    // only an end with neither offers every connection point.
    if let End::Glued { shape, point: None, port: None } = e
        && let Some(s) = page.shape(shape)
    {
        out.candidates =
            (0..s.connection_points().len()).filter_map(|i| Some((rp(s.connection_point_world(i)?), side_of(s.connection_direction(i)?)?))).collect();
        out.spread = spread_of(s);
    }
    out
}

/// Every position an end may take for a straight line: each connection point of a dynamically
/// glued shape, else its one resolved point.
fn straight_points(page: &Page, e: &End, plain: &EndPoint) -> Vec<Point> {
    match e {
        End::Glued { shape, point: None, port: None } => match page.shape(shape) {
            Some(s) if !s.connection_points().is_empty() => s.connection_points_world(),
            _ => vec![plain.pos],
        },
        _ => vec![plain.pos],
    }
}

/// The first clear straight line between the ends' positions, nearest pair first.
fn straight_pair(field: &Field, ctx: &Ctx, starts: &[Point], ends: &[Point]) -> Option<(Point, Point)> {
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (i, a) in starts.iter().enumerate() {
        for (j, b) in ends.iter().enumerate() {
            if a.is_finite() && b.is_finite() && pairs.len() < MAX_PAIRS {
                pairs.push((a.distance(*b), i, j));
            }
        }
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    pairs.into_iter().find_map(|(_, i, j)| {
        let (a, b) = (*starts.get(i)?, *ends.get(j)?);
        field.clear(ctx, &[a, b], KEEP_CLEARANCE).then_some((a, b))
    })
}

/// Whether the first segment of `pts` is shorter than `trim.0` or the last shorter than `trim.1`.
fn short_stub(pts: &[RPoint], trim: (f64, f64)) -> bool {
    let len = |a: Option<&RPoint>, b: Option<&RPoint>| match (a, b) {
        (Some(a), Some(b)) => ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt(),
        _ => f64::INFINITY,
    };
    let n = pts.len();
    pts.len() >= 3
        && (len(pts.first(), pts.get(1)) + 1e-6 < trim.0
            || len(n.checked_sub(1).and_then(|i| pts.get(i)), n.checked_sub(2).and_then(|i| pts.get(i))) + 1e-6 < trim.1)
}

/// How far an arrowhead of `arrow` at `line_width` cuts the line back from its tip.
pub fn head_trim(arrow: crate::arrows::Arrow, line_width: f64) -> f64 {
    crate::arrows::outline(arrow, Point::ZERO, Vec2::new(1.0, 0.0), line_width).map_or(0.0, |g| g.trim)
}

/// Route the named connectors. A connector in `keep` whose bends were put there by a person or a
/// file (not by the router) keeps them while the line through them stays clear.
fn route_with(page: &mut Page, ids: Option<&[String]>, opts: &Options, keep: &HashSet<String>) -> Vec<String> {
    let wanted: Option<HashSet<&str>> = ids.map(|ids| ids.iter().map(String::as_str).collect());
    let chosen: Vec<usize> =
        page.connectors.iter().enumerate().filter(|(_, c)| wanted.as_ref().is_none_or(|w| w.contains(c.id.as_str()))).map(|(i, _)| i).collect();
    if chosen.is_empty() {
        return Vec::new();
    }
    let keep_straight = page.routing.straight_fallback == StraightFallback::Keep;
    let (results, failed) = {
        let view: &Page = page;
        let lazy = LazyField::new(view);
        let field = lazy.get();
        let mut plans: Vec<Plan> = Vec::with_capacity(chosen.len());
        let mut requests: Vec<Request> = Vec::with_capacity(chosen.len());
        let mut trims: Vec<(f64, f64)> = Vec::with_capacity(chosen.len());
        for c in chosen.iter().filter_map(|&i| view.connectors.get(i)) {
            let ctx = field.context(c);
            let ends = resolve_through(view, c, &[]);
            let plain: Vec<Point> = crate::route::path_for(&ends, c.route).flatten(0.5).into_iter().next().map(|pl| pl.points).unwrap_or_default();
            let own_bends = keep.contains(&c.id) && !c.waypoints.is_empty() && !crate::route::stored_route(view, c);
            let plan = if own_bends && field.clear(&ctx, &crate::route::polyline_in(view, c, &lazy), -CROSSING_TOLERANCE) {
                Plan::Keep
            } else if field.clear(&ctx, &plain, KEEP_CLEARANCE) {
                Plan::Plain { points: plain.iter().copied().map(rp).collect(), nudge: c.route == Route::Orthogonal }
            } else if c.route == Route::Straight {
                let starts = straight_points(view, &c.start, &ends.start);
                let finishes = straight_points(view, &c.end, &ends.end);
                match straight_pair(field, &ctx, &starts, &finishes) {
                    Some((a, b)) => Plan::Straight(vec![rp(a), rp(b)]),
                    None if keep_straight => Plan::Plain { points: plain.iter().copied().map(rp).collect(), nudge: false },
                    None => Plan::Search,
                }
            } else {
                Plan::Search
            };
            plans.push(plan);
            trims.push((head_trim(c.start_arrow, c.style.line_width), head_trim(c.end_arrow, c.style.line_width)));
            // Each end's stub is at least its arrowhead's trim, so the head never sits over a bend.
            let (trim_start, trim_end) = trims.last().copied().unwrap_or_default();
            let mut start = router_end(view, &c.start, &ends.start, ctx.start_own);
            let mut end = router_end(view, &c.end, &ends.end, ctx.end_own);
            start.stub = (trim_start > 0.0).then_some(trim_start);
            end.stub = (trim_end > 0.0).then_some(trim_end);
            requests.push(Request::new(start, end).ignoring(ctx.ignore));
        }
        // Search the connectors that need it, together; nudging comes after, over every route.
        let search: Vec<Request> = plans.iter().zip(&requests).filter(|(p, _)| matches!(p, Plan::Search)).map(|(_, r)| r.clone()).collect();
        let found = vizcraft_routing::route_all(&field.obstacles, &search, &Options { segment_spacing: 0.0, ..*opts });
        let mut found = found.into_iter();
        let mut routes: Vec<RRoute> = Vec::with_capacity(plans.len());
        let mut failed: Vec<bool> = Vec::with_capacity(plans.len());
        for plan in &plans {
            let (r, f) = match plan {
                // Routes nudging must not move are passed as fallbacks, which it skips.
                Plan::Plain { points, nudge } => (RRoute { points: points.clone(), fallback: !nudge, ..RRoute::default() }, false),
                Plan::Straight(points) => (RRoute { points: points.clone(), fallback: true, ..RRoute::default() }, false),
                Plan::Keep => (RRoute { fallback: true, ..RRoute::default() }, false),
                Plan::Search => {
                    let r = found.next().unwrap_or_default();
                    let f = r.fallback;
                    (r, f)
                }
            };
            routes.push(r);
            failed.push(f);
        }
        let mut nudged = routes.clone();
        vizcraft_routing::nudge_routes(&field.obstacles, &requests, &mut nudged, opts);
        let results: Vec<Option<Vec<Point>>> = plans
            .iter()
            .zip(&routes)
            .zip(&nudged)
            .zip(&trims)
            .map(|(((plan, before), after), &trim)| {
                // Nudging centres a segment in its channel, which can leave the last one shorter than
                // its arrowhead; the stubs of the search result are one margin long, so it stands then.
                let after = if short_stub(&after.points, trim) && !short_stub(&before.points, trim) { before } else { after };
                let pts = || after.points.iter().map(|p| Point::new(p.x, p.y)).collect::<Vec<_>>();
                match plan {
                    // A plain route nudging left alone needs no bends: it is drawn from the glue.
                    Plan::Plain { .. } if before.points == after.points => Some(Vec::new()),
                    Plan::Keep => None,
                    _ => Some(pts()),
                }
            })
            .collect();
        (results, failed)
    };
    let mut unroutable = Vec::new();
    for ((&i, bends), f) in chosen.iter().zip(results).zip(failed) {
        if let Some(c) = page.connectors.get_mut(i) {
            if f {
                unroutable.push(c.id.clone());
            }
            if let Some(mut bends) = bends {
                bends.truncate(crate::model::MAX_WAYPOINTS);
                if c.waypoints != bends {
                    c.waypoints = bends;
                }
            }
        }
    }
    unroutable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Connector;
    use crate::route::{resolve_ends, route_path, route_polyline};
    use crate::routing_policy::Avoid;

    fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
        Shape::new(id, ShapeKind::Rectangle, x, y, w, h)
    }

    fn link(id: &str, a: &str, b: &str, route: Route) -> Connector {
        let mut c = Connector::new(id, End::dynamic(a), End::dynamic(b));
        c.route = route;
        c
    }

    /// Two shapes with a wall between them, joined by a connector of style `route`.
    fn walled(route: Route) -> Page {
        let mut page = Page::new("p", "p");
        page.shapes = vec![rect("a", 0.0, 100.0, 60.0, 40.0), rect("wall", 150.0, 0.0, 40.0, 260.0), rect("b", 300.0, 100.0, 60.0, 40.0)];
        page.connectors = vec![link("c", "a", "b", route)];
        page
    }

    fn doc_of(page: Page) -> Document {
        Document { pages: vec![page], ..Document::default() }
    }

    fn orthogonal(pts: &[Point]) -> bool {
        pts.windows(2).all(|w| (w[0].x - w[1].x).abs() < 1e-9 || (w[0].y - w[1].y).abs() < 1e-9)
    }

    #[test]
    fn rerouted_connectors_stay_clear_and_draw_what_was_stored() {
        let mut page = walled(Route::Orthogonal);
        assert_eq!(crossings(&page).len(), 1, "the automatic route crosses the wall");
        assert!(!clear_of_shapes(&page, &page.connectors[0]));
        assert!(reroute(&mut page, None).is_empty());
        let c = &page.connectors[0];
        let drawn = route_polyline(&page, c);
        assert!(crossings(&page).is_empty(), "{drawn:?}");
        assert!(orthogonal(&drawn), "{drawn:?}");
        // The stored route is the drawn one, end points included.
        assert_eq!(drawn, c.waypoints);
        let ends = resolve_ends(&page, c);
        assert_eq!((Some(&ends.start.pos), Some(&ends.end.pos)), (c.waypoints.first(), c.waypoints.last()));
    }

    #[test]
    fn a_clear_plain_route_is_kept_with_no_bends() {
        let mut page = walled(Route::Orthogonal);
        page.shapes.retain(|s| s.id != "wall");
        let before = route_polyline(&page, &page.connectors[0]);
        reroute(&mut page, None);
        assert!(page.connectors[0].waypoints.is_empty());
        assert_eq!(route_polyline(&page, &page.connectors[0]), before);
    }

    #[test]
    fn dynamic_glue_leaves_through_the_side_the_search_picks() {
        // A wall hugs a's right side, which faces b: the line leaves a from another side.
        let mut page = walled(Route::Orthogonal);
        page.shapes[1] = rect("wall", 68.0, 60.0, 20.0, 120.0);
        reroute(&mut page, None);
        assert!(crossings(&page).is_empty());
        let c = &page.connectors[0];
        let start = resolve_ends(&page, c).start;
        assert_ne!(start.point, Some(1), "not the right side, into the wall");
        assert!(matches!(c.start, End::Glued { point: None, .. }), "the end stays dynamic");
    }

    #[test]
    fn straight_connectors_pick_a_clear_pair_detour_or_keep() {
        // The wall blocks the facing points but not the line between the tops.
        let mut page = walled(Route::Straight);
        page.shapes[1] = rect("wall", 150.0, 110.0, 40.0, 150.0);
        reroute(&mut page, None);
        let pts = route_polyline(&page, &page.connectors[0]);
        assert_eq!(pts.len(), 2, "one straight segment: {pts:?}");
        assert!(crossings(&page).is_empty());
        // A full-height wall leaves no clear straight line: the detour, unless the page keeps it.
        let mut page = walled(Route::Straight);
        reroute(&mut page, None);
        assert!(crossings(&page).is_empty());
        assert!(route_polyline(&page, &page.connectors[0]).len() > 2);
        page.routing.straight_fallback = StraightFallback::Keep;
        reroute(&mut page, None);
        assert_eq!(route_polyline(&page, &page.connectors[0]).len(), 2);
        assert_eq!(crossings(&page).len(), 1, "a kept straight line is reported");
        assert!(unroutable(&page).is_empty(), "it is a choice, not a failure");
    }

    #[test]
    fn curved_connectors_round_the_route_without_cutting_into_shapes() {
        let mut page = walled(Route::Curved);
        assert!(!crossings(&page).is_empty(), "the plain curve crosses the wall");
        reroute(&mut page, None);
        assert!(crossings(&page).is_empty());
        let path = route_path(&page, &page.connectors[0]);
        assert!(path.els.iter().any(|e| matches!(e, crate::geom::PathEl::CubicTo(..))), "the bends are rounded");
    }

    #[test]
    fn containers_lanes_groups_and_hidden_shapes_are_not_in_the_way() {
        let mut page = Page::new("p", "p");
        let mut boxed = container::new_container("box", 0.0, 0.0, 500.0, 300.0, "Box");
        container::refresh(&mut boxed);
        page.shapes.push(boxed);
        for (id, x) in [("m1", 40.0), ("m2", 380.0)] {
            let mut s = rect(id, x, 120.0, 80.0, 50.0);
            s.parent = Some("box".into());
            page.shapes.push(s);
        }
        page.shapes.push(rect("out", 700.0, -200.0, 80.0, 50.0));
        // A hidden shape and a group straight across the members' line.
        let mut hidden = rect("hidden", 200.0, 100.0, 40.0, 100.0);
        hidden.behavior.hidden = true;
        page.shapes.push(hidden);
        let mut g = crate::arrange::new_group("g");
        (g.x, g.y, g.w, g.h) = (100.0, 50.0, 300.0, 200.0);
        page.shapes.push(g);
        page.connectors = vec![link("inside", "m1", "m2", Route::Orthogonal), link("out", "m1", "out", Route::Orthogonal)];
        reroute(&mut page, None);
        assert!(crossings(&page).is_empty(), "{:?}", crossings(&page));
        assert!(page.connectors[0].waypoints.is_empty(), "members of one container link straight across it");
        // Leaving the container, the line avoids its heading band.
        let band = container::heading_rect(page.shape("box").unwrap()).unwrap();
        let out = route_polyline(&page, &page.connectors[1]);
        let hits = vizcraft_routing::crossings(
            &out.iter().map(|p| RPoint::new(p.x, p.y)).collect::<Vec<_>>(),
            &[Obstacle::new(band.x0, band.y0, band.x1, band.y1)],
            &[],
            None,
            None,
            -CROSSING_TOLERANCE,
        );
        assert!(hits.is_empty(), "{out:?}");
        // A container that holds neither end is in the way like any shape.
        page.connectors.push(link("across", "out", "m2", Route::Orthogonal));
        page.shapes.push(rect("far", -300.0, 120.0, 80.0, 50.0));
        page.connectors.push(link("around", "far", "out", Route::Orthogonal));
        reroute(&mut page, None);
        assert!(crossings(&page).is_empty(), "{:?}", crossings(&page));
    }

    #[test]
    fn lanes_of_the_set_holding_an_end_are_crossed() {
        let mut page = Page::new("p", "p");
        let mut set = container::new_lane_set("set", 0.0, 0.0, false, "Set");
        (set.w, set.h) = (600.0, 450.0);
        page.shapes.push(set);
        for (i, id) in ["l0", "l1", "l2"].into_iter().enumerate() {
            let mut lane = container::new_lane(id, id, false);
            (lane.x, lane.y, lane.w, lane.h) = (0.0, 150.0 * i as f64, 600.0, 150.0);
            lane.parent = Some("set".into());
            page.shapes.push(lane);
            let mut m = rect(&format!("m{i}"), 200.0, 150.0 * i as f64 + 50.0, 80.0, 50.0);
            m.parent = Some(id.into());
            page.shapes.push(m);
        }
        page.connectors = vec![link("down", "m0", "m2", Route::Orthogonal)];
        reroute(&mut page, None);
        assert!(crossings(&page).is_empty(), "{:?}", crossings(&page));
        // Straight down through the middle lane, not round the outside of the set.
        let pts = route_polyline(&page, &page.connectors[0]);
        assert!(pts.iter().all(|p| p.x > 0.0 && p.x < 600.0), "{pts:?}");
    }

    #[test]
    fn an_enclosed_end_is_reported_and_keeps_its_fallback() {
        let mut page = Page::new("p", "p");
        page.shapes = vec![
            rect("a", 0.0, 0.0, 40.0, 40.0),
            rect("in", 400.0, 400.0, 40.0, 40.0),
            rect("t", 350.0, 350.0, 140.0, 20.0),
            rect("bm", 350.0, 470.0, 140.0, 20.0),
            rect("l", 350.0, 350.0, 20.0, 140.0),
            rect("r", 470.0, 350.0, 20.0, 140.0),
        ];
        page.connectors = vec![link("c", "a", "in", Route::Orthogonal)];
        assert_eq!(reroute(&mut page, None), vec!["c".to_string()]);
        assert!(!page.connectors[0].waypoints.is_empty(), "the fallback bends are kept");
        assert!(crossings(&page).iter().all(|x| x.connector == "c") && !crossings(&page).is_empty());
        assert_eq!(unroutable(&page), vec!["c".to_string()]);
    }

    #[test]
    fn connectors_sharing_a_side_fan_out_and_draw_where_they_were_slid() {
        // Both connectors leave a's right side and run on top of each other to the turn.
        let mut page = Page::new("p", "p");
        page.shapes = vec![rect("a", 0.0, 0.0, 100.0, 100.0), rect("b", 400.0, -300.0, 100.0, 60.0), rect("c", 400.0, 340.0, 100.0, 60.0)];
        page.connectors = vec![link("ab", "a", "b", Route::Orthogonal), link("ac", "a", "c", Route::Orthogonal)];
        let shared = |page: &Page| route_polyline(page, &page.connectors[0]).get(..2) == route_polyline(page, &page.connectors[1]).get(..2);
        assert!(shared(&page), "before routing the first segments coincide");
        reroute(&mut page, None);
        assert!(crossings(&page).is_empty(), "{:?}", crossings(&page));
        assert!(!shared(&page));
        let (ab, ac) = (resolve_ends(&page, &page.connectors[0]), resolve_ends(&page, &page.connectors[1]));
        assert_eq!((ab.start.point, ac.start.point), (Some(1), Some(1)), "both still on a's right side");
        assert_eq!((ab.start.pos.x, ac.start.pos.x), (100.0, 100.0));
        assert!((ab.start.pos.y - ac.start.pos.y).abs() >= 6.0 - 1e-9 && ab.start.pos.y < ac.start.pos.y, "{ab:?} {ac:?}");
        for c in &page.connectors {
            assert!(matches!(c.start, End::Glued { point: None, .. }), "the end stays dynamic");
            assert_eq!(route_polyline(&page, c), c.waypoints, "{} draws its stored route", c.id);
        }
        // Rerouting again lands on the same spread.
        let once = page.clone();
        reroute(&mut page, None);
        assert_eq!(page, once);
    }

    #[test]
    fn refresh_reroutes_only_what_a_change_touches() {
        let mut page = walled(Route::Orthogonal);
        page.shapes.push(rect("far", 1000.0, 1000.0, 50.0, 50.0));
        page.shapes.push(rect("x", 600.0, 100.0, 40.0, 40.0));
        page.shapes.push(rect("y", 800.0, 100.0, 40.0, 40.0));
        page.connectors.push(link("xy", "x", "y", Route::Orthogonal));
        let mut doc = doc_of(page);
        refresh(&mut doc, &Document::default());
        assert!(crossings(&doc.pages[0]).is_empty());
        let routed = doc.clone();
        refresh(&mut doc, &routed);
        assert_eq!(doc, routed, "nothing changed, nothing rerouted");
        // Plant a marker in xy's bends: a far move leaves it, a move of x reroutes it.
        let marker = vec![Point::new(620.0, 100.0), Point::new(620.0, 50.0), Point::new(820.0, 50.0), Point::new(820.0, 100.0)];
        doc.pages[0].connectors[1].waypoints = marker.clone();
        let before = doc.clone();
        if let Some(s) = doc.pages[0].shape_mut("far") {
            s.x += 30.0;
        }
        refresh(&mut doc, &before);
        assert_eq!(doc.pages[0].connectors[1].waypoints, marker, "a far shape moving touches nothing");
        let before = doc.clone();
        if let Some(s) = doc.pages[0].shape_mut("x") {
            s.y += 30.0;
        }
        refresh(&mut doc, &before);
        assert_ne!(doc.pages[0].connectors[1].waypoints, marker, "a glued shape moving reroutes");
        // A shape dropped onto a line reroutes that line too.
        let before = doc.clone();
        if let Some(s) = doc.pages[0].shape_mut("far") {
            (s.x, s.y) = (700.0, 100.0);
        }
        refresh(&mut doc, &before);
        assert!(crossings(&doc.pages[0]).is_empty(), "{:?}", crossings(&doc.pages[0]));
        // A page that does not route leaves its connectors alone.
        let mut plain = doc_of(walled(Route::Orthogonal));
        plain.pages[0].routing.avoid = Avoid::Never;
        let copy = plain.clone();
        refresh(&mut plain, &Document::default());
        assert_eq!(plain, copy);
    }

    #[test]
    fn bends_from_a_person_or_a_file_are_kept_while_clear() {
        let mut page = walled(Route::Orthogonal);
        let mine = vec![Point::new(80.0, 120.0), Point::new(80.0, 300.0), Point::new(250.0, 300.0), Point::new(250.0, 120.0)];
        page.connectors[0].waypoints = mine.clone();
        assert!(crossings(&page).is_empty());
        let mut doc = doc_of(page);
        refresh(&mut doc, &Document::default());
        assert_eq!(doc.pages[0].connectors[0].waypoints, mine, "clear bends of one's own stay");
        // Once the wall grows over them, they give way to a route.
        let before = doc.clone();
        if let Some(s) = doc.pages[0].shape_mut("wall") {
            s.h = 400.0;
        }
        refresh(&mut doc, &before);
        assert_ne!(doc.pages[0].connectors[0].waypoints, mine);
        assert!(crossings(&doc.pages[0]).is_empty());
        // An explicit reroute replaces them even when clear.
        let mut page = walled(Route::Orthogonal);
        page.connectors[0].waypoints = mine.clone();
        reroute(&mut page, None);
        assert_ne!(page.connectors[0].waypoints, mine);
    }

    #[test]
    fn preview_reroutes_around_a_dragged_shape() {
        let mut page = walled(Route::Orthogonal);
        page.shapes[1] = rect("wall", 150.0, 300.0, 40.0, 100.0);
        assert!(crossings(&page).is_empty());
        crate::arrange::translate(&mut page, &["wall".to_string()], 0.0, -230.0);
        assert!(!crossings(&page).is_empty(), "the wall now sits on the line");
        preview(&mut page, &["wall".to_string()], PREVIEW_BUDGET);
        assert!(crossings(&page).is_empty(), "{:?}", crossings(&page));
        // Nothing that cannot be affected is touched.
        let still = page.clone();
        preview(&mut page, &["nothing".to_string()], PREVIEW_BUDGET);
        assert_eq!(page, still);
    }

    #[test]
    fn hostile_pages_do_not_panic() {
        let mut page = walled(Route::Curved);
        page.shapes.push(rect("huge", -1e7, -1e7, 1e6, 1e6));
        page.shapes.push(Shape { id: "nan".into(), x: f64::NAN, y: f64::INFINITY, ..Shape::default() });
        page.shapes.push(Shape { id: "loop".into(), parent: Some("loop".into()), ..Shape::default() });
        page.connectors.push(Connector::new(
            "ghost",
            End::Glued { shape: "gone".into(), point: Some(99), port: None },
            End::free(Point::new(f64::NAN, 0.0)),
        ));
        page.connectors.push(Connector::new("self", End::dynamic("a"), End::dynamic("a")));
        page.connectors.push(Connector::new("junk", End::dynamic("nan"), End::dynamic("loop")));
        page.connectors[0].waypoints = vec![Point::new(f64::NAN, 1.0), Point::new(1e300, -1e300)];
        let _ = crossings(&page);
        reroute(&mut page, None);
        for c in &page.connectors {
            let _ = crate::route::connector_geometry(&page, c);
        }
        reroute(&mut page, Some(&["nope".to_string()]));
        let mut doc = doc_of(page);
        let empty = Document { pages: Vec::new(), ..Document::default() };
        refresh(&mut doc, &empty);
        preview(&mut doc.pages[0], &["a".to_string(), "ghost".to_string()], 0);
        let _ = unroutable(&doc.pages[0]);
    }

    #[test]
    fn page_of_two_hundred_shapes_reroutes_quickly() {
        let mut page = Page::default();
        for i in 0..200 {
            let (col, row) = ((i % 20) as f64, (i / 20) as f64);
            page.shapes.push(rect(&format!("s{i}"), col * 120.0, row * 100.0, 60.0, 40.0));
        }
        for i in 0..200usize {
            let j = (i * 37 + 11) % 200;
            page.connectors.push(link(&format!("c{i}"), &format!("s{i}"), &format!("s{j}"), Route::Orthogonal));
        }
        let mut doc = doc_of(page);
        let t = std::time::Instant::now();
        refresh(&mut doc, &Document::default());
        let first = t.elapsed();
        let routed = doc.clone();
        let t = std::time::Instant::now();
        refresh(&mut doc, &routed);
        let second = t.elapsed();
        let t = std::time::Instant::now();
        let found = crossings(&doc.pages[0]);
        let oracle = t.elapsed();
        eprintln!("refresh 200 shapes, 200 connectors: first {first:?}, again {second:?}, crossings {oracle:?}");
        let failed = unroutable(&doc.pages[0]);
        assert!(found.iter().all(|x| failed.contains(&x.connector)), "{found:?}");
        assert!(failed.len() < 10, "most connectors found a clear route: {failed:?}");
    }
}
