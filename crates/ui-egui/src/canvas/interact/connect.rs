//! Connector ends: where a dropped end glues (a connection point, a shape, or free) and which
//! drops are worth a connector.

use serde_json::Value;
use vizcraft_diagram::{self as dg, End, Page, Point};

use super::GRAB;

/// Where a connector end dropped at `p` attaches: a connection point near `p` (point glue), else
/// the shape under `p` (dynamic glue), else nowhere (a free end).
pub fn target_end(page: &Page, p: Point, zoom: f32) -> End {
    let tol = f64::from(GRAB) / f64::from(zoom.max(0.01));
    match dg::shape_at(page, p, tol) {
        Some(s) => match dg::connection_point_at(s, p, tol * 1.5) {
            Some(i) => End::point(s.id.clone(), i),
            None => End::dynamic(s.id.clone()),
        },
        None => End::free(p),
    }
}

/// Whether a connector from `from` to `to` is worth adding: not a drop back onto the body of the
/// shape it started from.
pub(super) fn connect_ok(from: &End, to: &End) -> bool {
    match (from, to) {
        (End::Glued { shape: a, .. }, End::Glued { shape: b, point: None }) => a != b,
        (End::Glued { shape: a, point: Some(i) }, End::Glued { shape: b, point: Some(j) }) => a != b || i != j,
        _ => true,
    }
}

pub(super) fn end_json(e: &End) -> Value {
    serde_json::to_value(e).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_refuses_a_drop_on_its_own_shape() {
        assert!(!connect_ok(&End::point("s1", 0), &End::dynamic("s1")));
        assert!(!connect_ok(&End::point("s1", 0), &End::point("s1", 0)));
        assert!(connect_ok(&End::point("s1", 0), &End::point("s1", 2)));
        assert!(connect_ok(&End::point("s1", 0), &End::dynamic("s2")));
        assert!(connect_ok(&End::free(Point::ZERO), &End::free(Point::ZERO)));
    }
}
