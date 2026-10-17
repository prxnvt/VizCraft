//! Small geometry helpers shared by the algorithms: bounding boxes, component packing, edge
//! clipping and the generic straight or orthogonal routes used by the radial and force layouts.

use crate::{Point, Rect, Sub};
use std::collections::BTreeMap;

/// Accumulates a bounding box over rectangles and points.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    any: bool,
}

impl Bounds {
    pub fn add_point(&mut self, p: Point) {
        if !p.x.is_finite() || !p.y.is_finite() {
            return;
        }
        if self.any {
            self.min_x = self.min_x.min(p.x);
            self.min_y = self.min_y.min(p.y);
            self.max_x = self.max_x.max(p.x);
            self.max_y = self.max_y.max(p.y);
        } else {
            *self = Bounds { min_x: p.x, min_y: p.y, max_x: p.x, max_y: p.y, any: true };
        }
    }

    pub fn add_rect(&mut self, r: &Rect) {
        self.add_point(Point::new(r.x, r.y));
        self.add_point(Point::new(r.right(), r.bottom()));
    }

    /// The box, or `None` when nothing was added.
    pub fn get(&self) -> Option<Rect> {
        self.any.then(|| Rect::new(self.min_x, self.min_y, self.max_x - self.min_x, self.max_y - self.min_y))
    }

    /// The box, or an empty rectangle at the origin.
    pub fn rect(&self) -> Rect {
        self.get().unwrap_or_default()
    }
}

/// Shelf packing: places boxes left to right in rows of a roughly square overall shape, keeping
/// the input order so packed components read in the same order as the caller's nodes.
///
/// Returns, per box, the offset that moves it to its packed position.
pub(crate) fn pack(boxes: &[Rect], spacing: f64) -> Vec<Point> {
    if boxes.len() <= 1 {
        return boxes.iter().map(|b| Point::new(-b.x, -b.y)).collect();
    }
    let area: f64 = boxes.iter().map(|b| (b.width + spacing) * (b.height + spacing)).sum();
    let widest = boxes.iter().map(|b| b.width).fold(0.0, f64::max);
    let target = widest.max(area.sqrt() * 1.2);
    let (mut x, mut y, mut row_h) = (0.0f64, 0.0f64, 0.0f64);
    let mut out = Vec::with_capacity(boxes.len());
    for b in boxes {
        if x > 0.0 && x + b.width > target {
            x = 0.0;
            y += row_h + spacing;
            row_h = 0.0;
        }
        out.push(Point::new(x - b.x, y - b.y));
        x += b.width + spacing;
        row_h = row_h.max(b.height);
    }
    out
}

/// The `k`-th of `count` nested self-loops on `rect`, leaving and re-entering the same side.
pub(crate) fn self_loop(rect: &Rect, k: usize, count: usize, base: f64, bottom_side: bool) -> Vec<Point> {
    let reach = base * (k + 1) as f64;
    let frac = (k + 1) as f64 / (count + 1) as f64;
    let c = rect.center();
    if bottom_side {
        let a = rect.width / 2.0 * frac;
        let b = rect.bottom();
        vec![Point::new(c.x - a, b), Point::new(c.x - a, b + reach), Point::new(c.x + a, b + reach), Point::new(c.x + a, b)]
    } else {
        let a = rect.height / 2.0 * frac;
        let r = rect.right();
        vec![Point::new(r, c.y - a), Point::new(r + reach, c.y - a), Point::new(r + reach, c.y + a), Point::new(r, c.y + a)]
    }
}

/// Where the ray from the centre of `r` toward `toward` leaves the rectangle.
pub(crate) fn clip(r: &Rect, toward: Point) -> Point {
    let c = r.center();
    let (dx, dy) = (toward.x - c.x, toward.y - c.y);
    let tx = if dx.abs() > 1e-12 { (r.width / 2.0) / dx.abs() } else { f64::INFINITY };
    let ty = if dy.abs() > 1e-12 { (r.height / 2.0) / dy.abs() } else { f64::INFINITY };
    let t = tx.min(ty).min(1.0);
    if !t.is_finite() {
        return c;
    }
    Point::new(c.x + dx * t, c.y + dy * t)
}

/// Removes repeated points and middle points of straight runs.
pub(crate) fn simplify(points: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_some_and(|q| (q.x - p.x).abs() < 1e-9 && (q.y - p.y).abs() < 1e-9) {
            continue;
        }
        if out.len() >= 2
            && let (Some(a), Some(b)) = (out.get(out.len() - 2).copied(), out.last().copied())
        {
            let collinear_x = (a.x - b.x).abs() < 1e-9 && (b.x - p.x).abs() < 1e-9;
            let collinear_y = (a.y - b.y).abs() < 1e-9 && (b.y - p.y).abs() < 1e-9;
            if collinear_x || collinear_y {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

/// Routes every edge of `sub` between the placed node rectangles (top-left positions `pos`) as a
/// straight segment or an orthogonal Z, spreading parallel edges by `spacing`.
pub(crate) fn route_free(sub: &Sub, pos: &[Point], spacing: f64, orthogonal: bool) -> Vec<Vec<Point>> {
    let rect = |v: usize| {
        let p = pos.get(v).copied().unwrap_or_default();
        let (w, h) = sub.sizes.get(v).copied().unwrap_or((0.0, 0.0));
        Rect::new(p.x, p.y, w, h)
    };
    // Parallel edges (either direction) share a key; each gets its own offset.
    let mut bundles: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (i, e) in sub.edges.iter().enumerate() {
        bundles.entry((e.s.min(e.t), e.s.max(e.t))).or_default().push(i);
    }
    let mut offset = vec![0.0f64; sub.edges.len()];
    for list in bundles.values() {
        let k = list.len() as f64;
        for (j, &i) in list.iter().enumerate() {
            if let Some(slot) = offset.get_mut(i) {
                *slot = (j as f64 - (k - 1.0) / 2.0) * spacing;
            }
        }
    }
    sub.edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let o = offset.get(i).copied().unwrap_or(0.0);
            let (a, b) = (rect(e.s), rect(e.t));
            if orthogonal { orthogonal_route(&a, &b, o) } else { straight_route(&a, &b, o) }
        })
        .collect()
}

fn straight_route(a: &Rect, b: &Rect, offset: f64) -> Vec<Point> {
    let (ca, cb) = (a.center(), b.center());
    if offset.abs() < 1e-12 {
        return vec![clip(a, cb), clip(b, ca)];
    }
    // Measure the offset along the perpendicular of a canonical direction (by position) so
    // parallel edges running in opposite directions do not land on the same side.
    let (lo, hi) = if (ca.x, ca.y) <= (cb.x, cb.y) { (ca, cb) } else { (cb, ca) };
    let (dx, dy) = (hi.x - lo.x, hi.y - lo.y);
    let len = (dx * dx + dy * dy).sqrt();
    let (nx, ny) = if len > 1e-12 { (-dy / len, dx / len) } else { (0.0, 1.0) };
    let mid = Point::new((ca.x + cb.x) / 2.0 + nx * offset, (ca.y + cb.y) / 2.0 + ny * offset);
    vec![clip(a, mid), mid, clip(b, mid)]
}

fn orthogonal_route(a: &Rect, b: &Rect, offset: f64) -> Vec<Point> {
    let (ca, cb) = (a.center(), b.center());
    let (dx, dy) = (cb.x - ca.x, cb.y - ca.y);
    let points = if dx.abs() >= dy.abs() {
        // Leave through the facing vertical sides and turn once in the middle.
        let lim = (a.height.min(b.height) / 2.0 * 0.8).max(0.0);
        let o = offset.clamp(-lim, lim);
        let (sx, ex) = if dx >= 0.0 { (a.right(), b.x) } else { (a.x, b.right()) };
        let xm = (sx + ex) / 2.0 + offset;
        vec![Point::new(sx, ca.y + o), Point::new(xm, ca.y + o), Point::new(xm, cb.y + o), Point::new(ex, cb.y + o)]
    } else {
        let lim = (a.width.min(b.width) / 2.0 * 0.8).max(0.0);
        let o = offset.clamp(-lim, lim);
        let (sy, ey) = if dy >= 0.0 { (a.bottom(), b.y) } else { (a.y, b.bottom()) };
        let ym = (sy + ey) / 2.0 + offset;
        vec![Point::new(ca.x + o, sy), Point::new(ca.x + o, ym), Point::new(cb.x + o, ym), Point::new(cb.x + o, ey)]
    };
    let simple = simplify(points);
    if simple.len() >= 2 { simple } else { vec![clip(a, cb), clip(b, ca)] }
}
