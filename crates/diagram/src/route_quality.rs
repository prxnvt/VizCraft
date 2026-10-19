//! How readable a page's connector routes are: the checks behind the routing standard.
//!
//! [`check`] reports, for the connectors the page routes around shapes
//! ([`crate::routing_policy::routes`]):
//!
//! - a segment that passes within the page's clearance ([`crate::routing_policy::clearance`]) of
//!   a shape that is not one of its ends ([`Problem::NearShape`]);
//! - a segment that runs along an edge of one of its own end shapes, within the clearance of it
//!   for longer than the clearance, instead of leaving it ([`Problem::AlongEnd`]);
//! - two connectors that share a collinear run longer than the clearance, or run side by side
//!   closer than the router's run spacing ([`Problem::SharedRun`]);
//! - a label (the middle label with its frame, or an end label) that overlaps a shape
//!   ([`Problem::LabelOnShape`]) or another connector's label ([`Problem::LabelOnLabel`]).
//!
//! The obstacle rules are [`crate::autoroute`]'s: containers, lanes and shapes holding an end,
//! groups, junctions and hidden shapes are not in the way. A shape that wholly holds a label box
//! (a container the label sits in) does not count as under it. The check is pure and never
//! changes the page; the router uses [`runs_along`] to refuse routes that hug their own ends.

use crate::autoroute::{Field, LazyField};
use crate::connector_look::LabelFrame;
use crate::geom::{Point, Rect};
use crate::model::{Connector, Page};
use crate::routing_policy::{clearance, routes};

/// Slack for drawing and rounding error: a route exactly one clearance from a shape is clear.
pub const TOLERANCE: f64 = 0.5;

/// Shortest run along an end's edge, or shared by two connectors, that counts, whatever the
/// clearance: shorter ones are where a line meets a corner.
const MIN_RUN: f64 = 2.0;

/// Two parallel runs closer than this read as one line: the spacing the router spreads runs to.
const RUN_SPACING: f64 = 6.0;

/// Most connectors one check compares pairwise; bigger pages are checked up to it.
const MAX_PAIRS: usize = 4096;

/// Width of one character of label text, as a share of the font size (the exports' measure).
const CHAR_WIDTH: f64 = 0.6;

/// Height of one line of label text, as a share of the font size.
const LINE_HEIGHT: f64 = 1.2;

/// What is wrong with a route.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Problem {
    /// A segment passes within the clearance of a shape that is not one of its ends.
    NearShape,
    /// A segment runs along an edge of the connector's own end shape.
    AlongEnd,
    /// Two connectors share a collinear run.
    SharedRun,
    /// A label overlaps a shape.
    LabelOnShape,
    /// A label overlaps another connector's label.
    LabelOnLabel,
}

impl Problem {
    /// A short name for reports.
    pub fn name(self) -> &'static str {
        match self {
            Problem::NearShape => "near a shape",
            Problem::AlongEnd => "along its end",
            Problem::SharedRun => "shares a run",
            Problem::LabelOnShape => "label on a shape",
            Problem::LabelOnLabel => "label on a label",
        }
    }
}

/// One finding of [`check`].
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// What is wrong.
    pub problem: Problem,
    /// The connector.
    pub connector: String,
    /// The shape or the other connector involved.
    pub other: String,
    /// The index of the segment of the connector's drawn polyline, when one segment is at fault.
    pub segment: Option<usize>,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.segment {
            Some(i) => write!(f, "{} segment {} {} {}", self.connector, i, self.problem.name(), self.other),
            None => write!(f, "{} {} {}", self.connector, self.problem.name(), self.other),
        }
    }
}

/// A label's outline: a box, or a diamond centred on a point with half-diagonals `a` and `b`.
#[derive(Clone, Copy, Debug)]
enum Outline {
    Box(Rect),
    Diamond { c: Point, a: f64, b: f64 },
}

impl Outline {
    fn bounds(&self) -> Rect {
        match *self {
            Outline::Box(r) => r,
            Outline::Diamond { c, a, b } => Rect::new(c.x - a, c.y - b, c.x + a, c.y + b),
        }
    }

    /// Whether the outline overlaps the interior of `r` (shrunk by the tolerance).
    fn overlaps(&self, r: &Rect) -> bool {
        let r = r.inflate(-TOLERANCE);
        match *self {
            Outline::Box(b) => b.x0 < r.x1 && r.x0 < b.x1 && b.y0 < r.y1 && r.y0 < b.y1,
            Outline::Diamond { c, a, b } => {
                // The diamond is |dx| / a + |dy| / b <= 1; the rectangle's point nearest the centre
                // in that measure is the centre clamped into it.
                if a <= 0.0 || b <= 0.0 {
                    return false;
                }
                let dx = (c.x.clamp(r.x0, r.x1) - c.x).abs();
                let dy = (c.y.clamp(r.y0, r.y1) - c.y).abs();
                dx / a + dy / b < 1.0
            }
        }
    }
}

/// Font size of a connector's labels, made safe.
fn font(c: &Connector) -> f64 {
    let fs = c.style.font_size;
    if fs.is_finite() { fs.clamp(1.0, 400.0) } else { 12.0 }
}

/// Width and height of a white label box holding `lines` at font size `fs`.
fn box_size(lines: &[&str], fs: f64) -> (f64, f64) {
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    (longest * fs * CHAR_WIDTH + fs * 0.6, lines.len().max(1) as f64 * fs * LINE_HEIGHT + fs * 0.3)
}

/// The middle label of `c` centred on `p`, in its frame, if it has one.
fn middle(c: &Connector, p: Point) -> Option<Outline> {
    if c.text.trim().is_empty() {
        return None;
    }
    let lines: Vec<&str> = c.text.lines().collect();
    let (w, h) = box_size(&lines, font(c));
    Some(match c.label_frame {
        LabelFrame::Box => Outline::Box(Rect::new(p.x - w / 2.0, p.y - h / 2.0, p.x + w / 2.0, p.y + h / 2.0)),
        LabelFrame::Diamond | LabelFrame::DoubleDiamond => Outline::Diamond { c: p, a: w.max(h * 0.5) + 6.0, b: h + 6.0 },
    })
}

/// The width and height of the middle label `c` draws, its frame included, if it has one.
pub fn label_size(c: &Connector) -> Option<(f64, f64)> {
    middle(c, Point::ZERO).map(|o| {
        let b = o.bounds();
        (b.width(), b.height())
    })
}

/// The labels `c` draws, as the exports draw them: the middle label in its frame, then the start
/// and end labels.
fn labels(c: &Connector, g: &crate::route::ConnectorGeometry) -> Vec<Outline> {
    let fs = font(c);
    let mut out: Vec<Outline> = middle(c, g.label_pos).into_iter().collect();
    for (text, at_start) in [(&c.start_label, true), (&c.end_label, false)] {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let Some(p) = crate::arrows::end_label_anchor(&g.polyline, at_start, fs, c.style.line_width) else { continue };
        let (w, h) = box_size(&[text.lines().next().unwrap_or_default()], fs);
        out.push(Outline::Box(Rect::new(p.x - w / 2.0, p.y - h / 2.0, p.x + w / 2.0, p.y + h / 2.0)));
    }
    out.retain(|o| {
        let b = o.bounds();
        [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite())
    });
    out
}

/// The interval of `t` in `[0, 1]` where `f(t) = f0 + t * df` lies strictly within `(lo, hi)`.
fn window(f0: f64, df: f64, lo: f64, hi: f64) -> Option<(f64, f64)> {
    if df.abs() < 1e-12 {
        return (f0 > lo && f0 < hi).then_some((0.0, 1.0));
    }
    let (a, b) = ((lo - f0) / df, (hi - f0) / df);
    let (t0, t1) = (a.min(b).max(0.0), a.max(b).min(1.0));
    (t1 > t0).then_some((t0, t1))
}

/// The points of `pts` that start a new direction, with their indices: points in the middle of
/// a straight run (a flattened curve's nearly straight stretch, say) are dropped, so a run is
/// judged whole rather than chord by chord.
fn runs(pts: &[Point]) -> Vec<(usize, Point)> {
    let mut out: Vec<(usize, Point)> = Vec::with_capacity(pts.len());
    for (i, p) in pts.iter().copied().enumerate().filter(|(_, p)| p.is_finite()) {
        if out.last().is_some_and(|(_, l)| l.distance(p) < 1e-9) {
            continue;
        }
        let n = out.len();
        if let (Some((_, a)), Some((_, b))) = (n.checked_sub(2).and_then(|k| out.get(k)).copied(), out.last().copied()) {
            let (u, v) = (b - a, p - b);
            let lu = u.length().max(1e-12);
            if (u.cross(p - a) / lu).abs() <= TOLERANCE / 4.0 && u.dot(v) > 0.0 {
                out.pop();
            }
        }
        out.push((i, p));
    }
    out
}

/// The first segment of `pts` that runs along an edge of `r` (a connector's own end shape): at
/// less than 30 degrees to the edge, closer to its line than `clear` (less the tolerance), beside
/// the edge itself, for longer than `clear` (at least [`MIN_RUN`]). A stub that leaves the edge
/// square, or a run held one clearance off, passes.
pub fn runs_along(pts: &[Point], r: &Rect, clear: f64) -> Option<usize> {
    let band = (clear - TOLERANCE).max(TOLERANCE);
    let min_run = clear.max(MIN_RUN);
    let edges = [
        (Point::new(r.x0, r.y0), Point::new(r.x1, r.y0)),
        (Point::new(r.x0, r.y1), Point::new(r.x1, r.y1)),
        (Point::new(r.x0, r.y0), Point::new(r.x0, r.y1)),
        (Point::new(r.x1, r.y0), Point::new(r.x1, r.y1)),
    ];
    let merged = runs(pts);
    let found = merged.windows(2).position(|w| {
        let [(_, a), (_, b)] = w else { return false };
        let d = *b - *a;
        let len = d.length();
        if !(len.is_finite() && len > MIN_RUN) {
            return false;
        }
        edges.iter().any(|(e0, e1)| {
            let Some(u) = (*e1 - *e0).normalized() else { return false };
            let n = Point::new(-u.y, u.x);
            if (d.x * u.y - d.y * u.x).abs() / len > 0.5 {
                return false;
            }
            let span = (*e1 - *e0).length();
            let Some((s0, s1)) = window(n.dot(*a - *e0), n.dot(d), -band, band) else { return false };
            let Some((p0, p1)) = window(u.dot(*a - *e0), u.dot(d), 0.0, span) else { return false };
            (s1.min(p1) - s0.max(p0)) * len > min_run
        })
    });
    found.and_then(|k| merged.get(k)).map(|(i, _)| *i)
}

/// Whether segments `p q` and `u v` lie on one line, or on parallel lines closer than the
/// router's run spacing (less the tolerance), and overlap along it by more than `min`.
fn shared(p: Point, q: Point, u: Point, v: Point, min: f64) -> bool {
    let d = q - p;
    let len = d.length();
    let Some(dir) = d.normalized() else { return false };
    if !(len.is_finite() && len > min) {
        return false;
    }
    let off = |x: Point| (x - p).cross(dir).abs();
    let near = RUN_SPACING - TOLERANCE;
    if off(u) >= near || off(v) >= near || (off(u) - off(v)).abs() > TOLERANCE {
        return false;
    }
    let (a, b) = (dir.dot(u - p), dir.dot(v - p));
    a.max(b).min(len) - a.min(b).max(0.0) > min
}

/// Whether polylines `a` and `b` share a run longer than `min` (see [`Problem::SharedRun`]).
pub fn share_a_run(a: &[Point], b: &[Point], min: f64) -> bool {
    let (a, b): (Vec<Point>, Vec<Point>) = (runs(a).into_iter().map(|(_, p)| p).collect(), runs(b).into_iter().map(|(_, p)| p).collect());
    a.windows(2).any(|s| b.windows(2).any(|t| shared(s[0], s[1], t[0], t[1], min.max(MIN_RUN))))
}

/// Every finding on `page`, connector by connector (see the module notes). Empty means every
/// routed connector reads cleanly.
pub fn check(page: &Page) -> Vec<Finding> {
    let clear = clearance(page);
    let lazy = LazyField::new(page);
    let field: &Field = lazy.get();
    let routed: Vec<&Connector> = page.connectors.iter().filter(|c| routes(page, c)).collect();
    let mut out = Vec::new();
    let lines: Vec<Vec<Point>> = routed.iter().map(|c| crate::route::polyline_in(page, c, &lazy)).collect();
    let straight: Vec<Vec<Point>> = lines.iter().map(|l| runs(l).into_iter().map(|(_, p)| p).collect()).collect();
    let mut all_labels: Vec<(usize, Outline)> = Vec::new();
    for (k, (c, pts)) in routed.iter().zip(&lines).enumerate() {
        let ctx = field.context(c);
        let mut seen: Vec<(usize, &str)> = Vec::new();
        for h in field.hits(&ctx, pts, (clear - TOLERANCE).max(-TOLERANCE)) {
            let Some(shape) = field.shape_id(h.obstacle) else { continue };
            if !seen.contains(&(h.segment, shape)) {
                seen.push((h.segment, shape));
                out.push(Finding { problem: Problem::NearShape, connector: c.id.clone(), other: shape.to_string(), segment: Some(h.segment) });
            }
        }
        for own in [ctx.start_own, ctx.end_own].into_iter().flatten() {
            let (Some(o), Some(shape)) = (field.obstacles.get(own), field.shape_id(own)) else { continue };
            let r = Rect::new(o.rect.min.x, o.rect.min.y, o.rect.max.x, o.rect.max.y);
            if let Some(i) = runs_along(pts, &r, clear)
                && !out.iter().any(|f| f.problem == Problem::AlongEnd && f.connector == c.id && f.other == shape)
            {
                out.push(Finding { problem: Problem::AlongEnd, connector: c.id.clone(), other: shape.to_string(), segment: Some(i) });
            }
        }
        let g = crate::route::geometry_in(page, c, &lazy);
        for label in labels(c, &g) {
            for (id, b) in field.bodies() {
                if b.contains_rect(&label.bounds()) || !label.overlaps(&b) {
                    continue;
                }
                if !out.iter().any(|f| f.problem == Problem::LabelOnShape && f.connector == c.id && f.other == id) {
                    out.push(Finding { problem: Problem::LabelOnShape, connector: c.id.clone(), other: id.to_string(), segment: None });
                }
            }
            all_labels.push((k, label));
        }
    }
    let n = routed.len().min(MAX_PAIRS);
    for i in 0..n {
        for j in i + 1..n {
            let (Some(a), Some(b)) = (straight.get(i), straight.get(j)) else { continue };
            let hit = a.windows(2).any(|s| b.windows(2).any(|t| shared(s[0], s[1], t[0], t[1], clear.max(MIN_RUN))));
            if hit && let (Some(ci), Some(cj)) = (routed.get(i), routed.get(j)) {
                out.push(Finding { problem: Problem::SharedRun, connector: ci.id.clone(), other: cj.id.clone(), segment: None });
            }
        }
    }
    for (x, (i, a)) in all_labels.iter().enumerate().take(MAX_PAIRS) {
        for (j, b) in all_labels.iter().skip(x + 1).take(MAX_PAIRS) {
            if i == j || !a.overlaps(&b.bounds()) || !b.overlaps(&a.bounds()) {
                continue;
            }
            if let (Some(ci), Some(cj)) = (routed.get(*i), routed.get(*j)) {
                out.push(Finding { problem: Problem::LabelOnLabel, connector: ci.id.clone(), other: cj.id.clone(), segment: None });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::ShapeKind;
    use crate::model::{End, Route, Shape};

    fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
        Shape::new(id, ShapeKind::Rectangle, x, y, w, h)
    }

    fn page(shapes: Vec<Shape>, connectors: Vec<Connector>) -> Page {
        let mut p = Page::new("p", "p");
        p.shapes = shapes;
        p.connectors = connectors;
        p
    }

    fn kinds(p: &Page) -> Vec<Problem> {
        check(p).into_iter().map(|f| f.problem).collect()
    }

    #[test]
    fn a_line_along_its_own_box_edge_is_found_and_a_square_stub_is_not() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        let along = [Point::new(100.0, 25.0), Point::new(100.0, 200.0)];
        assert_eq!(runs_along(&along, &r, 10.0), Some(0));
        let square = [Point::new(100.0, 25.0), Point::new(110.0, 25.0), Point::new(110.0, 200.0)];
        assert_eq!(runs_along(&square, &r, 10.0), None, "one clearance off is clear");
        let hugging = [Point::new(100.0, 25.0), Point::new(104.0, 25.0), Point::new(104.0, 200.0)];
        assert_eq!(runs_along(&hugging, &r, 10.0), Some(1));
        let diagonal = [Point::new(100.0, 25.0), Point::new(200.0, 125.0)];
        assert_eq!(runs_along(&diagonal, &r, 10.0), None);
        assert_eq!(runs_along(&[Point::new(f64::NAN, 0.0), Point::new(1.0, 1.0)], &r, 10.0), None);
    }

    #[test]
    fn each_problem_is_reported() {
        // Two stacked boxes joined by a straight line down their right edges, a third box beside
        // the line, a second connector on the same run and two labels on top of each other.
        let mut along = Connector::new(
            "along",
            End::Glued { shape: "a".into(), point: Some(1), port: None },
            End::Glued { shape: "b".into(), point: Some(1), port: None },
        );
        along.route = Route::Straight;
        along.text = "places".into();
        let mut twin = along.clone();
        twin.id = "twin".into();
        let p =
            page(vec![rect("a", 0.0, 0.0, 100.0, 50.0), rect("b", 0.0, 300.0, 100.0, 50.0), rect("x", 104.0, 160.0, 40.0, 40.0)], vec![along, twin]);
        let k = kinds(&p);
        for want in [Problem::AlongEnd, Problem::NearShape, Problem::SharedRun, Problem::LabelOnLabel, Problem::LabelOnShape] {
            assert!(k.contains(&want), "{want:?} in {:?}", check(&p));
        }
        assert!(check(&p).iter().all(|f| !f.to_string().is_empty()));
    }

    #[test]
    fn a_clean_page_has_no_findings_and_junk_does_not_panic() {
        let mut c = Connector::new("c", End::dynamic("a"), End::dynamic("b"));
        c.route = Route::Orthogonal;
        let p = page(vec![rect("a", 0.0, 0.0, 100.0, 50.0), rect("b", 0.0, 200.0, 100.0, 50.0)], vec![c]);
        assert!(check(&p).is_empty(), "{:?}", check(&p));
        let mut junk = p.clone();
        junk.shapes.push(Shape { id: "nan".into(), x: f64::NAN, ..Shape::default() });
        junk.connectors.push(Connector::new("j", End::dynamic("nan"), End::free(Point::new(f64::INFINITY, 0.0))));
        junk.routing.clearance = Some(f64::NAN);
        let _ = check(&junk);
    }
}
