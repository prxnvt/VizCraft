//! Glue and routing: where a connector's ends are and the path between them.
//!
//! Everything here is a pure function of the page. A connector stores only how it is glued; its
//! end points and route are derived from the shapes on every call, so moving, resizing or
//! rotating a shape moves its connectors with no extra bookkeeping.

use crate::geom::{Path, PathEl, Point, Vec2, point_along};
use crate::model::{Arrow, Connector, End, Page, Route};

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
        End::Glued { shape, point } => match page.shape(shape) {
            Some(s) => point.and_then(|i| s.connection_point_world(i)).unwrap_or_else(|| s.center()),
            None => Point::ZERO,
        },
    }
}

/// Resolve one end given a point near the other end.
fn resolve(page: &Page, e: &End, toward: Point) -> EndPoint {
    match e {
        End::Free { x, y } => EndPoint { pos: Point::new(*x, *y).sanitized(), dir: None, shape: None, point: None },
        End::Glued { shape, point } => {
            let Some(s) = page.shape(shape) else {
                // Glue to a shape that is gone: the commands detach ends before deleting, so this
                // only happens with a damaged file. Park the end at the origin rather than fail.
                return EndPoint { pos: Point::ZERO, dir: None, shape: None, point: None };
            };
            let fixed = point.filter(|i| s.connection_point_world(*i).is_some());
            let chosen = fixed.or_else(|| nearest_point(&s.connection_points_world(), toward));
            match chosen.and_then(|i| Some((i, s.connection_point_world(i)?))) {
                Some((i, pos)) => EndPoint { pos, dir: s.connection_direction(i), shape: Some(s.id.clone()), point: Some(i) },
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
/// the shape's connection point nearest the other end.
pub fn resolve_ends(page: &Page, c: &Connector) -> Ends {
    let start = resolve(page, &c.start, anchor(page, &c.end));
    let end = resolve(page, &c.end, start.pos);
    // A dynamic start was chosen against the other end's rough anchor; now that the end is
    // known exactly, choose again so both dynamic ends face each other.
    let start = if matches!(c.start, End::Glued { point: None, .. }) { resolve(page, &c.start, end.pos) } else { start };
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

/// The routed path of `c` on `page`: one line, axis-aligned lines, or one cubic.
pub fn route_path(page: &Page, c: &Connector) -> Path {
    let ends = resolve_ends(page, c);
    path_for(&ends, c.route)
}

/// The path for already resolved ends.
pub fn path_for(ends: &Ends, route: Route) -> Path {
    let (p0, p1) = (ends.start.pos, ends.end.pos);
    match route {
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
    }
}

/// The routed polyline of `c` (curves flattened), for hit testing and label placement.
pub fn route_polyline(page: &Page, c: &Connector) -> Vec<Point> {
    route_path(page, c).flatten(0.5).into_iter().next().map(|pl| pl.points).unwrap_or_default()
}

/// The geometry of one arrowhead.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrowhead {
    /// The head's outline (closed for triangle, diamond and circle; an open chevron for `Open`).
    pub path: Path,
    /// Whether to fill the outline with the line colour.
    pub filled: bool,
    /// Where the connector line should stop so it does not poke through a filled head.
    pub line_end: Point,
}

/// The arrowhead for an end at `tip`, with the line arriving along unit direction `dir` (pointing
/// into the tip). Size grows with the line width. `None` for [`Arrow::None`] or a bad direction.
pub fn arrowhead(arrow: Arrow, tip: Point, dir: Vec2, line_width: f64) -> Option<Arrowhead> {
    let dir = dir.normalized()?;
    let lw = if line_width.is_finite() { line_width.clamp(0.0, 100.0) } else { 1.0 };
    let len = 8.0 + 2.5 * lw;
    let half = len * 0.4;
    let back = tip - dir * len;
    let side = dir.perp() * half;
    match arrow {
        Arrow::None => None,
        Arrow::Triangle => Some(Arrowhead { path: Path::polygon(&[tip, back + side, back - side]), filled: true, line_end: back }),
        Arrow::Open => Some(Arrowhead { path: Path::polyline(&[back + side, tip, back - side]), filled: false, line_end: tip }),
        Arrow::Diamond => {
            let mid = tip - dir * (len * 0.6);
            let tail = tip - dir * (len * 1.2);
            Some(Arrowhead { path: Path::polygon(&[tip, mid + side, tail, mid - side]), filled: true, line_end: tail })
        }
        Arrow::Circle => {
            let r = half;
            let c = tip - dir * r;
            let path = crate::kind::ellipse(crate::geom::Rect::new(c.x - r, c.y - r, c.x + r, c.y + r));
            Some(Arrowhead { path, filled: true, line_end: tip - dir * (2.0 * r) })
        }
    }
}

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
    /// Where the label goes: halfway along the route.
    pub label_pos: Point,
}

/// Direction of the last non-degenerate segment of `pts`, pointing toward the last point.
fn arrival(pts: &[Point]) -> Option<Vec2> {
    let last = *pts.last()?;
    pts.iter().rev().skip(1).find_map(|p| (last - *p).normalized())
}

/// Resolve, route and decorate `c` on `page`.
pub fn connector_geometry(page: &Page, c: &Connector) -> ConnectorGeometry {
    let ends = resolve_ends(page, c);
    let path = path_for(&ends, c.route);
    let polyline = path.flatten(0.5).into_iter().next().map(|pl| pl.points).unwrap_or_default();
    let reversed: Vec<Point> = polyline.iter().rev().copied().collect();
    let lw = c.style.line_width;
    let end_head = arrival(&polyline).and_then(|d| arrowhead(c.end_arrow, ends.end.pos, d, lw));
    let start_head = arrival(&reversed).and_then(|d| arrowhead(c.start_arrow, ends.start.pos, d, lw));
    let mut line = path.clone();
    shorten(&mut line, start_head.as_ref().map(|h| h.line_end), end_head.as_ref().map(|h| h.line_end));
    let label_pos = point_along(&polyline, 0.5).unwrap_or(ends.start.pos);
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
