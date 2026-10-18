//! Hit testing in world space: which shape or connector is under a point, what a marquee holds,
//! and which connection point a drag is near.
//!
//! Layers decide what can be hit (see [`crate::layer`]): clicks and marquees find only shapes
//! that are visible and not locked, and a connector end finds only shapes it may glue to. A
//! shape that cannot be hit lets the click through to whatever lies under it.

use crate::geom::{Point, Rect, distance_to_polyline, distance_to_segment, point_in_polygon};
use crate::layer;
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

/// The topmost selectable shape under `p` (last in z order first), within `tolerance` of its
/// outline. Hidden shapes and shapes on a locked layer are passed over.
pub fn shape_at(page: &Page, p: Point, tolerance: f64) -> Option<&Shape> {
    page.shapes.iter().rev().find(|s| layer::selectable(page, s) && shape_contains(s, p, tolerance))
}

/// The topmost shape under `p` that a connector end may glue to: visible and on no layer that
/// turns glue off. A locked shape still takes glue; gluing changes the connector, not the shape.
pub fn glue_shape_at(page: &Page, p: Point, tolerance: f64) -> Option<&Shape> {
    page.shapes.iter().rev().find(|s| layer::gluable(page, s) && shape_contains(s, p, tolerance))
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

/// What is under `p`: connectors first (they are drawn above shapes), then selectable shapes.
pub fn hit(page: &Page, p: Point, tolerance: f64) -> Option<Hit> {
    connector_at(page, p, tolerance).map(|c| Hit::Connector(c.id.clone())).or_else(|| shape_at(page, p, tolerance).map(|s| Hit::Shape(s.id.clone())))
}

/// Selectable shapes whose rotated bounds lie entirely inside `rect` (marquee selection), in z
/// order; hidden and locked shapes are left out.
pub fn shapes_in_rect(page: &Page, rect: Rect) -> Vec<&Shape> {
    page.shapes.iter().filter(|s| layer::selectable(page, s) && rect.contains_rect(&s.bounds())).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::ShapeKind;
    use crate::page_fields::Layer;

    /// Two stacked shapes: `under` on no layer, `top` above it on layer `l`.
    fn stacked(l: Layer) -> Page {
        let mut p = Page::new("p1", "Page-1");
        p.layers = vec![Layer { id: "l".into(), ..l }];
        p.shapes.push(Shape::new("under", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 100.0));
        let mut top = Shape::new("top", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
        top.layers = vec!["l".into()];
        p.shapes.push(top);
        p
    }

    fn all() -> Rect {
        Rect::new(-10.0, -10.0, 200.0, 200.0)
    }

    fn ids(v: Vec<&Shape>) -> Vec<&str> {
        v.into_iter().map(|s| s.id.as_str()).collect()
    }

    #[test]
    fn hidden_and_locked_shapes_are_not_hit_and_let_the_click_through() {
        let at = Point::new(50.0, 50.0);
        let open = stacked(Layer::default());
        assert_eq!(hit(&open, at, 1.0), Some(Hit::Shape("top".into())));
        assert_eq!(ids(shapes_in_rect(&open, all())), vec!["under", "top"]);
        for l in [Layer { visible: false, ..Layer::default() }, Layer { locked: true, ..Layer::default() }] {
            let p = stacked(l);
            assert_eq!(hit(&p, at, 1.0), Some(Hit::Shape("under".into())));
            assert_eq!(shape_at(&p, at, 1.0).map(|s| s.id.as_str()), Some("under"));
            assert_eq!(ids(shapes_in_rect(&p, all())), vec!["under"], "the marquee skips it");
        }
    }

    #[test]
    fn glue_skips_hidden_and_no_glue_shapes_but_not_locked_ones() {
        let at = Point::new(50.0, 50.0);
        let glue_id = |l: Layer| glue_shape_at(&stacked(l), at, 1.0).map(|s| s.id.clone());
        assert_eq!(glue_id(Layer::default()).as_deref(), Some("top"));
        assert_eq!(glue_id(Layer { glue: false, ..Layer::default() }).as_deref(), Some("under"));
        assert_eq!(glue_id(Layer { visible: false, ..Layer::default() }).as_deref(), Some("under"));
        assert_eq!(glue_id(Layer { locked: true, ..Layer::default() }).as_deref(), Some("top"));
    }
}
