//! Hit testing in world space: which shape or connector is under a point, what a marquee holds,
//! which band (compartment and row) of a shape is under a point, and which connection point a drag
//! is near.
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
/// testing against the outline in world space. Closed rings count even-odd, as they fill: a
/// point inside a ring that lies inside another (a hole) is outside the shape. Open rings (a
/// freeform's lines) are hit by distance only.
pub fn shape_contains(shape: &Shape, p: Point, tolerance: f64) -> bool {
    if !p.is_finite() {
        return false;
    }
    let t = tol(tolerance);
    if !shape.bounds().inflate(t).contains(p) {
        return false;
    }
    let lines = shape.outline_world().flatten(HIT_FLATTEN);
    let rings = lines.iter().filter(|pl| pl.closed && point_in_polygon(p, &pl.points)).count();
    rings % 2 == 1 || lines.iter().any(|pl| pl.segments().any(|(a, b)| distance_to_segment(p, a, b) <= t))
}

/// Whether a click may pick `s`: selectable by its layers and not locked against selection
/// (a select-locked shape is still reached by id).
fn clickable(page: &Page, s: &Shape) -> bool {
    layer::selectable(page, s) && !s.locks.select
}

/// The topmost selectable shape under `p` (last in z order first), within `tolerance` of its
/// outline. Hidden shapes, shapes on a locked layer and shapes locked against selection are
/// passed over.
pub fn shape_at(page: &Page, p: Point, tolerance: f64) -> Option<&Shape> {
    page.shapes.iter().rev().find(|s| clickable(page, s) && shape_contains(s, p, tolerance))
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
/// order; hidden, locked and select-locked shapes are left out.
pub fn shapes_in_rect(page: &Page, rect: Rect) -> Vec<&Shape> {
    page.shapes.iter().filter(|s| clickable(page, s) && rect.contains_rect(&s.bounds())).collect()
}

/// Connectors whose whole route lies inside `rect` (marquee selection), in z order.
pub fn connectors_in_rect(page: &Page, rect: Rect) -> Vec<&Connector> {
    page.connectors.iter().filter(|c| Rect::bounding(route_polyline(page, c)).is_some_and(|b| rect.contains_rect(&b))).collect()
}

/// Which band of a shape [`band_at`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandHit {
    /// The shape's id.
    pub shape: String,
    /// The compartment's id; `None` is the shape's main text.
    pub compartment: Option<String>,
    /// The list row under the point, by index.
    pub row: Option<usize>,
}

/// The band under `p` of `shape` (see [`crate::compartment::layout`]): its compartment and, in a
/// list, its row. A shape without compartments, or a point outside its frame, reports the main
/// text when `p` is on the shape and `None` otherwise.
pub fn band_of(shape: &Shape, p: Point, tolerance: f64) -> Option<BandHit> {
    let main = || BandHit { shape: shape.id.clone(), compartment: None, row: None };
    match crate::compartment::band_at(shape, p) {
        Some((i, row)) => {
            let compartment = i.and_then(|i| shape.compartments.get(i)).map(|c| c.id.clone());
            Some(BandHit { row: row.filter(|_| compartment.is_some()), compartment, ..main() })
        }
        None => shape_contains(shape, p, tolerance).then(main),
    }
}

/// The band under `p` of the topmost selectable shape there (see [`shape_at`] and [`band_of`]).
pub fn band_at(page: &Page, p: Point, tolerance: f64) -> Option<BandHit> {
    band_of(shape_at(page, p, tolerance)?, p, tolerance)
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
    fn band_at_reports_the_compartment_and_row_under_the_point() {
        use crate::compartment::{Compartment, CompartmentKind, layout};
        let mut p = Page::new("p1", "Page-1");
        let mut s = Shape::new("k", ShapeKind::Rectangle, 10.0, 10.0, 160.0, 120.0);
        s.text = "Order".into();
        let mut attrs = Compartment::new("attrs", CompartmentKind::List);
        attrs.rows = vec!["+ id".into(), "+ total".into()];
        s.compartments = vec![attrs, Compartment::new("notes", CompartmentKind::Paragraph)];
        s.sanitize();
        let bands = layout(&s);
        let world = |q: Point| Point::new(q.x + 10.0, q.y + 10.0);
        p.shapes.push(s);
        let at = |q: Point| band_at(&p, q, 1.0).map(|b| (b.compartment, b.row));
        assert_eq!(at(world(bands[1].rows[1].center())), Some((Some("attrs".into()), Some(1))));
        assert_eq!(at(world(bands[2].rect.center())), Some((Some("notes".into()), None)));
        assert_eq!(at(world(Point::new(80.0, 4.0))), Some((None, None)));
        assert_eq!(at(Point::new(-50.0, -50.0)), None);
        // No compartments: the main text.
        let plain = stacked(Layer::default());
        assert_eq!(band_at(&plain, Point::new(50.0, 50.0), 1.0).map(|b| (b.shape, b.compartment)), Some(("top".into(), None)));
    }

    #[test]
    fn a_freeform_with_a_hole_is_hit_only_on_the_ring() {
        use crate::geom::Path;
        use crate::shape_fields::Freeform;
        let mut path = Path::polygon(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0), Point::new(0.0, 100.0)]);
        path.els.extend(Path::polygon(&[Point::new(30.0, 30.0), Point::new(70.0, 30.0), Point::new(70.0, 70.0), Point::new(30.0, 70.0)]).els);
        // An open line across the top, outside both rings, hit by distance only.
        path.els.extend(Path::polyline(&[Point::new(0.0, -40.0), Point::new(100.0, -40.0)]).els);
        let mut s = Shape::new("f", ShapeKind::Freeform, 0.0, -40.0, 100.0, 140.0);
        s.path = Some(Freeform::new(&path, true));
        s.sanitize();
        assert!(shape_contains(&s, Point::new(10.0, 50.0), 0.0), "on the ring");
        assert!(!shape_contains(&s, Point::new(50.0, 50.0), 0.0), "in the hole");
        assert!(shape_contains(&s, Point::new(50.0, 50.0), 25.0), "near the hole's edge with a wide tolerance");
        assert!(shape_contains(&s, Point::new(50.0, -40.5), 1.0), "on the open line");
        assert!(!shape_contains(&s, Point::new(50.0, -20.0), 1.0), "an open line encloses nothing");
    }

    #[test]
    fn a_select_locked_shape_is_not_clicked_or_marqueed() {
        let at = Point::new(50.0, 50.0);
        let mut p = stacked(Layer::default());
        p.shapes[1].locks.select = true;
        assert_eq!(hit(&p, at, 1.0), Some(Hit::Shape("under".into())));
        assert_eq!(ids(shapes_in_rect(&p, all())), vec!["under"]);
        assert!(p.shape("top").is_some(), "still there by id");
        assert_eq!(glue_shape_at(&p, at, 1.0).map(|s| s.id.as_str()), Some("top"), "glue is not a click");
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
