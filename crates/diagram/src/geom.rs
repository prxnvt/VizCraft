//! Geometry primitives: points, rectangles, affine transforms and paths of lines and cubics.
//!
//! Everything is `f64` in world units (pixels at 100% zoom, y pointing down). `f64` rather than
//! `f32` because diagrams can be large and connectors are derived from shape positions on every
//! frame; the extra precision keeps glued ends from drifting. Functions here never panic: they
//! guard against NaN and infinities and cap the work they do on hostile input.

use serde::{Deserialize, Serialize};

use crate::serde_util::num;

/// Largest coordinate magnitude the model keeps; anything further out is clamped.
pub const MAX_COORD: f64 = 1.0e7;

/// Replace a non-finite value with `fallback` and clamp to `[-MAX_COORD, MAX_COORD]`.
pub fn finite(v: f64, fallback: f64) -> f64 {
    if v.is_finite() { v.clamp(-MAX_COORD, MAX_COORD) } else { fallback }
}

/// A point (or a vector: the same type serves both, with the usual operators).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Point {
    /// Horizontal position, growing to the right.
    #[serde(deserialize_with = "num")]
    pub x: f64,
    /// Vertical position, growing downward.
    #[serde(deserialize_with = "num")]
    pub y: f64,
}

/// A displacement. Same representation as [`Point`]; the alias documents intent.
pub type Vec2 = Point;

impl Point {
    /// The origin.
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    /// A point at `(x, y)`.
    pub const fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
    /// Dot product, treating both as vectors.
    pub fn dot(self, o: Point) -> f64 {
        self.x * o.x + self.y * o.y
    }
    /// 2D cross product (z of the 3D cross product).
    pub fn cross(self, o: Point) -> f64 {
        self.x * o.y - self.y * o.x
    }
    /// Length as a vector.
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    /// Distance to another point.
    pub fn distance(self, o: Point) -> f64 {
        (self - o).length()
    }
    /// Unit vector in the same direction, or `None` for a (near) zero or non-finite vector.
    pub fn normalized(self) -> Option<Point> {
        let l = self.length();
        if l.is_finite() && l > 1e-12 { Some(Point::new(self.x / l, self.y / l)) } else { None }
    }
    /// Rotated 90 degrees (clockwise on screen, since y points down).
    pub fn perp(self) -> Point {
        Point::new(-self.y, self.x)
    }
    /// Linear interpolation: `t = 0` is `self`, `t = 1` is `o`.
    pub fn lerp(self, o: Point, t: f64) -> Point {
        Point::new(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
    }
    /// Midpoint between two points.
    pub fn midpoint(self, o: Point) -> Point {
        self.lerp(o, 0.5)
    }
    /// Whether both coordinates are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
    /// Finite and clamped to the model range; non-finite coordinates become 0.
    pub fn sanitized(self) -> Point {
        Point::new(finite(self.x, 0.0), finite(self.y, 0.0))
    }
}

impl std::ops::Add for Point {
    type Output = Point;
    fn add(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }
}
impl std::ops::Sub for Point {
    type Output = Point;
    fn sub(self, o: Point) -> Point {
        Point::new(self.x - o.x, self.y - o.y)
    }
}
impl std::ops::Mul<f64> for Point {
    type Output = Point;
    fn mul(self, k: f64) -> Point {
        Point::new(self.x * k, self.y * k)
    }
}
impl std::ops::Neg for Point {
    type Output = Point;
    fn neg(self) -> Point {
        Point::new(-self.x, -self.y)
    }
}
impl std::ops::AddAssign for Point {
    fn add_assign(&mut self, o: Point) {
        self.x += o.x;
        self.y += o.y;
    }
}

/// An axis-aligned rectangle from `(x0, y0)` (top left) to `(x1, y1)` (bottom right).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rect {
    /// Left edge.
    #[serde(deserialize_with = "num")]
    pub x0: f64,
    /// Top edge.
    #[serde(deserialize_with = "num")]
    pub y0: f64,
    /// Right edge.
    #[serde(deserialize_with = "num")]
    pub x1: f64,
    /// Bottom edge.
    #[serde(deserialize_with = "num")]
    pub y1: f64,
}

impl Rect {
    /// A rectangle from its corners (normalized so `x0 <= x1` and `y0 <= y1`).
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Rect { x0: x0.min(x1), y0: y0.min(y1), x1: x0.max(x1), y1: y0.max(y1) }
    }
    /// A rectangle from its top-left corner and size.
    pub fn from_xywh(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect::new(x, y, x + w, y + h)
    }
    /// The smallest rectangle holding two points (a marquee drag, for example).
    pub fn from_points(a: Point, b: Point) -> Self {
        Rect::new(a.x, a.y, b.x, b.y)
    }
    /// The bounding box of some points; `None` when there are none or none is finite.
    pub fn bounding(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
        let mut out: Option<Rect> = None;
        for p in points.into_iter().filter(|p| p.is_finite()) {
            out = Some(match out {
                None => Rect { x0: p.x, y0: p.y, x1: p.x, y1: p.y },
                Some(r) => Rect { x0: r.x0.min(p.x), y0: r.y0.min(p.y), x1: r.x1.max(p.x), y1: r.y1.max(p.y) },
            });
        }
        out
    }
    /// Width.
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }
    /// Height.
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }
    /// Centre point.
    pub fn center(&self) -> Point {
        Point::new((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }
    /// Whether `p` lies inside or on the edge.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x0 && p.x <= self.x1 && p.y >= self.y0 && p.y <= self.y1
    }
    /// Whether `o` lies entirely inside this rectangle.
    pub fn contains_rect(&self, o: &Rect) -> bool {
        o.x0 >= self.x0 && o.x1 <= self.x1 && o.y0 >= self.y0 && o.y1 <= self.y1
    }
    /// Whether the two rectangles overlap (touching counts).
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x0 <= o.x1 && o.x0 <= self.x1 && self.y0 <= o.y1 && o.y0 <= self.y1
    }
    /// The smallest rectangle holding both.
    pub fn union(&self, o: &Rect) -> Rect {
        Rect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }
    /// Grown by `d` on every side (shrunk for negative `d`, never past its centre).
    pub fn inflate(&self, d: f64) -> Rect {
        let c = self.center();
        Rect { x0: (self.x0 - d).min(c.x), y0: (self.y0 - d).min(c.y), x1: (self.x1 + d).max(c.x), y1: (self.y1 + d).max(c.y) }
    }
}

/// A 2D affine transform `[a b c d e f]`, mapping `(x, y)` to `(a x + c y + e, b x + d y + f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// The six coefficients in SVG `matrix(a b c d e f)` order.
    pub m: [f64; 6],
}

impl Default for Transform {
    fn default() -> Self {
        Transform::IDENTITY
    }
}

impl Transform {
    /// The identity.
    pub const IDENTITY: Transform = Transform { m: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] };

    /// A translation.
    pub fn translate(dx: f64, dy: f64) -> Self {
        Transform { m: [1.0, 0.0, 0.0, 1.0, dx, dy] }
    }
    /// A scale about the origin.
    pub fn scale(sx: f64, sy: f64) -> Self {
        Transform { m: [sx, 0.0, 0.0, sy, 0.0, 0.0] }
    }
    /// A rotation about the origin by `deg` degrees. Positive turns clockwise on screen, because
    /// y points down in world space.
    pub fn rotate(deg: f64) -> Self {
        let r = if deg.is_finite() { deg.to_radians() } else { 0.0 };
        let (s, c) = r.sin_cos();
        Transform { m: [c, s, -s, c, 0.0, 0.0] }
    }
    /// A rotation by `deg` degrees about `center`.
    pub fn rotate_about(center: Point, deg: f64) -> Self {
        Transform::translate(-center.x, -center.y).then(&Transform::rotate(deg)).then(&Transform::translate(center.x, center.y))
    }
    /// This transform followed by `next` (apply `self` first).
    pub fn then(&self, next: &Transform) -> Transform {
        let [a, b, c, d, e, f] = self.m;
        let [a2, b2, c2, d2, e2, f2] = next.m;
        Transform { m: [a2 * a + c2 * b, b2 * a + d2 * b, a2 * c + c2 * d, b2 * c + d2 * d, a2 * e + c2 * f + e2, b2 * e + d2 * f + f2] }
    }
    /// Map a point.
    pub fn apply(&self, p: Point) -> Point {
        let [a, b, c, d, e, f] = self.m;
        Point::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }
    /// Map a vector (no translation).
    pub fn apply_vector(&self, v: Vec2) -> Vec2 {
        let [a, b, c, d, _, _] = self.m;
        Point::new(a * v.x + c * v.y, b * v.x + d * v.y)
    }
    /// The inverse, or `None` for a singular (or non-finite) transform.
    pub fn inverse(&self) -> Option<Transform> {
        let [a, b, c, d, e, f] = self.m;
        let det = a * d - b * c;
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        let (ia, ib, ic, id) = (d / det, -b / det, -c / det, a / det);
        Some(Transform { m: [ia, ib, ic, id, -(ia * e + ic * f), -(ib * e + id * f)] })
    }
}

/// One path command. Coordinates are absolute.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathEl {
    /// Start a new subpath.
    MoveTo(Point),
    /// Straight line to the point.
    LineTo(Point),
    /// Cubic Bezier with two control points, ending at the third point.
    CubicTo(Point, Point, Point),
    /// Close the current subpath back to its start.
    Close,
}

/// A polyline produced by flattening one subpath.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline {
    /// The points, in order. A closed subpath does not repeat its first point.
    pub points: Vec<Point>,
    /// Whether the subpath was closed.
    pub closed: bool,
}

impl Polyline {
    /// The segments, including the closing one for a closed polyline.
    pub fn segments(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        let n = self.points.len();
        let closing = if self.closed && n > 2 { self.points.last().copied().zip(self.points.first().copied()) } else { None };
        self.points.windows(2).filter_map(|w| Some((*w.first()?, *w.get(1)?))).chain(closing)
    }
}

/// The points of a subpath made only of a MoveTo and LineTos (plus an optional Close), with a
/// closing point that repeats the start dropped; `None` when it has curves.
fn straight_subpath(sub: &[PathEl]) -> Option<(Vec<Point>, bool)> {
    let mut pts = Vec::with_capacity(sub.len());
    let mut closed = false;
    for el in sub {
        match *el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) if !closed => pts.push(p),
            PathEl::Close => closed = true,
            _ => return None,
        }
    }
    if closed && pts.len() > 1 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9) {
        pts.pop();
    }
    Some((pts, closed))
}

/// Append `pts` to `out` with each corner replaced by a cubic of radius up to `r`; an open
/// polyline keeps its two end points sharp.
fn round_polygon(out: &mut Path, pts: &[Point], closed: bool, r: f64) {
    // Control points at this fraction of the way to the corner approximate a circular arc.
    const K: f64 = 0.552_284_75;
    let n = pts.len();
    let corner = |k: usize| -> Option<(Point, Point, Point)> {
        let v = *pts.get(k)?;
        let prev = *pts.get(if k == 0 { n.checked_sub(1)? } else { k - 1 })?;
        let next = *pts.get(if k + 1 == n { 0 } else { k + 1 })?;
        let (d1, d2) = (v.distance(prev), v.distance(next));
        if d1 < 1e-9 || d2 < 1e-9 {
            return None;
        }
        let rr = r.min(d1 / 2.0).min(d2 / 2.0);
        Some((v.lerp(prev, rr / d1), v, v.lerp(next, rr / d2)))
    };
    let rounded = |k: usize| if closed || (k > 0 && k + 1 < n) { corner(k) } else { None };
    for k in 0..n {
        let Some(&v) = pts.get(k) else { continue };
        match rounded(k) {
            Some((a, v, b)) => {
                if k == 0 {
                    out.move_to(a);
                } else {
                    out.line_to(a);
                }
                out.cubic_to(a.lerp(v, K), b.lerp(v, K), b);
            }
            None if k == 0 => {
                out.move_to(v);
            }
            None => {
                out.line_to(v);
            }
        }
    }
    if closed {
        out.close();
    }
}

/// Most segments one cubic is split into when flattening, so hostile tolerances stay cheap.
const MAX_CUBIC_STEPS: usize = 64;

/// A path made of lines and cubic Beziers, in one or more subpaths.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Path {
    /// The commands, in order.
    pub els: Vec<PathEl>,
}

impl Path {
    /// An empty path.
    pub fn new() -> Self {
        Path::default()
    }
    /// Start a subpath.
    pub fn move_to(&mut self, p: Point) -> &mut Self {
        self.els.push(PathEl::MoveTo(p));
        self
    }
    /// Line to `p`.
    pub fn line_to(&mut self, p: Point) -> &mut Self {
        self.els.push(PathEl::LineTo(p));
        self
    }
    /// Cubic to `p` with control points `c1` and `c2`.
    pub fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) -> &mut Self {
        self.els.push(PathEl::CubicTo(c1, c2, p));
        self
    }
    /// Close the current subpath.
    pub fn close(&mut self) -> &mut Self {
        self.els.push(PathEl::Close);
        self
    }
    /// A closed polygon through `points` (empty path for no points).
    pub fn polygon(points: &[Point]) -> Path {
        let mut p = Path::new();
        let mut it = points.iter();
        if let Some(first) = it.next() {
            p.move_to(*first);
            for q in it {
                p.line_to(*q);
            }
            p.close();
        }
        p
    }
    /// An open polyline through `points`.
    pub fn polyline(points: &[Point]) -> Path {
        let mut p = Path::new();
        let mut it = points.iter();
        if let Some(first) = it.next() {
            p.move_to(*first);
            for q in it {
                p.line_to(*q);
            }
        }
        p
    }
    /// Whether there are no commands.
    pub fn is_empty(&self) -> bool {
        self.els.is_empty()
    }
    /// A copy with the corners of straight-sided subpaths rounded to radius `r` (each corner's
    /// radius shrinks to half its shorter side). Subpaths with curves, and any `r` that is not a
    /// positive finite number, are left as they are.
    pub fn rounded_corners(&self, r: f64) -> Path {
        if !(r.is_finite() && r > 0.0) {
            return self.clone();
        }
        let mut out = Path::new();
        let mut i = 0;
        while i < self.els.len() {
            // One subpath: a MoveTo and everything up to the next MoveTo.
            let mut j = i + 1;
            while j < self.els.len() && !matches!(self.els.get(j), Some(PathEl::MoveTo(_))) {
                j += 1;
            }
            let sub = self.els.get(i..j).unwrap_or_default();
            match straight_subpath(sub) {
                Some((pts, closed)) if pts.len() >= 3 => round_polygon(&mut out, &pts, closed, r),
                _ => out.els.extend_from_slice(sub),
            }
            i = j;
        }
        out
    }
    /// A copy with every point mapped through `t`.
    pub fn transformed(&self, t: &Transform) -> Path {
        let els = self
            .els
            .iter()
            .map(|e| match *e {
                PathEl::MoveTo(p) => PathEl::MoveTo(t.apply(p)),
                PathEl::LineTo(p) => PathEl::LineTo(t.apply(p)),
                PathEl::CubicTo(a, b, p) => PathEl::CubicTo(t.apply(a), t.apply(b), t.apply(p)),
                PathEl::Close => PathEl::Close,
            })
            .collect();
        Path { els }
    }
    /// Exact bounding box (cubic extrema included), or `None` for an empty path.
    pub fn bounds(&self) -> Option<Rect> {
        let mut pts = Vec::new();
        let mut cur = Point::ZERO;
        for e in &self.els {
            match *e {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => {
                    pts.push(p);
                    cur = p;
                }
                PathEl::CubicTo(a, b, p) => {
                    pts.push(cur);
                    pts.push(p);
                    for t in cubic_extrema(cur, a, b, p) {
                        pts.push(cubic_point(cur, a, b, p, t));
                    }
                    cur = p;
                }
                PathEl::Close => {}
            }
        }
        Rect::bounding(pts)
    }
    /// Flatten into polylines, one per subpath, with curves split until each chord is within
    /// `tolerance` world units (clamped to a sane range so hostile input stays cheap).
    pub fn flatten(&self, tolerance: f64) -> Vec<Polyline> {
        let tol = if tolerance.is_finite() { tolerance.clamp(0.01, 100.0) } else { 0.25 };
        let mut out: Vec<Polyline> = Vec::new();
        let mut cur = Polyline::default();
        let mut pen = Point::ZERO;
        let mut start = Point::ZERO;
        let flush = |cur: &mut Polyline, out: &mut Vec<Polyline>| {
            if !cur.points.is_empty() {
                out.push(std::mem::take(cur));
            }
        };
        for e in &self.els {
            match *e {
                PathEl::MoveTo(p) => {
                    flush(&mut cur, &mut out);
                    cur.points.push(p);
                    pen = p;
                    start = p;
                }
                PathEl::LineTo(p) => {
                    if cur.points.is_empty() {
                        cur.points.push(pen);
                    }
                    cur.points.push(p);
                    pen = p;
                }
                PathEl::CubicTo(a, b, p) => {
                    if cur.points.is_empty() {
                        cur.points.push(pen);
                    }
                    // Control polygon length bounds the curve length; it picks a step count.
                    let hull = pen.distance(a) + a.distance(b) + b.distance(p);
                    let steps = if hull.is_finite() { ((hull / tol).sqrt().ceil() as usize).clamp(1, MAX_CUBIC_STEPS) } else { 1 };
                    for i in 1..=steps {
                        cur.points.push(cubic_point(pen, a, b, p, i as f64 / steps as f64));
                    }
                    pen = p;
                }
                PathEl::Close => {
                    if !cur.points.is_empty() {
                        // Drop a repeated start point so closed polylines never double up.
                        if cur.points.len() > 1 && cur.points.last().is_some_and(|l| l.distance(start) < 1e-9) {
                            cur.points.pop();
                        }
                        cur.closed = true;
                        flush(&mut cur, &mut out);
                    }
                    pen = start;
                }
            }
        }
        flush(&mut cur, &mut out);
        out
    }
}

/// Point on a cubic Bezier at parameter `t`.
pub fn cubic_point(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

/// Parameters in `(0, 1)` where the cubic's x or y derivative is zero.
fn cubic_extrema(p0: Point, p1: Point, p2: Point, p3: Point) -> Vec<f64> {
    let mut out = Vec::new();
    for (a0, a1, a2, a3) in [(p0.x, p1.x, p2.x, p3.x), (p0.y, p1.y, p2.y, p3.y)] {
        // Derivative / 3 = a t^2 + b t + c.
        let a = -a0 + 3.0 * a1 - 3.0 * a2 + a3;
        let b = 2.0 * (a0 - 2.0 * a1 + a2);
        let c = a1 - a0;
        if a.abs() < 1e-12 {
            if b.abs() > 1e-12 {
                out.push(-c / b);
            }
        } else {
            let disc = b * b - 4.0 * a * c;
            if disc >= 0.0 {
                let s = disc.sqrt();
                out.push((-b + s) / (2.0 * a));
                out.push((-b - s) / (2.0 * a));
            }
        }
    }
    out.retain(|t| t.is_finite() && *t > 0.0 && *t < 1.0);
    out
}

/// Even-odd point-in-polygon test; the polygon is implicitly closed.
pub fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        if let (Some(a), Some(b)) = (poly.get(i), poly.get(j))
            && (a.y > p.y) != (b.y > p.y)
        {
            let x = (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x;
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// The point on segment `a`-`b` nearest to `p`.
pub fn nearest_on_segment(p: Point, a: Point, b: Point) -> Point {
    let ab = b - a;
    let len2 = ab.dot(ab);
    if !len2.is_finite() || len2 <= 1e-18 {
        return a;
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    a + ab * t
}

/// Distance from `p` to segment `a`-`b`.
pub fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    p.distance(nearest_on_segment(p, a, b))
}

/// Distance from `p` to an open polyline (infinity when it has no points).
pub fn distance_to_polyline(p: Point, pts: &[Point]) -> f64 {
    match pts {
        [] => f64::INFINITY,
        [only] => p.distance(*only),
        _ => pts.windows(2).filter_map(|w| Some(distance_to_segment(p, *w.first()?, *w.get(1)?))).fold(f64::INFINITY, f64::min),
    }
}

/// Total length of an open polyline.
pub fn polyline_length(pts: &[Point]) -> f64 {
    pts.windows(2).filter_map(|w| Some(w.first()?.distance(*w.get(1)?))).sum()
}

/// The point at fraction `t` (0..1) of an open polyline's length; `None` when it has no points.
pub fn point_along(pts: &[Point], t: f64) -> Option<Point> {
    let total = polyline_length(pts);
    let first = *pts.first()?;
    if !total.is_finite() || total <= 0.0 {
        return Some(first);
    }
    let mut left = total * if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.5 };
    for w in pts.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let d = a.distance(*b);
        if left <= d && d > 0.0 {
            return Some(a.lerp(*b, left / d));
        }
        left -= d;
    }
    pts.last().copied()
}

/// The farthest intersection of the ray from `origin` along `dir` with any segment of `lines`.
/// Farthest rather than nearest so a ray cast from a shape's centre reaches its outer edge even
/// when the outline is concave (the cloud) or has an inner arc.
pub fn ray_hit(origin: Point, dir: Vec2, lines: &[Polyline]) -> Option<Point> {
    let mut best: Option<(f64, Point)> = None;
    for pl in lines {
        for (a, b) in pl.segments() {
            let e = b - a;
            let den = dir.cross(e);
            if den.abs() < 1e-12 {
                continue;
            }
            let w = a - origin;
            let t = w.cross(e) / den;
            let u = w.cross(dir) / den;
            if t >= 0.0 && (-1e-9..=1.0 + 1e-9).contains(&u) && best.is_none_or(|(bt, _)| t > bt) {
                best = Some((t, origin + dir * t));
            }
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn transform_rotation_and_inverse() {
        let t = Transform::rotate_about(Point::new(10.0, 10.0), 90.0);
        let p = t.apply(Point::new(20.0, 10.0));
        assert!(close(p.x, 10.0) && close(p.y, 20.0), "{p:?}");
        let back = t.inverse().unwrap().apply(p);
        assert!(close(back.x, 20.0) && close(back.y, 10.0));
        assert!(Transform::scale(0.0, 1.0).inverse().is_none());
        assert_eq!(Transform::rotate(f64::NAN), Transform::IDENTITY.then(&Transform::rotate(0.0)));
    }

    #[test]
    fn cubic_bounds_are_tight() {
        let mut p = Path::new();
        p.move_to(Point::new(0.0, 0.0)).cubic_to(Point::new(0.0, -100.0), Point::new(100.0, -100.0), Point::new(100.0, 0.0));
        let b = p.bounds().unwrap();
        assert!(close(b.y0, -75.0), "{b:?}");
        assert!(close(b.x0, 0.0) && close(b.x1, 100.0) && close(b.y1, 0.0));
    }

    #[test]
    fn polygon_and_segment_helpers() {
        let sq = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)];
        assert!(point_in_polygon(Point::new(5.0, 5.0), &sq));
        assert!(!point_in_polygon(Point::new(15.0, 5.0), &sq));
        assert!(!point_in_polygon(Point::new(1.0, 1.0), &sq[..2]));
        assert!(close(distance_to_segment(Point::new(5.0, 3.0), Point::new(0.0, 0.0), Point::new(10.0, 0.0)), 3.0));
        assert!(close(distance_to_segment(Point::new(-3.0, 4.0), Point::new(0.0, 0.0), Point::new(0.0, 0.0)), 5.0));
        let mid = point_along(&[Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0)], 0.5).unwrap();
        assert!(close(mid.x, 10.0) && close(mid.y, 0.0));
        assert_eq!(point_along(&[], 0.5), None);
    }

    #[test]
    fn flatten_closes_and_caps_steps() {
        let p = Path::polygon(&[Point::new(0.0, 0.0), Point::new(1.0, 0.0), Point::new(1.0, 1.0), Point::new(0.0, 0.0)]);
        let f = p.flatten(0.1);
        assert_eq!(f.len(), 1);
        assert!(f[0].closed);
        assert_eq!(f[0].points.len(), 3);
        let mut c = Path::new();
        c.move_to(Point::ZERO).cubic_to(Point::new(1e9, 0.0), Point::new(-1e9, 1e9), Point::new(5.0, 5.0));
        assert!(c.flatten(f64::NAN)[0].points.len() <= MAX_CUBIC_STEPS + 1);
    }
}
