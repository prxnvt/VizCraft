//! Snapping: where a moving rectangle or point lands near the grid, guides, other shapes' edges
//! and centres, and connection points, and the alignment lines to show for it. Pure geometry in
//! page (world) units; the UI turns a screen tolerance into page units before asking.
//!
//! Each axis snaps on its own. Targets are tried in priority order, guides first, then shape
//! geometry and connection points, then the grid, so a shape edge within reach wins over a grid
//! line that happens to be nearer; within one class the nearest wins. Also here: the grid
//! spacing for a zoom level and the size of one ruler unit, which the grid, the rulers and grid
//! snapping share so that what snaps is what is drawn.

use crate::geom::{Point, Rect};
use crate::page_fields::{Guide, GuideOrientation, Units};

/// What a moving item can snap to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    /// Grid spacing in page units; `None` (or a step that is not positive) turns grid snapping off.
    pub grid: Option<f64>,
    /// Guide lines.
    pub guides: Vec<Guide>,
    /// Bounds of other shapes: their left, centre and right (top, middle, bottom) attract.
    pub rects: Vec<Rect>,
    /// Connection points of other shapes.
    pub points: Vec<Point>,
}

/// Which kind of target a snap landed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A guide line.
    Guide,
    /// An edge or centre of another shape.
    Shape,
    /// A connection point.
    Point,
    /// A grid line.
    Grid,
}

impl Kind {
    /// Lower is tried first.
    fn rank(self) -> u8 {
        match self {
            Kind::Guide => 0,
            Kind::Shape | Kind::Point => 1,
            Kind::Grid => 2,
        }
    }
}

/// An alignment line to draw while snapped: vertical at `x = at` (or horizontal at `y = at`),
/// running from `from` to `to` along the other axis so it joins the moving item and the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignLine {
    /// A vertical line (constant x); otherwise horizontal (constant y).
    pub vertical: bool,
    /// The x of a vertical line or the y of a horizontal one.
    pub at: f64,
    /// Start along the other axis.
    pub from: f64,
    /// End along the other axis.
    pub to: f64,
    /// What it aligns with.
    pub kind: Kind,
}

/// The result of a snap: add `offset` to the moving item; draw `lines`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapped {
    /// How far to shift the item, in page units (zero on an axis with nothing within reach).
    pub offset: Point,
    /// Alignment lines for the snapped position; grid snaps draw none.
    pub lines: Vec<AlignLine>,
}

/// One value on one axis a target offers, with the extent along the other axis an alignment
/// line should reach.
#[derive(Clone, Copy)]
struct Line {
    at: f64,
    span: (f64, f64),
    kind: Kind,
}

/// Most targets of each kind looked at, so a huge page cannot stall a drag frame.
const MAX_TARGETS: usize = 20_000;

/// Values targets offer on one axis (`vertical` means x values); grid lines are worked out per
/// candidate instead, since they are everywhere.
fn axis_lines(t: &Targets, vertical: bool) -> Vec<Line> {
    let mut out = Vec::new();
    let want = if vertical { GuideOrientation::Vertical } else { GuideOrientation::Horizontal };
    for g in t.guides.iter().take(MAX_TARGETS).filter(|g| g.orientation == want && g.position.is_finite()) {
        out.push(Line { at: g.position, span: (f64::NAN, f64::NAN), kind: Kind::Guide });
    }
    for r in t.rects.iter().take(MAX_TARGETS) {
        let (a0, a1, b0, b1) = if vertical { (r.x0, r.x1, r.y0, r.y1) } else { (r.y0, r.y1, r.x0, r.x1) };
        for at in [a0, (a0 + a1) / 2.0, a1] {
            if at.is_finite() {
                out.push(Line { at, span: (b0, b1), kind: Kind::Shape });
            }
        }
    }
    for p in t.points.iter().take(MAX_TARGETS) {
        let (at, b) = if vertical { (p.x, p.y) } else { (p.y, p.x) };
        if at.is_finite() && b.is_finite() {
            out.push(Line { at, span: (b, b), kind: Kind::Point });
        }
    }
    out
}

/// Best shift on one axis for the moving values `cands`, and the line it lands on.
fn best_shift(cands: &[f64], lines: &[Line], grid: Option<f64>, tol: f64) -> Option<(f64, Line)> {
    let mut best: Option<(f64, Line)> = None;
    let mut consider = |shift: f64, line: Line| {
        if !shift.is_finite() || shift.abs() > tol {
            return;
        }
        let better = match &best {
            None => true,
            Some((s, l)) => (line.kind.rank(), shift.abs()) < (l.kind.rank(), s.abs()),
        };
        if better {
            best = Some((shift, line));
        }
    };
    for &c in cands {
        for l in lines {
            consider(l.at - c, *l);
        }
        if let Some(g) = grid.filter(|g| g.is_finite() && *g > 0.0) {
            let at = (c / g).round() * g;
            consider(at - c, Line { at, span: (f64::NAN, f64::NAN), kind: Kind::Grid });
        }
    }
    best
}

/// Lines for every target the shifted candidates now touch on one axis, merged per position.
fn touching(cands: &[f64], lines: &[Line], own: (f64, f64), vertical: bool) -> Vec<AlignLine> {
    const EPS: f64 = 1e-6;
    let mut out: Vec<AlignLine> = Vec::new();
    for l in lines {
        if !cands.iter().any(|c| (c - l.at).abs() < EPS) {
            continue;
        }
        let (mut from, mut to) = own;
        if l.span.0.is_finite() && l.span.1.is_finite() {
            from = from.min(l.span.0);
            to = to.max(l.span.1);
        }
        match out.iter_mut().find(|a| (a.at - l.at).abs() < EPS) {
            Some(a) => {
                a.from = a.from.min(from);
                a.to = a.to.max(to);
                if l.kind.rank() < a.kind.rank() {
                    a.kind = l.kind;
                }
            }
            None => out.push(AlignLine { vertical, at: l.at, from, to, kind: l.kind }),
        }
    }
    out
}

fn snap_axes(xs: &[f64], ys: &[f64], own_x: (f64, f64), own_y: (f64, f64), t: &Targets, tol: f64) -> Snapped {
    let tol = if tol.is_finite() { tol.max(0.0) } else { 0.0 };
    let vlines = axis_lines(t, true);
    let hlines = axis_lines(t, false);
    let dx = best_shift(xs, &vlines, t.grid, tol).map_or(0.0, |(s, _)| s);
    let dy = best_shift(ys, &hlines, t.grid, tol).map_or(0.0, |(s, _)| s);
    let xs: Vec<f64> = xs.iter().map(|x| x + dx).collect();
    let ys: Vec<f64> = ys.iter().map(|y| y + dy).collect();
    // A vertical line spans the item's (shifted) y extent and the target's.
    let mut lines = touching(&xs, &vlines, (own_y.0 + dy, own_y.1 + dy), true);
    lines.extend(touching(&ys, &hlines, (own_x.0 + dx, own_x.1 + dx), false));
    Snapped { offset: Point::new(dx, dy), lines }
}

/// Snap a moving rectangle: its left, centre and right snap on x, its top, middle and bottom on
/// y, to targets within `tol` page units.
pub fn snap_rect(moving: Rect, t: &Targets, tol: f64) -> Snapped {
    let xs = [moving.x0, (moving.x0 + moving.x1) / 2.0, moving.x1];
    let ys = [moving.y0, (moving.y0 + moving.y1) / 2.0, moving.y1];
    if !xs.iter().chain(&ys).all(|v| v.is_finite()) {
        return Snapped::default();
    }
    snap_axes(&xs, &ys, (moving.x0, moving.x1), (moving.y0, moving.y1), t, tol)
}

/// Snap a moving point to targets within `tol` page units.
pub fn snap_point(p: Point, t: &Targets, tol: f64) -> Snapped {
    if !p.is_finite() {
        return Snapped::default();
    }
    snap_axes(&[p.x], &[p.y], (p.x, p.x), (p.y, p.y), t, tol)
}

/// Page units in one ruler unit (100 page units are an inch).
pub fn unit_size(units: Units) -> f64 {
    match units {
        Units::Inches => 100.0,
        Units::Feet => 1200.0,
        Units::Millimeters => 100.0 / 25.4,
        Units::Centimeters => 1000.0 / 25.4,
        Units::Meters => 100_000.0 / 25.4,
        Units::Points => 100.0 / 72.0,
        Units::Pixels => 1.0,
    }
}

/// Inches and feet split in halves; the rest in steps of 1, 2 and 5.
fn binary(units: Units) -> bool {
    matches!(units, Units::Inches | Units::Feet)
}

/// The smallest tidy step in page units that is at least `min_px` screen points apart at `zoom`:
/// a power of two of the unit for inches and feet, 1, 2 or 5 times a power of ten otherwise.
/// The grid draws (and snaps to) this step, so it gets finer as the view zooms in.
pub fn grid_step(units: Units, zoom: f64, min_px: f64) -> f64 {
    let u = unit_size(units);
    let zoom = if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 };
    let min_px = if min_px.is_finite() && min_px > 0.0 { min_px } else { 1.0 };
    let want = min_px / zoom;
    let steps: Vec<f64> = if binary(units) {
        (-10..=12).map(|k| u * 2f64.powi(k)).collect()
    } else {
        (-6..=8).flat_map(|k| [1.0, 2.0, 5.0].map(|m| u * m * 10f64.powi(k))).collect()
    };
    steps.iter().copied().find(|s| *s >= want).unwrap_or_else(|| steps.last().copied().unwrap_or(u))
}

/// How many grid steps make one major (darker, labelled) line: a whole unit where the step is
/// smaller than one, else every fourth (binary) or fifth step.
pub fn major_every(units: Units, step: f64) -> u32 {
    let u = unit_size(units);
    let per_unit = u / step;
    if per_unit >= 1.5 && per_unit.is_finite() {
        (per_unit.round() as u32).clamp(1, 1024)
    } else if binary(units) {
        4
    } else {
        5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guide(vertical: bool, at: f64) -> Guide {
        Guide { orientation: if vertical { GuideOrientation::Vertical } else { GuideOrientation::Horizontal }, position: at }
    }

    #[test]
    fn rect_snaps_to_the_grid() {
        let t = Targets { grid: Some(10.0), ..Targets::default() };
        let s = snap_rect(Rect::from_xywh(103.0, 48.0, 40.0, 20.0), &t, 5.0);
        assert_eq!(s.offset, Point::new(-3.0, 2.0));
        assert!(s.lines.is_empty(), "grid snaps draw no lines: {:?}", s.lines);
    }

    #[test]
    fn point_snaps_to_the_grid() {
        let t = Targets { grid: Some(25.0), ..Targets::default() };
        let s = snap_point(Point::new(49.0, 76.0), &t, 3.0);
        assert_eq!(s.offset, Point::new(1.0, -1.0));
    }

    #[test]
    fn guide_wins_over_a_nearer_grid_line() {
        let t = Targets { grid: Some(10.0), guides: vec![guide(true, 204.0), guide(false, 97.0)], ..Targets::default() };
        // Left edge at 200 sits on a grid line, but the guide at 204 is within reach and ranks first.
        let s = snap_rect(Rect::from_xywh(200.0, 100.0, 50.0, 30.0), &t, 6.0);
        assert_eq!(s.offset, Point::new(4.0, -3.0));
        let v = s.lines.iter().find(|l| l.vertical).unwrap();
        assert_eq!((v.at, v.kind), (204.0, Kind::Guide));
        // The line runs along the moved rectangle.
        assert_eq!((v.from, v.to), (97.0, 127.0));
    }

    #[test]
    fn edges_and_centres_of_other_shapes_attract() {
        let other = Rect::from_xywh(300.0, 0.0, 100.0, 60.0);
        let t = Targets { rects: vec![other], ..Targets::default() };
        // Right edge 297 snaps to the other's left edge 300; centre y 32 snaps to its middle 30.
        let s = snap_rect(Rect::from_xywh(257.0, 22.0, 40.0, 20.0), &t, 5.0);
        assert_eq!(s.offset, Point::new(3.0, -2.0));
        let v = s.lines.iter().find(|l| l.vertical).unwrap();
        assert_eq!((v.at, v.kind), (300.0, Kind::Shape));
        // A vertical line joins the moving shape and the target: its y extent covers both.
        assert_eq!((v.from, v.to), (0.0, 60.0));
        let h = s.lines.iter().find(|l| !l.vertical).unwrap();
        assert_eq!(h.at, 30.0);
        assert_eq!((h.from, h.to), (260.0, 400.0));
    }

    #[test]
    fn centre_to_centre_alignment() {
        let t = Targets { rects: vec![Rect::from_xywh(0.0, 0.0, 100.0, 100.0)], ..Targets::default() };
        let s = snap_rect(Rect::from_xywh(32.0, 300.0, 40.0, 40.0), &t, 3.0);
        assert_eq!(s.offset.x, -2.0);
        assert!(s.lines.iter().any(|l| l.vertical && l.at == 50.0));
    }

    #[test]
    fn connection_points_attract() {
        let t = Targets { points: vec![Point::new(150.0, 75.0)], ..Targets::default() };
        let s = snap_point(Point::new(148.0, 77.5), &t, 4.0);
        assert_eq!(s.offset, Point::new(2.0, -2.5));
        assert!(s.lines.iter().all(|l| l.kind == Kind::Point));
    }

    #[test]
    fn tolerance_limits_reach() {
        let t = Targets { guides: vec![guide(true, 110.0)], ..Targets::default() };
        assert_eq!(snap_point(Point::new(104.0, 0.0), &t, 5.0).offset, Point::ZERO);
        assert_eq!(snap_point(Point::new(105.0, 0.0), &t, 5.0).offset, Point::new(5.0, 0.0));
        // A broken tolerance snaps nothing rather than everything.
        assert_eq!(snap_point(Point::new(109.0, 0.0), &t, f64::NAN).offset, Point::ZERO);
        assert_eq!(snap_point(Point::new(109.0, 0.0), &t, -3.0).offset, Point::ZERO);
    }

    #[test]
    fn nothing_within_reach_leaves_the_item_alone() {
        let t = Targets {
            grid: None,
            guides: vec![guide(false, 900.0)],
            rects: vec![Rect::from_xywh(500.0, 500.0, 10.0, 10.0)],
            points: vec![Point::new(-400.0, -400.0)],
        };
        let s = snap_rect(Rect::from_xywh(0.0, 0.0, 50.0, 50.0), &t, 8.0);
        assert_eq!(s, Snapped::default());
        assert_eq!(snap_rect(Rect::from_xywh(0.0, 0.0, 5.0, 5.0), &Targets::default(), 8.0), Snapped::default());
    }

    #[test]
    fn hostile_input_is_ignored() {
        let t = Targets {
            grid: Some(f64::NAN),
            guides: vec![guide(true, f64::INFINITY)],
            rects: vec![Rect::new(f64::NAN, f64::NAN, f64::NAN, f64::NAN)],
            points: vec![],
        };
        assert_eq!(snap_rect(Rect::from_xywh(3.0, 3.0, 4.0, 4.0), &t, 10.0).offset, Point::ZERO);
        assert_eq!(snap_point(Point::new(f64::NAN, 1.0), &Targets { grid: Some(10.0), ..Targets::default() }, 10.0), Snapped::default());
        assert_eq!(snap_point(Point::new(3.0, 3.0), &Targets { grid: Some(-10.0), ..Targets::default() }, 10.0).offset, Point::ZERO);
    }

    #[test]
    fn grid_step_follows_zoom_and_units() {
        // Inches: at 100% a step of at least 8 points is an eighth of an inch (12.5 units).
        assert_eq!(grid_step(Units::Inches, 1.0, 8.0), 12.5);
        // Zoomed in four times, the grid gets four times finer.
        assert_eq!(grid_step(Units::Inches, 4.0, 8.0), 3.125);
        // Zoomed out, coarser.
        assert_eq!(grid_step(Units::Inches, 0.25, 8.0), 50.0);
        // Millimetres step in 1, 2, 5: two millimetres is under 8 points at 100%, five is not.
        let mm = unit_size(Units::Millimeters);
        assert!((grid_step(Units::Millimeters, 1.0, 8.0) - 5.0 * mm).abs() < 1e-9);
        assert_eq!(grid_step(Units::Pixels, 1.0, 8.0), 10.0);
        assert!(grid_step(Units::Inches, f64::NAN, 8.0).is_finite());
        assert!(grid_step(Units::Inches, 1e-30, 8.0).is_finite());
        assert_eq!(major_every(Units::Inches, 12.5), 8);
        assert_eq!(major_every(Units::Inches, 200.0), 4);
        assert_eq!(major_every(Units::Pixels, 10.0), 5);
    }
}
