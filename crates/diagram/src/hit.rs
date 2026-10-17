//! Hit testing in world space: which shape or connector is under a point, what a marquee holds,
//! and which connection point a drag is near.

use crate::geom::{Point, Rect, distance_to_polyline, distance_to_segment, point_in_polygon};
use crate::model::{Connector, Page, Shape};
use crate::route::route_polyline;

/// Flattening tolerance for hit tests; finer than a pixel at 100% zoom.
const HIT_FLATTEN: f64 = 0.25;

/// What [`hit`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A shape, by id.
    Shape(String),
    /// A connector, by id.
    Connector(String),
}

fn tol(t: f64) -> f64 {
    if t.is_finite() { t.clamp(0.0, 1.0e4) } else { 0.0 }
}

/// Whether `p` is inside `shape`'s outline or within `tolerance` of it. Rotation is honoured by
/// testing against the outline in world space.
pub fn shape_contains(shape: &Shape, p: Point, tolerance: f64) -> bool {
    if !p.is_finite() {
        return false;
    }
    let t = tol(tolerance);
    if !shape.bounds().inflate(t).contains(p) {
        return false;
    }
    let lines = shape.outline_world().flatten(HIT_FLATTEN);
    lines.iter().any(|pl| pl.closed && point_in_polygon(p, &pl.points))
        || lines.iter().any(|pl| pl.segments().any(|(a, b)| distance_to_segment(p, a, b) <= t))
}

/// The topmost shape under `p` (last in z order first), within `tolerance` of its outline.
pub fn shape_at(page: &Page, p: Point, tolerance: f64) -> Option<&Shape> {
    page.shapes.iter().rev().find(|s| shape_contains(s, p, tolerance))
}

/// Whether `p` is within `tolerance` (plus half the line width) of `c`'s routed line.
pub fn connector_contains(page: &Page, c: &Connector, p: Point, tolerance: f64) -> bool {
    if !p.is_finite() {
        return false;
    }
    let reach = tol(tolerance) + c.style.line_width.clamp(0.0, 100.0) / 2.0;
    distance_to_polyline(p, &route_polyline(page, c)) <= reach
}

/// The topmost connector near `p`.
pub fn connector_at(page: &Page, p: Point, tolerance: f64) -> Option<&Connector> {
    page.connectors.iter().rev().find(|c| connector_contains(page, c, p, tolerance))
}

/// What is under `p`: connectors first (they are drawn above shapes), then shapes.
pub fn hit(page: &Page, p: Point, tolerance: f64) -> Option<Hit> {
    connector_at(page, p, tolerance).map(|c| Hit::Connector(c.id.clone())).or_else(|| shape_at(page, p, tolerance).map(|s| Hit::Shape(s.id.clone())))
}

/// Shapes whose rotated bounds lie entirely inside `rect` (marquee selection), in z order.
pub fn shapes_in_rect(page: &Page, rect: Rect) -> Vec<&Shape> {
    page.shapes.iter().filter(|s| rect.contains_rect(&s.bounds())).collect()
}

/// Connectors whose whole route lies inside `rect` (marquee selection), in z order.
pub fn connectors_in_rect(page: &Page, rect: Rect) -> Vec<&Connector> {
    page.connectors.iter().filter(|c| Rect::bounding(route_polyline(page, c)).is_some_and(|b| rect.contains_rect(&b))).collect()
}

/// The index of `shape`'s connection point nearest `p`, if within `tolerance`.
pub fn connection_point_at(shape: &Shape, p: Point, tolerance: f64) -> Option<usize> {
    let t = tol(tolerance);
    let pts = shape.connection_points_world();
    crate::route::nearest_point(&pts, p).filter(|i| pts.get(*i).is_some_and(|q| q.distance(p) <= t))
}
