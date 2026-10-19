//! Line markers: the small marks repeated along a line that say what it carries (ISA-5.1 line
//! symbols): two slashes for a pneumatic signal, crosses for a capillary, an `L` for a hydraulic
//! signal, small circles for a data link, a wave for an electromagnetic signal.
//!
//! One table, like [`crate::arrows::outline`]: [`marks`] turns a route into the marks' path in
//! world coordinates, and every renderer (the SVG and PDF exports now, the canvas later) strokes
//! that path in the line's own colour and width, so they all draw the same marks.

use crate::geom::{Path, Point, Vec2};

/// What a line carries, drawn as marks repeated along it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineMarker {
    /// No marks (the default).
    #[default]
    None,
    /// Two slashes: a pneumatic signal.
    DoubleSlash,
    /// A cross: a capillary tube.
    Cross,
    /// An `L`: a hydraulic signal.
    L,
    /// A small circle: a data link (software or shared data).
    Circle,
    /// A wave: an electromagnetic or sonic signal.
    Wave,
}

crate::serde_util::named_enum!(LineMarker, LineMarker::None, {
    LineMarker::None => "none",
    LineMarker::DoubleSlash => "doubleSlash",
    LineMarker::Cross => "cross",
    LineMarker::L => "l",
    LineMarker::Circle => "circle",
    LineMarker::Wave => "wave",
});

impl LineMarker {
    /// Every marker, in documentation order.
    pub const ALL: [LineMarker; 6] =
        [LineMarker::None, LineMarker::DoubleSlash, LineMarker::Cross, LineMarker::L, LineMarker::Circle, LineMarker::Wave];
}

/// Distance between marks, in line widths.
pub const SPACING: f64 = 12.0;

/// Most marks drawn on one line, so a huge line on a thin width stays cheap.
pub const MAX_MARKS: usize = 2000;

/// The marks of `marker` along the polyline `points` for a line of `line_width`, in world
/// coordinates: one every [`SPACING`] line widths, evenly spread so the first and last sit half
/// a gap from the ends, and at least one on any line long enough to hold it. Empty for
/// [`LineMarker::None`] or a degenerate line.
pub fn marks(points: &[Point], marker: LineMarker, line_width: f64) -> Path {
    let mut path = Path::new();
    if marker == LineMarker::None {
        return path;
    }
    let lw = if line_width.is_finite() { line_width.clamp(1.0, 100.0) } else { 1.0 };
    let pts: Vec<Point> = points.iter().copied().filter(|p| p.is_finite()).collect();
    let total: f64 = pts.windows(2).map(|w| w.first().zip(w.get(1)).map_or(0.0, |(a, b)| a.distance(*b))).sum();
    let size = 4.0 * lw;
    if !total.is_finite() || total < size {
        return path;
    }
    let gap = SPACING * lw;
    let count = ((total / gap).floor() as usize).clamp(1, MAX_MARKS);
    for i in 0..count {
        let at = total * (i as f64 + 0.5) / count as f64;
        if let Some((p, u)) = point_and_tangent(&pts, at) {
            mark(&mut path, marker, p, u, size);
        }
    }
    path
}

/// The point at distance `at` along `pts` and the unit direction of the segment it is on.
fn point_and_tangent(pts: &[Point], at: f64) -> Option<(Point, Vec2)> {
    let mut left = at;
    for w in pts.windows(2) {
        let (&a, &b) = (w.first()?, w.get(1)?);
        let len = a.distance(b);
        if len <= 1e-12 {
            continue;
        }
        if left <= len {
            return Some((a.lerp(b, left / len), (b - a).normalized()?));
        }
        left -= len;
    }
    let (&a, &b) = (pts.get(pts.len().checked_sub(2)?)?, pts.last()?);
    Some((b, (b - a).normalized()?))
}

/// One mark of `size` centred at `p` on a line running along `u`.
fn mark(path: &mut Path, marker: LineMarker, p: Point, u: Vec2, size: f64) {
    // `v` is the line's normal; marks are drawn in (along, across) coordinates.
    let v = u.perp();
    let at = |a: f64, b: f64| p + u * a + v * b;
    let h = size / 2.0;
    match marker {
        LineMarker::None => {}
        LineMarker::DoubleSlash => {
            for o in [-0.3 * size, 0.3 * size] {
                path.move_to(at(o - h * 0.5, h)).line_to(at(o + h * 0.5, -h));
            }
        }
        LineMarker::Cross => {
            path.move_to(at(-h, -h)).line_to(at(h, h));
            path.move_to(at(-h, h)).line_to(at(h, -h));
        }
        LineMarker::L => {
            // An upright L standing on the line's far side.
            path.move_to(at(-h * 0.5, -size * 1.4)).line_to(at(-h * 0.5, -size * 0.3)).line_to(at(h * 0.5, -size * 0.3));
        }
        LineMarker::Circle => {
            // Four cubics approximate the circle; 0.5523 is the usual quarter-arc handle.
            let r = h * 0.8;
            let k = 0.552_284_75 * r;
            path.move_to(at(r, 0.0));
            path.cubic_to(at(r, k), at(k, r), at(0.0, r));
            path.cubic_to(at(-k, r), at(-r, k), at(-r, 0.0));
            path.cubic_to(at(-r, -k), at(-k, -r), at(0.0, -r));
            path.cubic_to(at(k, -r), at(r, -k), at(r, 0.0));
            path.close();
        }
        LineMarker::Wave => {
            path.move_to(at(-size, 0.0));
            path.cubic_to(at(-size * 0.75, -h * 1.5), at(-size * 0.25, -h * 1.5), at(0.0, 0.0));
            path.cubic_to(at(size * 0.25, h * 1.5), at(size * 0.75, h * 1.5), at(size, 0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::PathEl;

    fn moves(p: &Path) -> usize {
        p.els.iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count()
    }

    #[test]
    fn marks_repeat_every_twelve_line_widths() {
        let line = [Point::new(0.0, 0.0), Point::new(240.0, 0.0)];
        // 240 long at width 2: a gap of 24, ten marks of two strokes each.
        assert_eq!(moves(&marks(&line, LineMarker::DoubleSlash, 2.0)), 20);
        assert_eq!(moves(&marks(&line, LineMarker::Cross, 2.0)), 20);
        assert_eq!(moves(&marks(&line, LineMarker::L, 2.0)), 10);
        assert!(marks(&line, LineMarker::None, 2.0).is_empty());
        // A short line still gets one mark; a dot of a line gets none.
        assert_eq!(moves(&marks(&[Point::new(0.0, 0.0), Point::new(10.0, 0.0)], LineMarker::Circle, 1.0)), 1);
        assert!(marks(&[Point::new(0.0, 0.0), Point::new(1.0, 0.0)], LineMarker::Wave, 1.0).is_empty());
    }

    #[test]
    fn marks_follow_bends_and_survive_junk() {
        let bent = [Point::new(0.0, 0.0), Point::new(0.0, 100.0), Point::new(100.0, 100.0)];
        let p = marks(&bent, LineMarker::Cross, 1.0);
        let b = p.bounds().unwrap();
        assert!(b.x0 < 0.0 && b.y1 > 100.0, "marks sit on both runs: {b:?}");
        let junk = [Point::new(f64::NAN, 0.0), Point::new(0.0, 0.0), Point::new(1e300, 0.0), Point::new(f64::INFINITY, 1.0)];
        let _ = marks(&junk, LineMarker::Wave, f64::NAN);
        assert!(moves(&marks(&[Point::new(0.0, 0.0), Point::new(1e9, 0.0)], LineMarker::DoubleSlash, 1.0)) <= 2 * MAX_MARKS);
    }

    #[test]
    fn names_round_trip() {
        for m in LineMarker::ALL {
            assert_eq!(LineMarker::from_name(m.name()), Some(m));
        }
        let m: LineMarker = serde_json::from_str("\"plasma\"").unwrap();
        assert_eq!(m, LineMarker::None);
    }
}
