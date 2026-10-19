//! The looks ER notations give connectors and shapes beyond line ends: a line drawn differently
//! on each half (Barker's optional half is dashed, Chen's total participation is a double line),
//! a frame around the label (Chen's relationship diamond) and a double outline (a weak entity).
//!
//! The data lives on [`crate::Connector`] (`halves`, `label_frame`) and in
//! [`crate::Style::compound`]; this module holds the types and the geometry every renderer draws
//! from, so the canvas, SVG and PDF agree.

use serde::{Deserialize, Serialize};

use crate::Shape;
use crate::geom::{Path, Point, Rect, finite, polyline_length};
use crate::kind::ShapeKind;
use crate::serde_util::is_default;
use crate::style_fields::Dash;

/// How one half of a connector line is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Half {
    /// The dash of this half; `None` keeps the connector's `style.dash`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dash: Option<Dash>,
    /// Draw this half as two parallel lines.
    #[serde(skip_serializing_if = "is_default")]
    pub double: bool,
}

/// The two halves of a connector line, split at half its length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Halves {
    /// The half nearest the start.
    #[serde(skip_serializing_if = "is_default")]
    pub start: Half,
    /// The half nearest the end.
    #[serde(skip_serializing_if = "is_default")]
    pub end: Half,
}

impl Halves {
    /// Whether both halves draw as the plain line (nothing to split).
    pub fn is_plain(&self) -> bool {
        *self == Halves::default()
    }
}

/// The frame drawn around a connector's label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LabelFrame {
    /// The plain white label box. Unknown names load as this.
    #[default]
    Box,
    /// A diamond (a Chen relationship).
    Diamond,
    /// A diamond inside a diamond (a Chen identifying relationship).
    DoubleDiamond,
}

crate::serde_util::named_enum!(LabelFrame, LabelFrame::Box, {
    LabelFrame::Box => "box",
    LabelFrame::Diamond => "diamond",
    LabelFrame::DoubleDiamond => "doubleDiamond",
});

/// Most points [`split_half`] and [`parallel`] look at; routes are a handful of points.
const MAX_POINTS: usize = 4096;

/// `pts` cut at half its arc length: the start half and the end half, both including the cut
/// point. A route with no length gives the whole route twice.
pub fn split_half(pts: &[Point]) -> (Vec<Point>, Vec<Point>) {
    let pts: Vec<Point> = pts.iter().take(MAX_POINTS).map(|p| p.sanitized()).collect();
    let total = polyline_length(&pts);
    if pts.len() < 2 || total <= 0.0 || !total.is_finite() {
        return (pts.clone(), pts);
    }
    let mut left = total / 2.0;
    for i in 0..pts.len().saturating_sub(1) {
        let (Some(&a), Some(&b)) = (pts.get(i), pts.get(i + 1)) else { break };
        let d = a.distance(b);
        if left <= d && d > 0.0 {
            let cut = a.lerp(b, left / d);
            let mut first: Vec<Point> = pts.get(..=i).map(<[Point]>::to_vec).unwrap_or_default();
            first.push(cut);
            let mut second = vec![cut];
            second.extend(pts.get(i + 1..).unwrap_or_default());
            return (first, second);
        }
        left -= d;
    }
    (pts.clone(), pts.last().map(|p| vec![*p]).unwrap_or_default())
}

/// The two lines of a double line along `pts`, each `d` to one side, mitred at bends (a very
/// sharp bend is bevelled at four times `d` so the corner never shoots off). Repeated points are
/// skipped; fewer than two distinct points give two empty lines.
pub fn parallel(pts: &[Point], d: f64) -> (Vec<Point>, Vec<Point>) {
    let d = finite(d, 0.0).clamp(0.0, 1000.0);
    let mut clean: Vec<Point> = Vec::new();
    for p in pts.iter().take(MAX_POINTS).map(|p| p.sanitized()) {
        if clean.last().is_none_or(|q| q.distance(p) > 1e-9) {
            clean.push(p);
        }
    }
    if clean.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let normals: Vec<Point> = clean.windows(2).filter_map(|w| Some((*w.get(1)? - *w.first()?).normalized()?.perp())).collect();
    let mut left = Vec::with_capacity(clean.len());
    let mut right = Vec::with_capacity(clean.len());
    for (i, p) in clean.iter().enumerate() {
        let before = i.checked_sub(1).and_then(|j| normals.get(j));
        let after = normals.get(i);
        let offset = match (before, after) {
            (Some(a), Some(b)) => {
                let sum = *a + *b;
                let cos = a.dot(*b);
                match sum.normalized() {
                    // A mitre: along the bisector, long enough to keep distance `d` from both
                    // segments, capped for sharp bends.
                    Some(m) if cos > -0.9 => m * (d / ((1.0 + cos) / 2.0).sqrt()).min(4.0 * d),
                    _ => *b * d,
                }
            }
            (Some(n), None) | (None, Some(n)) => *n * d,
            (None, None) => Point::ZERO,
        };
        left.push(*p + offset);
        right.push(*p - offset);
    }
    (left, right)
}

/// The inset between a double outline's two lines for a line width.
pub fn inset(line_width: f64) -> f64 {
    2.5 * finite(line_width, 1.0).clamp(0.0, 100.0).max(1.0)
}

/// The inner outline of a double-outlined shape, in the shape's local frame coordinates (like
/// `Shape::local_outline`): its kind's outline inset by [`inset`] of its line width. Rectangles,
/// rounded rectangles, ellipses, diamonds and freeforms have one; other kinds, and frames too
/// small for the inset, give `None`.
pub fn inner_outline(s: &Shape) -> Option<Path> {
    let d = inset(s.style.line_width);
    let (w, h) = (finite(s.w, 0.0), finite(s.h, 0.0));
    if w <= 4.0 * d || h <= 4.0 * d {
        return None;
    }
    let shifted = |p: Path, dx: f64, dy: f64| p.transformed(&crate::Transform::translate(dx, dy));
    match s.kind {
        ShapeKind::Rectangle | ShapeKind::RoundedRectangle | ShapeKind::Ellipse => {
            let p = s.kind.outline(w - 2.0 * d, h - 2.0 * d);
            let p = if s.style.corner_radius > 0.0 { p.rounded_corners((s.style.corner_radius - d).max(0.0)) } else { p };
            Some(shifted(p, d, d))
        }
        ShapeKind::Diamond => {
            // Each edge moves in by `d`: the half-diagonals shrink by `d` times the edge length
            // over the other half-diagonal.
            let (a, b) = (w / 2.0, h / 2.0);
            let edge = (a * a + b * b).sqrt();
            let (a2, b2) = (a - d * edge / b, b - d * edge / a);
            if a2 <= 0.0 || b2 <= 0.0 {
                return None;
            }
            let p = Point::new;
            Some(Path::polygon(&[p(a, b - b2), p(a + a2, b), p(a, b + b2), p(a - a2, b)]))
        }
        ShapeKind::Freeform => {
            // Scaled about the centre: inside for any outline that is star-shaped about it.
            let (sx, sy) = ((w - 2.0 * d) / w, (h - 2.0 * d) / h);
            let t = crate::Transform::translate(-w / 2.0, -h / 2.0)
                .then(&crate::Transform::scale(sx, sy))
                .then(&crate::Transform::translate(w / 2.0, h / 2.0));
            Some(s.local_outline().transformed(&t))
        }
        _ => None,
    }
}

/// The frame around a label of `w` by `h` centred on `center`: the box itself, or a diamond
/// wide enough to hold the box (and a second one inset inside it for the double diamond).
pub fn label_frame_path(center: Point, w: f64, h: f64, frame: LabelFrame) -> Path {
    let (w, h) = (finite(w, 0.0).clamp(0.0, 1.0e5), finite(h, 0.0).clamp(0.0, 1.0e5));
    let c = center.sanitized();
    match frame {
        LabelFrame::Box => {
            let r = Rect::from_xywh(c.x - w / 2.0, c.y - h / 2.0, w, h);
            Path::polygon(&[Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)])
        }
        LabelFrame::Diamond | LabelFrame::DoubleDiamond => {
            // A diamond through the box's corners has twice the box's width and height.
            let (a, b) = (w.max(h * 0.5) + 6.0, h + 6.0);
            let diamond = |a: f64, b: f64| {
                Path::polygon(&[Point::new(c.x, c.y - b), Point::new(c.x + a, c.y), Point::new(c.x, c.y + b), Point::new(c.x - a, c.y)])
            };
            let mut p = diamond(a, b);
            if frame == LabelFrame::DoubleDiamond {
                let edge = (a * a + b * b).sqrt();
                let d = 4.0;
                p.els.extend(diamond(a - d * edge / b, b - d * edge / a).els);
            }
            p
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::point_in_polygon;

    fn pts(v: &[(f64, f64)]) -> Vec<Point> {
        v.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    #[test]
    fn split_half_cuts_orthogonal_and_diagonal_routes_at_half_their_length() {
        let route = pts(&[(0.0, 0.0), (100.0, 0.0), (100.0, 50.0)]);
        let (a, b) = split_half(&route);
        assert_eq!(a, pts(&[(0.0, 0.0), (75.0, 0.0)]));
        assert_eq!(b, pts(&[(75.0, 0.0), (100.0, 0.0), (100.0, 50.0)]));
        assert!((polyline_length(&a) - polyline_length(&b)).abs() < 1e-9);
        let diag = pts(&[(0.0, 0.0), (30.0, 40.0)]);
        let (a, b) = split_half(&diag);
        assert_eq!(a.last(), Some(&Point::new(15.0, 20.0)));
        assert_eq!(b.first(), Some(&Point::new(15.0, 20.0)));
        let (a, b) = split_half(&[Point::ZERO]);
        assert_eq!((a.len(), b.len()), (1, 1));
        let (a, _) = split_half(&[]);
        assert!(a.is_empty());
        let junk = [Point::new(f64::NAN, 1.0), Point::new(f64::INFINITY, 5.0)];
        let (a, b) = split_half(&junk);
        assert!(a.iter().chain(&b).all(|p| p.is_finite()));
    }

    #[test]
    fn parallel_lines_keep_their_distance_on_straight_runs_and_mitre_at_bends() {
        let route = pts(&[(0.0, 0.0), (100.0, 0.0), (100.0, 50.0)]);
        let (l, r) = parallel(&route, 2.0);
        assert_eq!((l.len(), r.len()), (3, 3));
        assert!((l[0].distance(r[0]) - 4.0).abs() < 1e-9);
        // At the right-angle bend the mitre sits on the diagonal, sqrt(2) times d away.
        assert!((l[1].distance(route[1]) - 2.0 * 2f64.sqrt()).abs() < 1e-9, "{:?}", l[1]);
        assert!((l[2].x - r[2].x).abs() - 4.0 < 1e-9);
        let diag = pts(&[(0.0, 0.0), (30.0, 40.0)]);
        let (l, r) = parallel(&diag, 5.0);
        assert!((l[0].distance(r[0]) - 10.0).abs() < 1e-9);
        assert!(((l[1] - l[0]).cross(diag[1] - diag[0])).abs() < 1e-6, "parallel to the route");
        // A reversal does not shoot off.
        let back = pts(&[(0.0, 0.0), (50.0, 0.0), (0.0, 0.0)]);
        let (l, _) = parallel(&back, 2.0);
        assert!(l.iter().all(|p| p.distance(Point::new(25.0, 0.0)) < 40.0));
        assert_eq!(parallel(&[Point::ZERO, Point::ZERO], 2.0), (Vec::new(), Vec::new()));
    }

    #[test]
    fn inner_outline_sits_inside_the_outline_for_every_kind_it_draws() {
        for kind in [ShapeKind::Rectangle, ShapeKind::RoundedRectangle, ShapeKind::Ellipse, ShapeKind::Diamond, ShapeKind::Freeform] {
            let mut s = Shape::new("s", kind, 0.0, 0.0, 120.0, 70.0);
            if kind == ShapeKind::Freeform {
                s.path = Some(crate::Freeform::new(&Path::polygon(&pts(&[(0.0, 0.0), (120.0, 10.0), (110.0, 70.0), (10.0, 60.0)])), true));
            }
            let inner = inner_outline(&s).unwrap_or_else(|| panic!("{kind:?}"));
            let outer: Vec<Point> = s.local_outline().flatten(0.1).into_iter().flat_map(|p| p.points).collect();
            let inner_pts: Vec<Point> = inner.flatten(0.1).into_iter().flat_map(|p| p.points).collect();
            assert!(!inner_pts.is_empty());
            for p in &inner_pts {
                assert!(point_in_polygon(*p, &outer), "{kind:?}: {p:?} is outside");
            }
        }
        assert!(inner_outline(&Shape::new("t", ShapeKind::Rectangle, 0.0, 0.0, 6.0, 6.0)).is_none(), "too small");
        assert!(inner_outline(&Shape::new("t", ShapeKind::Cloud, 0.0, 0.0, 100.0, 100.0)).is_none());
    }

    #[test]
    fn label_frames_hold_the_label_box() {
        let c = Point::new(50.0, 50.0);
        let b = label_frame_path(c, 40.0, 16.0, LabelFrame::Box).bounds().unwrap();
        assert_eq!((b.width(), b.height()), (40.0, 16.0));
        let d = label_frame_path(c, 40.0, 16.0, LabelFrame::Diamond);
        let poly: Vec<Point> = d.flatten(0.1).into_iter().flat_map(|p| p.points).collect();
        for corner in [(30.0, 42.0), (70.0, 42.0), (70.0, 58.0), (30.0, 58.0)] {
            assert!(point_in_polygon(Point::new(corner.0, corner.1), &poly), "{corner:?}");
        }
        let dd = label_frame_path(c, 40.0, 16.0, LabelFrame::DoubleDiamond);
        assert_eq!(dd.flatten(0.1).len(), 2);
        assert!(label_frame_path(Point::new(f64::NAN, 0.0), f64::INFINITY, -1.0, LabelFrame::Diamond).bounds().is_some_and(|b| b.x0.is_finite()));
    }

    #[test]
    fn defaults_are_left_out_of_files_and_junk_loads_as_the_default() {
        assert_eq!(serde_json::to_string(&Halves::default()).unwrap(), "{}");
        let h = Halves { start: Half { dash: Some(Dash::Dashed), double: false }, end: Half { dash: None, double: true } };
        let text = serde_json::to_string(&h).unwrap();
        assert_eq!(text, r#"{"start":{"dash":"dashed"},"end":{"double":true}}"#);
        assert_eq!(serde_json::from_str::<Halves>(&text).unwrap(), h);
        assert_eq!(serde_json::from_str::<LabelFrame>("\"oval\"").unwrap(), LabelFrame::Box);
        assert_eq!(LabelFrame::from_name("doubleDiamond"), Some(LabelFrame::DoubleDiamond));
    }
}
