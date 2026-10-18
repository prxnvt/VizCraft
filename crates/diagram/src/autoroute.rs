//! Obstacle-avoiding connector routes, built on `vizcraft-routing`.
//!
//! Connectors with `avoid` set keep their bends in `waypoints`; [`refresh`] recomputes them after
//! every document change so they stay clear of the shapes. Routing is too costly to redo on every
//! frame, which is why the result is stored rather than derived like the other route styles.
//!
//! Every shape on the page is an obstacle (a rotated shape counts as its bounding box). Each end
//! is resolved as if the connector had no bends, so dynamic glue picks its connection point from
//! the shapes alone and the result does not depend on the bends it replaces; that keeps rerouting
//! deterministic (the same page always gives the same bends) and lets [`refresh`] write only the
//! connectors whose bends really changed.

use std::collections::{HashMap, HashSet};

use vizcraft_routing::{End as REnd, Obstacle, Options, Point as RPoint, Request, Side};

use crate::geom::{Point, Vec2};
use crate::model::{Document, Page};
use crate::route::{EndPoint, resolve_through};

/// Recompute the stored bends of every connector that avoids shapes, on every page. The engine
/// calls this after each command that changes the document, inside the same undo step. A page
/// with no such connector costs one scan of its connectors.
pub fn refresh(doc: &mut Document) {
    for page in &mut doc.pages {
        if !page.connectors.iter().any(|c| c.avoid) {
            continue;
        }
        let ids: Vec<String> = page.connectors.iter().filter(|c| c.avoid).map(|c| c.id.clone()).collect();
        reroute(page, Some(&ids));
    }
}

/// Route the connectors named by `ids` (every connector on the page when `None`) around the
/// page's shapes now, storing the bends in their `waypoints`. The connectors are routed together
/// in page order, so overlapping parallel runs are spread apart. Only bends that changed are
/// written. A connector whose route could not avoid the shapes gets no bends (it draws as the
/// automatic route for its style). Unknown ids are ignored.
pub fn reroute(page: &mut Page, ids: Option<&[String]>) {
    let wanted: Option<HashSet<&str>> = ids.map(|ids| ids.iter().map(String::as_str).collect());
    let chosen: Vec<usize> =
        page.connectors.iter().enumerate().filter(|(_, c)| wanted.as_ref().is_none_or(|w| w.contains(c.id.as_str()))).map(|(i, _)| i).collect();
    if chosen.is_empty() {
        return;
    }
    // Groups draw nothing and enclose their members, so they are not obstacles.
    let solid: Vec<&crate::model::Shape> = page.shapes.iter().filter(|s| !crate::arrange::is_group(s)).collect();
    let obstacles: Vec<Obstacle> = solid
        .iter()
        .map(|s| {
            let b = s.bounds();
            Obstacle::new(b.x0, b.y0, b.x1, b.y1)
        })
        .collect();
    let index_of: HashMap<&str, usize> = solid.iter().enumerate().map(|(i, s)| (s.id.as_str(), i)).collect();
    let requests: Vec<Request> = chosen
        .iter()
        .filter_map(|&i| page.connectors.get(i))
        .map(|c| {
            let ends = resolve_through(page, c, &[]);
            Request::new(end_of(&ends.start, &index_of), end_of(&ends.end, &index_of))
        })
        .collect();
    let routes = vizcraft_routing::route_all(&obstacles, &requests, &Options::default());
    for (&i, r) in chosen.iter().zip(routes) {
        let bends: Vec<Point> = if r.fallback || r.points.len() < 3 {
            Vec::new()
        } else {
            r.points.iter().skip(1).take(r.points.len().saturating_sub(2)).map(|p| Point::new(p.x, p.y)).collect()
        };
        if let Some(c) = page.connectors.get_mut(i)
            && c.waypoints != bends
        {
            c.waypoints = bends;
            c.waypoints.truncate(crate::model::MAX_WAYPOINTS);
        }
    }
}

/// Whether the drawn route of `c` stays out of every shape on the page except the ones its ends
/// are glued to (touching an edge is allowed). For tests and checks of rerouted connectors.
pub fn clear_of_shapes(page: &Page, c: &crate::model::Connector) -> bool {
    let solid: Vec<&crate::model::Shape> = page.shapes.iter().filter(|s| !crate::arrange::is_group(s)).collect();
    let obstacles: Vec<Obstacle> = solid
        .iter()
        .map(|s| {
            let b = s.bounds();
            Obstacle::new(b.x0, b.y0, b.x1, b.y1)
        })
        .collect();
    let ignore: Vec<usize> = solid.iter().enumerate().filter(|(_, s)| c.touches(&s.id)).map(|(i, _)| i).collect();
    let points = crate::route::route_polyline(page, c).into_iter().map(|p| RPoint::new(p.x, p.y)).collect();
    vizcraft_routing::clear_of_obstacles(&vizcraft_routing::Route { points, fallback: false }, &obstacles, &ignore, 0.0)
}

/// A resolved end as a routing end: its position, the side its direction leaves through, and the
/// glued shape's obstacle index.
fn end_of(e: &EndPoint, index_of: &HashMap<&str, usize>) -> REnd {
    let mut out = REnd::at(RPoint::new(e.pos.x, e.pos.y));
    out.side = e.dir.and_then(side_of);
    out.obstacle = e.shape.as_deref().and_then(|s| index_of.get(s).copied());
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Connector, End, Route, Shape};
    use crate::route::{connector_geometry, route_polyline};

    fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
        Shape { id: id.into(), x, y, w, h, ..Shape::default() }
    }

    /// Two shapes with a wall between them, and an orthogonal connector that avoids shapes.
    fn walled() -> Page {
        let mut page = Page::new("p", "p");
        page.shapes = vec![rect("a", 0.0, 100.0, 60.0, 40.0), rect("wall", 150.0, 0.0, 40.0, 260.0), rect("b", 300.0, 100.0, 60.0, 40.0)];
        let mut c = Connector::new("c", End::dynamic("a"), End::dynamic("b"));
        c.route = Route::Orthogonal;
        c.avoid = true;
        page.connectors = vec![c];
        page
    }

    fn obstacles(page: &Page) -> Vec<Obstacle> {
        page.shapes
            .iter()
            .map(|s| {
                let b = s.bounds();
                Obstacle::new(b.x0, b.y0, b.x1, b.y1)
            })
            .collect()
    }

    fn as_route(pts: &[Point]) -> vizcraft_routing::Route {
        vizcraft_routing::Route { points: pts.iter().map(|p| RPoint::new(p.x, p.y)).collect(), fallback: false }
    }

    #[test]
    fn rerouted_connectors_stay_clear_of_other_shapes() {
        let mut page = walled();
        let before = route_polyline(&page, &page.connectors[0]);
        assert!(!vizcraft_routing::clear_of_obstacles(&as_route(&before), &obstacles(&page), &[0, 2], 0.0), "the automatic route crosses the wall");
        reroute(&mut page, None);
        let c = &page.connectors[0];
        assert!(!c.waypoints.is_empty());
        let after = route_polyline(&page, c);
        assert!(vizcraft_routing::clear_of_obstacles(&as_route(&after), &obstacles(&page), &[0, 2], 0.0), "{after:?}");
        // Every drawn segment is horizontal or vertical.
        for w in after.windows(2) {
            assert!((w[0].x - w[1].x).abs() < 1e-9 || (w[0].y - w[1].y).abs() < 1e-9, "{after:?}");
        }
    }

    #[test]
    fn refresh_is_deterministic_and_skips_pages_without_avoid() {
        let mut doc = Document::default();
        if let Some(p) = doc.pages.first_mut() {
            *p = walled();
        }
        refresh(&mut doc);
        let once = doc.clone();
        refresh(&mut doc);
        assert_eq!(doc, once);
        // Without avoid, nothing is touched.
        let mut plain = Document::default();
        if let Some(p) = plain.pages.first_mut() {
            *p = walled();
            p.connectors[0].avoid = false;
        }
        let copy = plain.clone();
        refresh(&mut plain);
        assert_eq!(plain, copy);
    }

    #[test]
    fn hostile_pages_do_not_panic() {
        let mut page = walled();
        page.shapes.push(rect("huge", -1e7, -1e7, 1e6, 1e6));
        page.shapes.push(Shape { id: "nan".into(), x: f64::NAN, y: f64::INFINITY, ..Shape::default() });
        page.connectors.push(Connector::new("ghost", End::point("gone", 99), End::free(Point::new(f64::NAN, 0.0))));
        reroute(&mut page, None);
        for c in &page.connectors {
            let _ = connector_geometry(&page, c);
        }
        reroute(&mut page, Some(&["nope".to_string()]));
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
            let mut c = Connector::new(format!("c{i}"), End::dynamic(format!("s{i}")), End::dynamic(format!("s{j}")));
            c.route = Route::Orthogonal;
            c.avoid = true;
            page.connectors.push(c);
        }
        let mut doc = Document::default();
        if let Some(p) = doc.pages.first_mut() {
            *p = page;
        }
        let t = std::time::Instant::now();
        refresh(&mut doc);
        let first = t.elapsed();
        let t = std::time::Instant::now();
        refresh(&mut doc);
        let second = t.elapsed();
        let routed = doc.pages[0].connectors.iter().filter(|c| !c.waypoints.is_empty()).count();
        eprintln!("refresh 200 shapes, 200 avoid connectors: first {first:?}, again {second:?}, {routed} with bends");
        assert!(routed > 150, "most connectors found a route: {routed}");
    }
}
