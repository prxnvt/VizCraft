//! Glue and routing: where a connector's ends are and the path between them.
//!
//! Everything here is a pure function of the page. A connector stores how it is glued and,
//! optionally, its bends (`waypoints`); its end points and route are derived from the shapes on
//! every call, so moving, resizing or rotating a shape moves its connectors with no extra
//! bookkeeping. Stored bends come from obstacle-avoiding routing ([`crate::autoroute`]) or a file.

use crate::autoroute::LazyField;
use crate::geom::{Path, PathEl, Point, Vec2, point_along};
use crate::model::{Connector, End, Page, Route};

/// How far an orthogonal route runs straight out of a glued side before its first turn.
pub const STUB: f64 = 20.0;

/// A resolved connector end.
#[derive(Clone, Debug, PartialEq)]
pub struct EndPoint {
    /// Position in world space.
    pub pos: Point,
    /// Unit direction the route leaves (or arrives) along, pointing away from the glued shape.
    /// `None` for a free end, or glue to a point at a shape's centre.
    pub dir: Option<Vec2>,
    /// The glued shape's id, when glued to a shape that exists.
    pub shape: Option<String>,
    /// The connection point index in use (the chosen one for dynamic glue).
    pub point: Option<usize>,
}

/// Both resolved ends of a connector.
#[derive(Clone, Debug, PartialEq)]
pub struct Ends {
    /// The start.
    pub start: EndPoint,
    /// The end.
    pub end: EndPoint,
}

/// A rough position for an end, used to pick the other end's dynamic connection point: a free
/// end's point, a point-glued end's connection point, or a dynamically glued shape's centre.
fn anchor(page: &Page, e: &End) -> Point {
    match e {
        End::Free { x, y } => Point::new(*x, *y),
        End::Glued { shape, .. } => match page.shape(shape) {
            Some(s) => e.resolve_port(s).and_then(|i| s.connection_point_world(i)).unwrap_or_else(|| s.center()),
            None => Point::ZERO,
        },
    }
}

/// Resolve one end given a point near the other end. With `slide`, a dynamic end whose `toward`
/// (a routed connector's stored end point) lies on its side a little way from the connection
/// point is drawn there: the nudge pass slid it along the side to fan connectors out.
fn resolve(page: &Page, e: &End, toward: Point, slide: bool) -> EndPoint {
    match e {
        End::Free { x, y } => EndPoint { pos: Point::new(*x, *y).sanitized(), dir: None, shape: None, point: None },
        End::Glued { shape, .. } => {
            let Some(s) = page.shape(shape) else {
                // Glue to a shape that is gone: the commands detach ends before deleting, so this
                // only happens with a damaged file. Park the end at the origin rather than fail.
                return EndPoint { pos: Point::ZERO, dir: None, shape: None, point: None };
            };
            let fixed = e.resolve_port(s).filter(|i| s.connection_point_world(*i).is_some());
            // A routed end slid along a side belongs to the connection point of that side, even
            // where another side's point is nearer (a long bar's end, say).
            let on_side = || {
                (0..s.connection_points().len()).find(|&i| {
                    let (Some(p), Some(d)) = (s.connection_point_world(i), axis(s.connection_direction(i))) else { return false };
                    let off = toward - p;
                    off.is_finite() && off.length() > 1e-6 && off.length() <= crate::autoroute::slide_limit(s, d) + 1e-6 && off.dot(d).abs() <= 1e-6
                })
            };
            let exact = nearest_point(&s.connection_points_world(), toward)
                .filter(|&i| s.connection_point_world(i).is_some_and(|p| p.distance(toward) < 1e-6));
            let chosen =
                fixed.or(exact).or_else(|| if slide { on_side() } else { None }).or_else(|| nearest_point(&s.connection_points_world(), toward));
            match chosen.and_then(|i| Some((i, s.connection_point_world(i)?))) {
                Some((i, pos)) => {
                    let dir = s.connection_direction(i);
                    let slid = (slide && fixed.is_none())
                        .then(|| axis(dir))
                        .flatten()
                        .filter(|d| {
                            let off = toward - pos;
                            off.is_finite() && off.length() <= crate::autoroute::slide_limit(s, *d) + 1e-6 && off.dot(*d).abs() <= 1e-6
                        })
                        .map(|_| toward);
                    EndPoint { pos: slid.unwrap_or(pos), dir, shape: Some(s.id.clone()), point: Some(i) }
                }
                None => EndPoint { pos: s.center(), dir: None, shape: Some(s.id.clone()), point: None },
            }
        }
    }
}

/// Index of the point in `pts` nearest to `p`.
pub fn nearest_point(pts: &[Point], p: Point) -> Option<usize> {
    pts.iter().enumerate().filter(|(_, q)| q.is_finite()).min_by(|(_, a), (_, b)| a.distance(p).total_cmp(&b.distance(p))).map(|(i, _)| i)
}

/// Resolve both ends of `c` on `page`. Point glue uses that connection point; dynamic glue picks
/// the shape's connection point nearest the other end, or, when the connector has bends, nearest
/// its first (or last) bend, so the route does not double back across the shape.
///
/// A connector the page routes around shapes stores its whole route, end points included (see
/// [`crate::autoroute`]), so its dynamic ends land exactly on the stored end points: the
/// connection points the router chose, or a point slid a little along that side.
pub fn resolve_ends(page: &Page, c: &Connector) -> Ends {
    resolve_with(page, c, &c.waypoints, stored_route(page, c))
}

/// Whether `w` is where the router leaves end `e` of a routed connector: the free point, the glued
/// connection point, or for dynamic glue a connection point or a point slid along its side.
fn router_end(page: &Page, e: &End, w: Point) -> bool {
    match e {
        End::Free { x, y } => Point::new(*x, *y).sanitized().distance(w) < 1e-6,
        End::Glued { shape, point, .. } => {
            let Some(s) = page.shape(shape) else { return false };
            // A port resolves to one connection point, so it is as fixed as point glue.
            let fixed = e.resolve_port(s).or(*point);
            let point = &fixed;
            let indices: Vec<usize> = match point {
                Some(i) => vec![*i],
                None => (0..s.connection_points().len()).collect(),
            };
            indices.into_iter().any(|i| {
                let Some(p) = s.connection_point_world(i) else { return false };
                let off = w - p;
                off.length() < 1e-6
                    || (point.is_none()
                        && axis(s.connection_direction(i))
                            .is_some_and(|d| off.length() <= crate::autoroute::slide_limit(s, d) + 1e-6 && off.dot(d).abs() <= 1e-6))
            })
        }
    }
}

/// Whether the stored `waypoints` of `c` are a route the page's router made (it stores the whole
/// polyline, its first and last points on the ends), as opposed to bends a person or a file put
/// there, which are drawn the classic way and kept while they stay clear.
pub fn stored_route(page: &Page, c: &Connector) -> bool {
    if !crate::routing_policy::routes(page, c) {
        return false;
    }
    let first = c.waypoints.iter().copied().find(|p| p.is_finite());
    let last = c.waypoints.iter().rev().copied().find(|p| p.is_finite());
    match (first, last) {
        (Some(a), Some(b)) => router_end(page, &c.start, a) && router_end(page, &c.end, b),
        _ => false,
    }
}

/// [`resolve_ends`] as if the connector's bends were `waypoints` (empty: as if it had none).
pub fn resolve_through(page: &Page, c: &Connector, waypoints: &[Point]) -> Ends {
    resolve_with(page, c, waypoints, false)
}

fn resolve_with(page: &Page, c: &Connector, waypoints: &[Point], slide: bool) -> Ends {
    let first = waypoints.iter().copied().find(|p| p.is_finite());
    let last = waypoints.iter().rev().copied().find(|p| p.is_finite());
    if let (Some(first), Some(last)) = (first, last) {
        return Ends { start: resolve(page, &c.start, first, slide), end: resolve(page, &c.end, last, slide) };
    }
    let start = resolve(page, &c.start, anchor(page, &c.end), false);
    let end = resolve(page, &c.end, start.pos, false);
    // A dynamic start was chosen against the other end's rough anchor; now that the end is
    // known exactly, choose again so both dynamic ends face each other.
    let start = if matches!(c.start, End::Glued { point: None, .. }) { resolve(page, &c.start, end.pos, false) } else { start };
    Ends { start, end }
}

/// Snap a direction to the nearest axis; `None` stays `None`.
fn axis(dir: Option<Vec2>) -> Option<Vec2> {
    let d = dir?;
    if !d.is_finite() {
        return None;
    }
    Some(if d.x.abs() >= d.y.abs() { Point::new(d.x.signum(), 0.0) } else { Point::new(0.0, d.y.signum()) })
}

/// The dominant axis direction from `a` toward `b` (right when they coincide).
fn axis_toward(a: Point, b: Point) -> Vec2 {
    let d = b - a;
    if d.x.abs() >= d.y.abs() { Point::new(if d.x < 0.0 { -1.0 } else { 1.0 }, 0.0) } else { Point::new(0.0, d.y.signum()) }
}

/// Drop repeated points and middle points of straight runs.
fn simplify(pts: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last().is_some_and(|l| l.distance(p) < 1e-9) {
            continue;
        }
        let n = out.len();
        if let (Some(a), Some(b)) = (n.checked_sub(2).and_then(|i| out.get(i)).copied(), out.last().copied())
            && (b - a).cross(p - b).abs() < 1e-9
            && (b - a).dot(p - b) >= 0.0
        {
            out.pop();
        }
        out.push(p);
    }
    out
}

/// An orthogonal route: out of each end along its side direction for [`STUB`], then one or two
/// bends to meet. Every segment is horizontal or vertical.
pub fn orthogonal_points(start: &EndPoint, end: &EndPoint) -> Vec<Point> {
    let (p0, p1) = (start.pos, end.pos);
    let d0 = axis(start.dir).unwrap_or_else(|| axis_toward(p0, p1));
    let d1 = axis(end.dir).unwrap_or_else(|| axis_toward(p1, p0));
    // Free ends need no stub: they are not leaving a shape side.
    let s0 = if start.dir.is_some() { STUB } else { 0.0 };
    let s1 = if end.dir.is_some() { STUB } else { 0.0 };
    let a = p0 + d0 * s0;
    let b = p1 + d1 * s1;
    let horizontal = |d: Vec2| d.y == 0.0;
    let mid: Vec<Point> = match (horizontal(d0), horizontal(d1)) {
        (true, true) => {
            let mx = (a.x + b.x) / 2.0;
            vec![Point::new(mx, a.y), Point::new(mx, b.y)]
        }
        (false, false) => {
            let my = (a.y + b.y) / 2.0;
            vec![Point::new(a.x, my), Point::new(b.x, my)]
        }
        (true, false) => vec![Point::new(b.x, a.y)],
        (false, true) => vec![Point::new(a.x, b.y)],
    };
    let mut pts = vec![p0, a];
    pts.extend(mid);
    pts.push(b);
    pts.push(p1);
    simplify(pts)
}

/// The routed path of `c` on `page`: one line, axis-aligned lines, or one cubic, passing through
/// the connector's stored bends when it has any. A route the page's router stored is drawn in the
/// connector's style: straight segments, right angles, or right angles with the bends rounded as
/// far as stays clear of the shapes.
pub fn route_path(page: &Page, c: &Connector) -> Path {
    let ends = resolve_ends(page, c);
    path_in(page, c, &ends, &LazyField::new(page))
}

/// [`route_path`] for resolved ends, with the page's obstacles built at most once per caller.
pub(crate) fn path_in(page: &Page, c: &Connector, ends: &Ends, field: &LazyField) -> Path {
    if c.route == Route::Curved && stored_route(page, c) {
        let inner = inner_bends(ends, &c.waypoints);
        let mut pts = Vec::with_capacity(inner.len().saturating_add(2));
        pts.push(ends.start.pos);
        pts.extend(inner);
        pts.push(ends.end.pos);
        return rounded(c, &simplify(pts), field);
    }
    path_through(ends, c.route, &c.waypoints)
}

/// The finite bends of `waypoints` without the leading and trailing ones that sit on the ends: a
/// routed connector stores its end points with its bends.
fn inner_bends(ends: &Ends, waypoints: &[Point]) -> Vec<Point> {
    let wps: Vec<Point> = waypoints.iter().copied().filter(|p| p.is_finite()).collect();
    let (p0, p1) = (ends.start.pos, ends.end.pos);
    let lead = wps.iter().take_while(|p| p.distance(p0) < 1e-6).count();
    let inner = wps.get(lead..).unwrap_or_default();
    let trail = inner.iter().rev().take_while(|p| p.distance(p1) < 1e-6).count();
    inner.get(..inner.len().saturating_sub(trail)).unwrap_or_default().to_vec()
}

/// The polyline `pts` with each bend rounded by the largest arc that stays clear of the shapes
/// the connector must avoid.
fn rounded(c: &Connector, pts: &[Point], lazy: &LazyField) -> Path {
    let field = lazy.get();
    let ctx = field.context(c);
    let ignore = field.rounding_ignore(&ctx, pts);
    let rpts: Vec<vizcraft_routing::Point> = pts.iter().map(|p| vizcraft_routing::Point::new(p.x, p.y)).collect();
    // Arcs keep the page's clearance from other shapes, and only the drawing tolerance from the
    // connector's own end shapes, which its first and last bends sit one stub away from.
    let own: Vec<usize> = [ctx.start_own, ctx.end_own].into_iter().flatten().collect();
    let mut others = ignore.clone();
    others.extend(own.iter().copied());
    let clear = lazy.clearance().max(crate::autoroute::ARC_CLEARANCE);
    let wide = vizcraft_routing::round_bends(&rpts, &field.obstacles, &others, clear);
    let not_own: Vec<usize> = (0..field.obstacles.len()).filter(|k| !own.contains(k) || ignore.contains(k)).collect();
    let near = vizcraft_routing::round_bends(&rpts, &field.obstacles, &not_own, crate::autoroute::ARC_CLEARANCE);
    let reach = |b: &vizcraft_routing::Bend, i: usize| {
        rpts.get(i + 1).map_or(f64::INFINITY, |p| ((b.from.x - p.x).powi(2) + (b.from.y - p.y).powi(2)).sqrt())
    };
    let bends: Vec<Option<vizcraft_routing::Bend>> = wide
        .iter()
        .zip(&near)
        .enumerate()
        .map(|(i, (a, b))| match (a, b) {
            (Some(a), Some(b)) => Some(if reach(a, i) <= reach(b, i) { *a } else { *b }),
            _ => None,
        })
        .collect();
    let q = |p: vizcraft_routing::Point| Point::new(p.x, p.y);
    let mut path = Path::new();
    let Some(&first) = pts.first() else { return path };
    path.move_to(first);
    for (i, &p) in pts.iter().enumerate().skip(1) {
        match bends.get(i - 1).copied().flatten() {
            Some(b) if i + 1 < pts.len() => {
                path.line_to(q(b.from));
                path.cubic_to(q(b.c1), q(b.c2), q(b.to));
            }
            _ => {
                path.line_to(p);
            }
        }
    }
    path
}

/// The path for already resolved ends, with no stored bends.
pub fn path_for(ends: &Ends, route: Route) -> Path {
    path_through(ends, route, &[])
}

/// The path for already resolved ends through `waypoints` (non-finite ones, and ones on top of the
/// ends, are skipped). Without waypoints this is the automatic route. With them, a straight route
/// is a polyline through them,
/// an orthogonal one runs the start stub, the bends and the end stub with elbows added so every
/// segment is horizontal or vertical, and a curved one is a smooth curve through them.
pub fn path_through(ends: &Ends, route: Route, waypoints: &[Point]) -> Path {
    let (p0, p1) = (ends.start.pos, ends.end.pos);
    // Bends on top of the ends add nothing (and would turn an orthogonal stub back on itself).
    let wps = inner_bends(ends, waypoints);
    if wps.is_empty() {
        return match route {
            Route::Straight => Path::polyline(&[p0, p1]),
            Route::Orthogonal => Path::polyline(&orthogonal_points(&ends.start, &ends.end)),
            Route::Curved => {
                let k = (p0.distance(p1) * 0.4).clamp(0.0, 1.0e5);
                let d0 = ends.start.dir.or_else(|| (p1 - p0).normalized()).unwrap_or(Point::ZERO);
                let d1 = ends.end.dir.or_else(|| (p0 - p1).normalized()).unwrap_or(Point::ZERO);
                let mut p = Path::new();
                p.move_to(p0).cubic_to(p0 + d0 * k, p1 + d1 * k, p1);
                p
            }
        };
    }
    match route {
        Route::Straight => {
            let mut pts = Vec::with_capacity(wps.len().saturating_add(2));
            pts.push(p0);
            pts.extend(wps);
            pts.push(p1);
            Path::polyline(&simplify(pts))
        }
        Route::Orthogonal => Path::polyline(&orthogonal_through(&ends.start, &ends.end, &wps)),
        Route::Curved => curve_through(&ends.start, &ends.end, &wps),
    }
}

/// Whether `w` lies on the ray from `p` along axis direction `d` (so the stub is already there).
fn on_ray(p: Point, d: Vec2, w: Point) -> bool {
    let off = w - p;
    off.cross(d).abs() < 1e-6 && off.dot(d) > 0.0
}

/// An orthogonal route through `wps`: the start stub (unless the first bend already lies straight
/// out of the side), the bends, the end stub, with an elbow between any two points that are not
/// on a common horizontal or vertical line. Each elbow keeps the previous heading, which adds the
/// fewest turns.
fn orthogonal_through(start: &EndPoint, end: &EndPoint, wps: &[Point]) -> Vec<Point> {
    let (p0, p1) = (start.pos, end.pos);
    let (Some(&first), Some(&last)) = (wps.first(), wps.last()) else { return orthogonal_points(start, end) };
    let mut pts: Vec<Point> = Vec::with_capacity(wps.len().saturating_mul(2).saturating_add(6));
    pts.push(p0);
    if let Some(d0) = axis(start.dir)
        && !on_ray(p0, d0, first)
    {
        pts.push(p0 + d0 * STUB);
    }
    pts.extend(wps.iter().copied());
    if let Some(d1) = axis(end.dir)
        && !on_ray(p1, d1, last)
    {
        pts.push(p1 + d1 * STUB);
    }
    pts.push(p1);
    // The heading into the first point: out of the start side, else toward the next point.
    let mut heading = axis(start.dir).unwrap_or_else(|| axis_toward(p0, pts.get(1).copied().unwrap_or(p1)));
    let mut out: Vec<Point> = Vec::with_capacity(pts.len().saturating_mul(2));
    for p in pts {
        if let Some(&u) = out.last() {
            let d = p - u;
            if d.x.abs() > 1e-9 && d.y.abs() > 1e-9 {
                // Keep going the way we were heading, then turn.
                let elbow = if heading.y == 0.0 { Point::new(p.x, u.y) } else { Point::new(u.x, p.y) };
                out.push(elbow);
                heading = axis_toward(elbow, p);
            } else if d.length() > 1e-9 {
                heading = axis_toward(u, p);
            }
        }
        out.push(p);
    }
    simplify(out)
}

/// A smooth curve through the bends: one cubic per span, with tangents from the neighbouring
/// points (Catmull-Rom) and the glued sides' directions at the ends.
fn curve_through(start: &EndPoint, end: &EndPoint, wps: &[Point]) -> Path {
    let mut pts: Vec<Point> = Vec::with_capacity(wps.len().saturating_add(2));
    pts.push(start.pos);
    pts.extend(wps.iter().copied());
    pts.push(end.pos);
    let pts = simplify_repeats(pts);
    let n = pts.len();
    let at = |i: usize| pts.get(i).copied().unwrap_or(Point::ZERO);
    let tangent = |i: usize| -> Vec2 {
        if i == 0 {
            let span = at(0).distance(at(1));
            start.dir.map_or(at(1) - at(0), |d| d * span)
        } else if i + 1 >= n {
            let span = at(i).distance(at(i.saturating_sub(1)));
            end.dir.map_or(at(i) - at(i.saturating_sub(1)), |d| -d * span)
        } else {
            (at(i + 1) - at(i - 1)) * 0.5
        }
    };
    let mut path = Path::new();
    path.move_to(at(0));
    for i in 0..n.saturating_sub(1) {
        let (a, b) = (at(i), at(i + 1));
        path.cubic_to(a + tangent(i) * (1.0 / 3.0), b - tangent(i + 1) * (1.0 / 3.0), b);
    }
    path
}

/// Drop consecutive repeated points.
fn simplify_repeats(pts: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last().is_none_or(|l| l.distance(p) >= 1e-9) {
            out.push(p);
        }
    }
    out
}

/// The routed polyline of `c` (curves flattened), for hit testing and label placement.
pub fn route_polyline(page: &Page, c: &Connector) -> Vec<Point> {
    polyline_in(page, c, &LazyField::new(page))
}

/// [`route_polyline`] sharing the page's obstacles with other calls.
pub(crate) fn polyline_in(page: &Page, c: &Connector, field: &LazyField) -> Vec<Point> {
    path_in(page, c, &resolve_ends(page, c), field).flatten(0.5).into_iter().next().map(|pl| pl.points).unwrap_or_default()
}

pub use crate::arrows::{Arrowhead, arrowhead};

/// Everything needed to draw a connector.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectorGeometry {
    /// The resolved ends.
    pub ends: Ends,
    /// The full route, end to end.
    pub path: Path,
    /// The route as a polyline (curves flattened).
    pub polyline: Vec<Point>,
    /// The line to stroke: the route, shortened where a filled arrowhead covers it.
    pub line: Path,
    /// Arrowhead at the start, if any.
    pub start_head: Option<Arrowhead>,
    /// Arrowhead at the end, if any.
    pub end_head: Option<Arrowhead>,
    /// Where the label goes: halfway along the route (above it for a sequence message).
    pub label_pos: Point,
}

/// Direction of the last non-degenerate segment of `pts`, pointing toward the last point.
fn arrival(pts: &[Point]) -> Option<Vec2> {
    let last = *pts.last()?;
    pts.iter().rev().skip(1).find_map(|p| (last - *p).normalized())
}

/// Resolve, route and decorate `c` on `page`.
pub fn connector_geometry(page: &Page, c: &Connector) -> ConnectorGeometry {
    geometry_in(page, c, &LazyField::new(page))
}

/// [`connector_geometry`] sharing the page's obstacles with other calls.
pub(crate) fn geometry_in(page: &Page, c: &Connector, field: &LazyField) -> ConnectorGeometry {
    let ends = resolve_ends(page, c);
    let path = path_in(page, c, &ends, field);
    let polyline = path.flatten(0.5).into_iter().next().map(|pl| pl.points).unwrap_or_default();
    let reversed: Vec<Point> = polyline.iter().rev().copied().collect();
    let lw = c.style.line_width;
    let end_head = arrival(&polyline).and_then(|d| arrowhead(c.end_arrow, ends.end.pos, d, lw));
    let start_head = arrival(&reversed).and_then(|d| arrowhead(c.start_arrow, ends.start.pos, d, lw));
    let mut line = path.clone();
    shorten(&mut line, start_head.as_ref().map(|h| h.line_end), end_head.as_ref().map(|h| h.line_end));
    let label_pos = c
        .message
        .as_ref()
        .and_then(|_| crate::sequence::label_anchor_on(page, &polyline, &c.text, &c.style))
        .or_else(|| point_along(&polyline, 0.5))
        .unwrap_or(ends.start.pos);
    ConnectorGeometry { ends, path, polyline, line, start_head, end_head, label_pos }
}

/// Move the first and last points of a path to new positions (where an arrowhead begins).
fn shorten(path: &mut Path, start: Option<Point>, end: Option<Point>) {
    if let (Some(s), Some(PathEl::MoveTo(p))) = (start, path.els.first_mut()) {
        *p = s;
    }
    if let Some(e) = end {
        match path.els.last_mut() {
            Some(PathEl::LineTo(p)) | Some(PathEl::CubicTo(_, _, p)) => *p = e,
            _ => {}
        }
    }
}

#[cfg(test)]
mod waypoint_tests {
    use super::*;
    use crate::geom::cubic_point;
    use crate::model::Shape;

    /// A page that leaves routing to the connectors' own flags, so stored bends are the user's.
    fn page_with(route: Route, waypoints: Vec<Point>) -> (Page, Connector) {
        let mut page = Page::new("p", "p");
        page.routing.avoid = crate::routing_policy::Avoid::Never;
        page.shapes = vec![
            Shape { id: "a".into(), x: 0.0, y: 0.0, w: 60.0, h: 40.0, ..Shape::default() },
            Shape { id: "b".into(), x: 300.0, y: 200.0, w: 60.0, h: 40.0, ..Shape::default() },
        ];
        let mut c = Connector::new("c", End::dynamic("a"), End::dynamic("b"));
        c.route = route;
        c.waypoints = waypoints;
        (page, c)
    }

    #[test]
    fn orthogonal_waypoints_draw_axis_aligned_through_every_bend() {
        let wps = vec![Point::new(120.0, -50.0), Point::new(200.0, 330.0), Point::new(250.0, 290.0)];
        let (page, c) = page_with(Route::Orthogonal, wps.clone());
        let pts = route_polyline(&page, &c);
        for w in pts.windows(2) {
            assert!((w[0].x - w[1].x).abs() < 1e-9 || (w[0].y - w[1].y).abs() < 1e-9, "{pts:?}");
        }
        for wp in &wps {
            // A bend may sit in the middle of a straight run, so it is on the line, not a vertex.
            assert!(crate::geom::distance_to_polyline(*wp, &pts) < 1e-9, "{wp:?} missing from {pts:?}");
        }
    }

    #[test]
    fn a_stub_already_on_the_route_is_not_doubled_back() {
        // The first bend lies straight out of a's right side: no extra stub before it.
        let (page, c) = page_with(Route::Orthogonal, vec![Point::new(70.0, 20.0), Point::new(70.0, 220.0)]);
        let pts = route_polyline(&page, &c);
        assert_eq!(pts.first(), Some(&Point::new(60.0, 20.0)));
        assert_eq!(pts.get(1), Some(&Point::new(70.0, 20.0)));
    }

    #[test]
    fn dynamic_glue_faces_the_nearest_bend() {
        // A bend above-left of b pulls b's end to its top point instead of facing a.
        let (page, c) = page_with(Route::Straight, vec![Point::new(330.0, 100.0)]);
        let ends = resolve_ends(&page, &c);
        assert_eq!(ends.end.pos, Point::new(330.0, 200.0));
        let pts = route_polyline(&page, &c);
        assert_eq!(pts.len(), 3);
        assert_eq!(pts.get(1), Some(&Point::new(330.0, 100.0)));
    }

    #[test]
    fn curves_pass_through_their_bends() {
        let wp = Point::new(150.0, 250.0);
        let (page, c) = page_with(Route::Curved, vec![wp]);
        let path = route_path(&page, &c);
        let ends: Vec<Point> = path.els.iter().filter_map(|e| if let PathEl::CubicTo(_, _, p) = e { Some(*p) } else { None }).collect();
        assert_eq!(ends.len(), 2);
        assert_eq!(ends.first(), Some(&wp));
        if let (Some(PathEl::MoveTo(p0)), Some(PathEl::CubicTo(a, b, p1))) = (path.els.first(), path.els.get(1)) {
            assert!(cubic_point(*p0, *a, *b, *p1, 0.5).is_finite());
        }
    }

    #[test]
    fn without_waypoints_nothing_changes_and_junk_bends_are_skipped() {
        let (page, c) = page_with(Route::Orthogonal, Vec::new());
        let ends = resolve_ends(&page, &c);
        assert_eq!(route_path(&page, &c), path_for(&ends, Route::Orthogonal));
        let (page, c) = page_with(Route::Orthogonal, vec![Point::new(f64::NAN, 1.0)]);
        assert_eq!(route_path(&page, &c), path_for(&resolve_ends(&page, &c), Route::Orthogonal));
    }
}
