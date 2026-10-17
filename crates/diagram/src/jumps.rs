//! Line jumps: the small hops a connector draws where it crosses a connector drawn before it.
//!
//! Hops depend on every connector on the page, so they come from a page-level function,
//! [`page_geometry`], which the canvas and the SVG export both draw from. Hit testing and label
//! placement keep using the plain route, which is where the line really runs.

use vizcraft_routing::{JumpOrder, Point as RPoint};

use crate::geom::{Path, PathEl, Point, Vec2, polyline_length};
use crate::model::Page;
use crate::route::{ConnectorGeometry, connector_geometry};

/// Radius of a hop, in world units.
pub const HOP_RADIUS: f64 = 5.0;

/// How far one quarter of a circle's control points sit from the ends, as a fraction of the
/// radius, so two cubics make a close semicircle.
const KAPPA: f64 = 0.552_284_75;

/// The geometry of every connector on `page`, in page order. Connectors with `jumps` set have
/// their `line` drawn with a semicircular hop at each place where they cross a connector earlier
/// in the list; every other field is exactly [`connector_geometry`]'s. Costs nothing extra when
/// no connector has `jumps` set.
pub fn page_geometry(page: &Page) -> Vec<ConnectorGeometry> {
    let mut geoms: Vec<ConnectorGeometry> = page.connectors.iter().map(|c| connector_geometry(page, c)).collect();
    // Only connectors up to the last one with jumps can matter: later ones are never hopped over.
    let Some(last) = page.connectors.iter().rposition(|c| c.jumps) else { return geoms };
    let polylines: Vec<Vec<RPoint>> =
        geoms.iter().take(last.saturating_add(1)).map(|g| g.polyline.iter().map(|p| RPoint::new(p.x, p.y)).collect()).collect();
    let found = vizcraft_routing::jumps(&polylines, JumpOrder::LaterOverEarlier);
    for ((g, c), jumps) in geoms.iter_mut().zip(&page.connectors).zip(found) {
        if !c.jumps || jumps.is_empty() {
            continue;
        }
        let hops: Vec<(usize, f64)> = jumps.iter().map(|j| (j.segment, j.distance)).collect();
        g.line = hopped_line(&g.polyline, &g.line, &hops, HOP_RADIUS);
    }
    geoms
}

/// The first and last points of a stroked line (where arrowheads shorten it).
fn line_ends(line: &Path) -> Option<(Point, Point)> {
    let start = match line.els.first() {
        Some(PathEl::MoveTo(p)) => *p,
        _ => return None,
    };
    let end = match line.els.last() {
        Some(PathEl::LineTo(p)) | Some(PathEl::CubicTo(_, _, p)) => *p,
        _ => return None,
    };
    Some((start, end))
}

/// The line along `polyline` from `line`'s first point to its last, with a hop of `radius` at
/// each `(segment, distance)` crossing. A hop that would not fit on its segment (too near a bend,
/// a shortened end or the previous hop) is left out rather than drawn across a corner.
pub fn hopped_line(polyline: &[Point], line: &Path, hops: &[(usize, f64)], radius: f64) -> Path {
    let Some((start, end)) = line_ends(line) else { return line.clone() };
    let n = polyline.len();
    if n < 2 || hops.is_empty() || polyline_length(polyline) <= 0.0 {
        return line.clone();
    }
    // How much of the first and last segments the arrowheads cover.
    let trim_start = polyline.first().map_or(0.0, |p| p.distance(start));
    let trim_end = polyline.last().map_or(0.0, |p| p.distance(end));
    let mut path = Path::new();
    path.move_to(start);
    let last_seg = n.saturating_sub(2);
    for seg in 0..n.saturating_sub(1) {
        let (Some(&a), Some(&b)) = (polyline.get(seg), polyline.get(seg + 1)) else { continue };
        let len = a.distance(b);
        let Some(d) = (b - a).normalized() else { continue };
        let lo = if seg == 0 { trim_start } else { 0.0 };
        let hi = if seg == last_seg { len - trim_end } else { len };
        let mut cursor = lo;
        for &(_, t) in hops.iter().filter(|(s, _)| *s == seg) {
            if !t.is_finite() || t - radius < cursor || t + radius > hi {
                continue;
            }
            let c = a + d * t;
            path.line_to(c - d * radius);
            semicircle(&mut path, c, d, radius);
            cursor = t + radius;
        }
        let stop = if seg == last_seg { end } else { b };
        path.line_to(stop);
    }
    path
}

/// The side a hop bulges to: up for a horizontal line, left for a vertical one, whichever way the
/// line runs, so crossings read the same everywhere.
fn hop_normal(d: Vec2) -> Vec2 {
    let n = d.perp();
    if n.y > 1e-9 || (n.y.abs() <= 1e-9 && n.x > 0.0) { -n } else { n }
}

/// Append a semicircle of `r` around `c` from `c - d*r` to `c + d*r` as two cubics.
fn semicircle(path: &mut Path, c: Point, d: Vec2, r: f64) {
    let n = hop_normal(d);
    let (a, top, b) = (c - d * r, c + n * r, c + d * r);
    let k = KAPPA * r;
    path.cubic_to(a + n * k, top - d * k, top);
    path.cubic_to(top + d * k, b + n * k, b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Connector, End, Route};

    fn crossing_page(first_jumps: bool, second_jumps: bool) -> Page {
        let mut page = Page::default();
        let mut h = Connector::new("h", End::free(Point::new(0.0, 100.0)), End::free(Point::new(200.0, 100.0)));
        h.route = Route::Straight;
        h.jumps = first_jumps;
        let mut v = Connector::new("v", End::free(Point::new(100.0, 0.0)), End::free(Point::new(100.0, 200.0)));
        v.route = Route::Straight;
        v.jumps = second_jumps;
        page.connectors = vec![h, v];
        page
    }

    fn cubics(p: &Path) -> Vec<Point> {
        p.els.iter().filter_map(|e| if let PathEl::CubicTo(_, _, q) = e { Some(*q) } else { None }).collect()
    }

    #[test]
    fn a_hop_appears_exactly_at_the_crossing_on_the_later_connector() {
        let page = crossing_page(true, true);
        let g = page_geometry(&page);
        // The first connector is drawn first, so it never hops.
        assert!(cubics(&g[0].line).is_empty());
        let hop = cubics(&g[1].line);
        assert_eq!(hop.len(), 2, "one semicircle is two cubics");
        // Vertical line: the hop bulges left, centred on the crossing at (100, 100).
        assert!((hop[0].x - (100.0 - HOP_RADIUS)).abs() < 1e-9 && (hop[0].y - 100.0).abs() < 1e-9);
        assert!((hop[1].y - (100.0 + HOP_RADIUS)).abs() < 1e-9 || (hop[1].y - (100.0 - HOP_RADIUS)).abs() < 1e-9);
        assert!((hop[1].x - 100.0).abs() < 1e-9);
        // The route itself is unchanged.
        assert_eq!(g[1].polyline, connector_geometry(&page, &page.connectors[1]).polyline);
    }

    #[test]
    fn only_connectors_with_jumps_hop() {
        let page = crossing_page(true, false);
        let g = page_geometry(&page);
        assert!(g.iter().all(|g| cubics(&g.line).is_empty()));
        let plain = crossing_page(false, false);
        assert_eq!(page_geometry(&plain), plain.connectors.iter().map(|c| connector_geometry(&plain, c)).collect::<Vec<_>>());
    }

    #[test]
    fn no_crossing_no_hop_and_hostile_input_is_safe() {
        let mut page = crossing_page(true, true);
        if let Some(c) = page.connectors.get_mut(1) {
            c.start = End::free(Point::new(300.0, 0.0));
            c.end = End::free(Point::new(300.0, 200.0));
        }
        assert!(page_geometry(&page).iter().all(|g| cubics(&g.line).is_empty()));
        let line = Path::polyline(&[Point::new(0.0, 0.0), Point::new(10.0, 0.0)]);
        let pts = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        // A hop too near the end, past the end, and on a segment that does not exist.
        let out = hopped_line(&pts, &line, &[(0, 2.0), (0, 50.0), (7, 1.0), (0, f64::NAN)], HOP_RADIUS);
        assert!(cubics(&out).is_empty());
        assert_eq!(hopped_line(&[], &Path::new(), &[(0, 1.0)], 5.0), Path::new());
    }
}
