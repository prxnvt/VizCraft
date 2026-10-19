//! Step badges: the numbered circle a connector with a `step` shows (the order of requests on a
//! topology). The badge sits a quarter of the way along the drawn route, so it never covers the
//! label in the middle. The canvas and every exporter place it from [`badge`].

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
    let center = along(route, ALONG)?;
    let text = step.to_string();
    let font = if c.style.font_size.is_finite() && c.style.font_size > 0.0 { c.style.font_size.min(72.0) } else { 12.0 };
    let digits = text.len() as f64;
    let radius = font * (0.55 + 0.2 * (digits - 1.0).max(0.0));
    Some(Badge { center, radius, text })
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
