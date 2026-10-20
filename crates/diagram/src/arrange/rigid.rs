//! Transforming a set of shapes and connectors as one object: move, then scale about a pivot,
//! then turn about a pivot. The engine's `shape.transform` and the canvas preview of a group
//! resize or rotate both call [`apply`], so what a drag shows is what the command makes.
//!
//! The rules, per item:
//!
//! - A shape's centre goes through the whole transform. Its size scales by the factors, swapped
//!   when the shape is turned nearer a quarter than not (its own width then runs along the page's
//!   y), which is exact for shapes square to the page and an approximation for others; its angle
//!   grows by the turn.
//! - A symbol drawn by a glyph builder, and a wire junction, keeps its size (symbols never
//!   distort) and turns in quarter turns through its orientation (text upright, pins turned), the
//!   nearest whole number of quarters to the turn; the turn left over moves its centre only.
//! - Groups carry their members and are refitted afterwards.
//! - A connector among the items, or glued at both ends to shapes among them, is carried: its
//!   bends and free ends go through the transform and its glued ends stay glued. A connector
//!   among the items with one end glued to a shape outside them keeps that end, moves its free
//!   end and drops its bends, so it is routed again from its ends.
//! - On a page with a symbol grid module, each carried symbol's first pin, and each carried bend
//!   and free end, lands on the module.
//!
//! Nothing here panics: non-finite numbers leave the page as it was, coordinates are clamped to
//! the model's range and sizes to `MAX_EXTENT`.

use std::collections::HashSet;

use super::{coord, is_group, refit_groups, top_level, with_descendants};
use crate::MAX_EXTENT;
use crate::geom::{Point, Transform};
use crate::model::{End, Page, Shape};

/// Smallest width or height a scale leaves a shape that had at least this much.
pub const MIN_SIZE: f64 = 1.0;

/// A transform of a set of items as one rigid body: a move by `dx, dy`, then a scale by `sx, sy`
/// about `scale_about`, then a clockwise turn of `deg` degrees about `rotate_about`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigid {
    /// Move along x.
    pub dx: f64,
    /// Move along y.
    pub dy: f64,
    /// Scale along x (1 for none).
    pub sx: f64,
    /// Scale along y (1 for none).
    pub sy: f64,
    /// The fixed point of the scale.
    pub scale_about: Point,
    /// Clockwise turn in degrees (0 for none).
    pub deg: f64,
    /// The fixed point of the turn.
    pub rotate_about: Point,
}

impl Default for Rigid {
    fn default() -> Self {
        Rigid { dx: 0.0, dy: 0.0, sx: 1.0, sy: 1.0, scale_about: Point::ZERO, deg: 0.0, rotate_about: Point::ZERO }
    }
}

impl Rigid {
    /// A move by `dx, dy`.
    pub fn moved(dx: f64, dy: f64) -> Self {
        Rigid { dx, dy, ..Rigid::default() }
    }
    /// A scale by `sx, sy` about `about`.
    pub fn scaled(sx: f64, sy: f64, about: Point) -> Self {
        Rigid { sx, sy, scale_about: about, ..Rigid::default() }
    }
    /// A clockwise turn of `deg` degrees about `about`.
    pub fn turned(deg: f64, about: Point) -> Self {
        Rigid { deg, rotate_about: about, ..Rigid::default() }
    }
    /// Whether every number is finite and both scale factors are non-zero.
    pub fn is_valid(&self) -> bool {
        [self.dx, self.dy, self.sx, self.sy, self.deg].iter().all(|v| v.is_finite())
            && self.scale_about.is_finite()
            && self.rotate_about.is_finite()
            && self.sx.abs() > 1e-9
            && self.sy.abs() > 1e-9
    }
    /// Whether it changes nothing.
    pub fn is_identity(&self) -> bool {
        self.dx == 0.0 && self.dy == 0.0 && self.sx == 1.0 && self.sy == 1.0 && self.deg == 0.0
    }
    /// The whole transform as one matrix.
    pub fn matrix(&self) -> Transform {
        let a = self.scale_about;
        Transform::translate(self.dx, self.dy)
            .then(&Transform::translate(-a.x, -a.y))
            .then(&Transform::scale(self.sx, self.sy))
            .then(&Transform::translate(a.x, a.y))
            .then(&Transform::rotate_about(self.rotate_about, self.deg))
    }
    /// Where the transform takes `p`.
    pub fn apply(&self, p: Point) -> Point {
        self.matrix().apply(p)
    }
    /// The whole quarter turns a symbol takes for this turn (the nearest).
    pub fn quarter_turns(&self) -> i64 {
        let q = (self.deg / 90.0).round();
        if q.is_finite() { q.clamp(-4.0e6, 4.0e6) as i64 } else { 0 }
    }
}

/// Whether `s` keeps its size and turns in quarter turns: a symbol drawn by a glyph builder, or a
/// wire junction.
pub fn fixed_size(s: &Shape) -> bool {
    (s.glyph.is_some() && !is_group(s)) || crate::net::is_junction(s)
}

/// What [`apply`] changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// The shapes transformed, members of groups included.
    pub shapes: Vec<String>,
    /// The connectors carried (bends and free ends transformed).
    pub carried: Vec<String>,
    /// The connectors among the items that keep an end on a shape outside them and are routed
    /// again.
    pub rerouted: Vec<String>,
}

/// `v` on the nearest multiple of `step`.
fn on_module(v: f64, step: f64) -> f64 {
    crate::snap::round_to(v, step)
}

/// Transform the shapes and connectors named by `ids` on `page` as one object (see the module
/// docs). `module` is the page's symbol grid step, if it has one. An invalid transform changes
/// nothing.
pub fn apply(page: &mut Page, ids: &[String], t: &Rigid, module: Option<f64>) -> Outcome {
    let mut out = Outcome::default();
    if !t.is_valid() {
        return out;
    }
    let module = module.filter(|m| m.is_finite() && *m > 0.0);
    let m = t.matrix();
    let top = top_level(page, ids);
    let shapes = with_descendants(page, &top);
    let moving: HashSet<&str> = shapes.iter().map(String::as_str).collect();
    let named: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let quarter = t.quarter_turns();
    for s in page.shapes.iter_mut().filter(|s| moving.contains(s.id.as_str())) {
        let c = m.apply(s.center());
        if fixed_size(s) {
            if quarter != 0 {
                s.orient = s.orient.turned(quarter);
            }
        } else {
            // Turned nearer a quarter than not: the shape's width runs along the page's y.
            let rel = s.angle_deg.rem_euclid(180.0);
            let (fx, fy) = if (rel - 90.0).abs() < 45.0 { (t.sy.abs(), t.sx.abs()) } else { (t.sx.abs(), t.sy.abs()) };
            s.w = (s.w * fx).clamp(s.w.min(MIN_SIZE), MAX_EXTENT);
            s.h = (s.h * fy).clamp(s.h.min(MIN_SIZE), MAX_EXTENT);
            s.angle_deg += t.deg;
        }
        s.x = coord(c.x - s.w / 2.0);
        s.y = coord(c.y - s.h / 2.0);
        if let Some(step) = module.filter(|_| fixed_size(s)) {
            snap_first_pin(s, step);
        }
        s.sanitize();
        out.shapes.push(s.id.clone());
    }
    let inside = |e: &End| match e {
        End::Glued { shape, .. } => moving.contains(shape.as_str()),
        End::Free { .. } => true,
    };
    let glued_inside = |e: &End| matches!(e, End::Glued { shape, .. } if moving.contains(shape.as_str()));
    let move_end = |e: &mut End| {
        if let End::Free { x, y } = e {
            let mut p = m.apply(Point::new(*x, *y));
            if let Some(step) = module {
                p = Point::new(on_module(p.x, step), on_module(p.y, step));
            }
            (*x, *y) = (coord(p.x), coord(p.y));
        }
    };
    for c in &mut page.connectors {
        let is_named = named.contains(c.id.as_str());
        let both_glued = glued_inside(&c.start) && glued_inside(&c.end);
        if !is_named && !both_glued {
            continue;
        }
        if inside(&c.start) && inside(&c.end) {
            for p in &mut c.waypoints {
                let mut q = m.apply(*p);
                if let Some(step) = module {
                    q = Point::new(on_module(q.x, step), on_module(q.y, step));
                }
                *p = Point::new(coord(q.x), coord(q.y));
            }
            move_end(&mut c.start);
            move_end(&mut c.end);
            out.carried.push(c.id.clone());
        } else {
            move_end(&mut c.start);
            move_end(&mut c.end);
            c.waypoints.clear();
            out.rerouted.push(c.id.clone());
        }
        c.sanitize();
    }
    refit_groups(page);
    out
}

/// Move `s` so its first connection point (through its transform, so a turned symbol snaps the
/// pin where it is drawn) lands on the `step` grid, as a placed symbol does.
fn snap_first_pin(s: &mut Shape, step: f64) {
    let Some(p) = s.connection_point_world(0) else { return };
    let (dx, dy) = (on_module(p.x, step) - p.x, on_module(p.y, step) - p.y);
    if dx.is_finite() && dy.is_finite() {
        s.x = coord(s.x + dx);
        s.y = coord(s.y + dy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShapeKind;
    use crate::model::Connector;

    fn page() -> Page {
        let mut p = Page::new("p1", "Page 1");
        p.shapes.push(Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 40.0, 20.0));
        p.shapes.push(Shape::new("b", ShapeKind::Rectangle, 200.0, 100.0, 40.0, 20.0));
        let mut c =
            Connector::new("c", End::Glued { shape: "a".into(), point: None, port: None }, End::Glued { shape: "b".into(), point: None, port: None });
        c.waypoints = vec![Point::new(100.0, 10.0), Point::new(100.0, 110.0)];
        p.connectors.push(c);
        p
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_move_scale_and_turn_carry_shapes_and_bends_together() {
        let mut p = page();
        let t = Rigid { dx: 10.0, dy: 0.0, sx: 2.0, sy: 2.0, scale_about: Point::ZERO, deg: 90.0, rotate_about: Point::ZERO };
        let out = apply(&mut p, &ids(&["a", "b", "c"]), &t, None);
        assert_eq!(out.shapes, ids(&["a", "b"]));
        assert_eq!(out.carried, ids(&["c"]));
        let a = p.shape("a").unwrap();
        // Centre (20, 10) -> moved (30, 10) -> scaled (60, 20) -> turned (-20, 60).
        assert!(a.center().distance(Point::new(-20.0, 60.0)) < 1e-9, "{:?}", a.center());
        assert_eq!((a.w, a.h, a.angle_deg), (80.0, 40.0, 90.0));
        let c = p.connector("c").unwrap();
        // Bend (100, 10) -> (110, 10) -> (220, 20) -> (-20, 220).
        assert!(c.waypoints[0].distance(Point::new(-20.0, 220.0)) < 1e-9, "{:?}", c.waypoints);
    }

    #[test]
    fn a_connector_glued_outside_keeps_its_end_and_drops_its_bends() {
        let mut p = page();
        apply(&mut p, &ids(&["a", "c"]), &Rigid::moved(50.0, 0.0), None);
        let c = p.connector("c").unwrap();
        assert!(c.waypoints.is_empty());
        assert_eq!(p.shape("b").unwrap().x, 200.0, "the shape outside stays");
        // Not named, glued at both ends to moved shapes: carried anyway.
        let mut p = page();
        let out = apply(&mut p, &ids(&["a", "b"]), &Rigid::moved(50.0, 0.0), None);
        assert_eq!(out.carried, ids(&["c"]));
        assert_eq!(p.connector("c").unwrap().waypoints[0], Point::new(150.0, 10.0));
    }

    #[test]
    fn symbols_keep_their_size_and_turn_in_quarters() {
        let mut p = page();
        if let Some(s) = p.shape_mut("a") {
            s.glyph = Some(crate::glyph_state::GlyphState::default());
        }
        let about = Point::new(20.0, 10.0);
        apply(&mut p, &ids(&["a"]), &Rigid::scaled(3.0, 3.0, Point::ZERO), None);
        let a = p.shape("a").unwrap();
        assert_eq!((a.w, a.h), (40.0, 20.0));
        assert!(a.center().distance(Point::new(60.0, 30.0)) < 1e-9);
        let mut p = page();
        if let Some(s) = p.shape_mut("a") {
            s.glyph = Some(crate::glyph_state::GlyphState::default());
        }
        apply(&mut p, &ids(&["a"]), &Rigid::turned(100.0, about), None);
        let a = p.shape("a").unwrap();
        assert_eq!((a.orient.turns, a.angle_deg), (1, 0.0));
    }

    #[test]
    fn invalid_transforms_change_nothing() {
        for t in [Rigid::scaled(0.0, 1.0, Point::ZERO), Rigid::moved(f64::NAN, 0.0), Rigid::turned(f64::INFINITY, Point::ZERO)] {
            let mut p = page();
            let before = p.clone();
            let out = apply(&mut p, &ids(&["a", "b", "c"]), &t, None);
            assert_eq!(p, before);
            assert!(out.shapes.is_empty());
        }
        let mut p = page();
        apply(&mut p, &ids(&["a", "b", "c"]), &Rigid::scaled(1e300, 1e-300, Point::ZERO), None);
        assert!(p.shapes.iter().all(|s| s.w.is_finite() && s.h >= 1.0 && s.x.is_finite()));
    }

    #[test]
    fn the_module_puts_first_pins_and_bends_on_the_grid() {
        let mut p = page();
        if let Some(s) = p.shape_mut("a") {
            s.glyph = Some(crate::glyph_state::GlyphState::default());
        }
        apply(&mut p, &ids(&["a", "b", "c"]), &Rigid::moved(3.3, 1.1), Some(5.0));
        let pin = p.shape("a").unwrap().connection_point_world(0).unwrap();
        let on = |v: f64| ((v / 5.0) - (v / 5.0).round()).abs() < 1e-9;
        assert!(on(pin.x) && on(pin.y), "{pin:?}");
        assert!(p.connector("c").unwrap().waypoints.iter().all(|w| on(w.x) && on(w.y)));
    }
}
