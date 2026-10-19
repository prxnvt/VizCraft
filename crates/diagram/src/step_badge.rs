//! Step badges: the numbered circle a connector with a `step` shows (the order of requests on a
//! topology). The badge sits a quarter of the way along the drawn route; when the connector has a
//! label (drawn in the middle) and the route is too short for that, it moves toward the start so
//! the circle stays clear of the label box, and at last to the side of the line. The canvas and
//! every exporter place it from [`badge`].

use crate::geom::Point;
use crate::model::Connector;

/// Highest step a connector may show.
pub const MAX_STEP: u16 = 999;

/// Where along the route the badge sits, as a share of its length.
pub const ALONG: f64 = 0.25;

/// A step badge: a filled circle with the number in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    /// Centre, in world units.
    pub center: Point,
    /// Radius, in world units.
    pub radius: f64,
    /// The number as drawn.
    pub text: String,
}

/// The badge of `c` drawn along `route` (the connector's polyline in world units), or `None`
/// when it shows no step or the route is empty or not finite.
pub fn badge(c: &Connector, route: &[Point]) -> Option<Badge> {
    if c.step == 0 {
        return None;
    }
    let step = c.step.min(MAX_STEP);
    let mut center = along(route, ALONG)?;
    let text = step.to_string();
    let font = if c.style.font_size.is_finite() && c.style.font_size > 0.0 { c.style.font_size.min(72.0) } else { 12.0 };
    let digits = text.len() as f64;
    let radius = font * (0.55 + 0.2 * (digits - 1.0).max(0.0));
    let total = length(route);
    if !c.text.trim().is_empty() && total > 0.0 {
        let gap = GAP;
        let (lw, lh) = crate::connector_look::label_size(&c.text, &c.style);
        // The label box is centred on the route's midpoint; its reach along the route there.
        let (_, dir) = at_distance(route, total / 2.0)?;
        let extent = dir.0.abs() * lw / 2.0 + dir.1.abs() * lh / 2.0;
        let (lo, hi) = (radius + gap, total / 2.0 - extent - radius - gap);
        if lo <= hi {
            let d = (total * ALONG).clamp(lo, hi);
            center = at_distance(route, d)?.0;
        } else {
            // No room along the line: beside it, just past the start.
            let (p, dir) = at_distance(route, lo.min(total))?;
            // Beside the line, and clear of the label box when the label reaches this far.
            let perp = dir.0.abs() * lh / 2.0 + dir.1.abs() * lw / 2.0;
            let off = (radius + c.style.line_width.max(0.0) + gap).max(perp + radius + gap);
            center = if dir.0.abs() >= dir.1.abs() { Point::new(p.x, p.y - off) } else { Point::new(p.x - off, p.y) };
        }
    }
    Some(Badge { center, radius, text })
}

/// Clear space kept between a badge and the label or the line, in world units.
const GAP: f64 = 3.0;

fn length(route: &[Point]) -> f64 {
    route.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// The point `d` along `route` and the unit direction of the segment it lies on.
fn at_distance(route: &[Point], d: f64) -> Option<(Point, (f64, f64))> {
    let mut left = d.max(0.0);
    let mut last = None;
    for w in route.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = dist(a, b);
        if len <= 0.0 {
            continue;
        }
        let dir = ((b.x - a.x) / len, (b.y - a.y) / len);
        last = Some((b, dir));
        if len >= left {
            return Some((Point::new(a.x + dir.0 * left, a.y + dir.1 * left), dir));
        }
        left -= len;
    }
    last
}

/// The point at share `t` (0 to 1) of the length of `route`.
pub fn along(route: &[Point], t: f64) -> Option<Point> {
    let finite = |p: &Point| p.x.is_finite() && p.y.is_finite();
    if route.is_empty() || !route.iter().all(finite) {
        return None;
    }
    let total: f64 = route.windows(2).map(|w| dist(w[0], w[1])).sum();
    let first = route.first().copied()?;
    if !(total.is_finite() && total > 0.0) {
        return Some(first);
    }
    let mut left = total * t.clamp(0.0, 1.0);
    for w in route.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = dist(a, b);
        if d >= left && d > 0.0 {
            let k = left / d;
            return Some(Point::new(a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k));
        }
        left -= d;
    }
    route.last().copied()
}

fn dist(a: Point, b: Point) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::End;

    #[test]
    fn badge_sits_a_quarter_along_and_only_with_a_step() {
        let mut c = Connector::new("c", End::free(Point::new(0.0, 0.0)), End::free(Point::new(100.0, 0.0)));
        let route = [Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0)];
        assert_eq!(badge(&c, &route), None);
        c.step = 3;
        let b = badge(&c, &route).unwrap();
        assert_eq!((b.center, b.text.as_str()), (Point::new(50.0, 0.0), "3"));
        c.step = 120;
        assert!(badge(&c, &route).unwrap().radius > b.radius, "more digits, a wider badge");
        assert_eq!(badge(&c, &[]), None);
        assert_eq!(badge(&c, &[Point::new(f64::NAN, 0.0)]), None);
        assert_eq!(badge(&c, &[Point::new(5.0, 5.0)]).unwrap().center, Point::new(5.0, 5.0));
    }

    fn line(len: f64, text: &str) -> (Connector, Vec<Point>) {
        let mut c = Connector::new("c", End::free(Point::new(0.0, 0.0)), End::free(Point::new(len, 0.0)));
        c.step = 2;
        c.text = text.into();
        (c, vec![Point::new(0.0, 0.0), Point::new(len, 0.0)])
    }

    /// Whether the badge circle overlaps the label box centred at the route's midpoint.
    fn hits_label(b: &Badge, c: &Connector, mid: Point) -> bool {
        let (w, h) = crate::connector_look::label_size(&c.text, &c.style);
        let (dx, dy) = ((b.center.x - mid.x).abs() - w / 2.0, (b.center.y - mid.y).abs() - h / 2.0);
        dx.max(0.0).hypot(dy.max(0.0)) < b.radius
    }

    #[test]
    fn a_long_route_keeps_the_badge_at_a_quarter_with_a_label() {
        let (c, route) = line(400.0, "route");
        assert_eq!(badge(&c, &route).unwrap().center, Point::new(100.0, 0.0));
    }

    #[test]
    fn the_badge_stays_clear_of_the_label_on_short_horizontal_and_vertical_routes() {
        let (c, route) = line(60.0, "route");
        let b = badge(&c, &route).unwrap();
        assert!(!hits_label(&b, &c, Point::new(30.0, 0.0)), "{b:?}");
        let (c, route) = line(120.0, "route");
        let b = badge(&c, &route).unwrap();
        assert!(b.center.x < 30.0 && !hits_label(&b, &c, Point::new(60.0, 0.0)), "{b:?}");
        // A vertical route: the label's height is what matters.
        let (c, _) = line(60.0, "route");
        let route = [Point::new(0.0, 0.0), Point::new(0.0, 60.0)];
        let b = badge(&c, &route).unwrap();
        assert!(!hits_label(&b, &c, Point::new(0.0, 30.0)), "{b:?}");
    }

    #[test]
    fn steps_load_leniently_and_clamp() {
        let c: Connector = serde_json::from_str(r#"{"id": "c", "step": 5000}"#).unwrap();
        let mut c2 = c.clone();
        c2.sanitize();
        assert_eq!(c2.step, MAX_STEP);
        let c: Connector = serde_json::from_str(r#"{"id": "c", "step": -4}"#).unwrap();
        assert_eq!(c.step, 0);
        let text = serde_json::to_string(&Connector::default()).unwrap();
        assert!(!text.contains("step"), "no step, nothing written");
    }
}
