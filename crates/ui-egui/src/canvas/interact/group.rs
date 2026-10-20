//! Transforming a multi-selection as one object: one frame around everything selected, with the
//! eight resize handles and the rotate handle a single shape has, and a move that carries the
//! wires between the shapes.
//!
//! A group drag ([`Drag::ResizeGroup`], [`Drag::RotateGroup`], and a [`Drag::Move`] of more than
//! one item) previews by running `vizcraft_diagram::arrange::rigid` on a copy of the page, the
//! code `shape.transform` runs, and commits that one command on release (inside an `edit.batch`
//! named Move, Resize or Rotate, so Undo names the gesture), so the preview is what the command
//! makes and the drag is one undo step.
//!
//! The handles follow the locks: a position lock on any selected shape hides them all (every
//! group transform moves centres), a rotation lock hides the rotate handle, a width or height
//! lock on a shape that would scale hides the resize handles, and an aspect lock keeps the
//! group's ratio and hides the edge handles. The engine refuses the whole transform for one
//! locked shape, so a handle that could only fail is not drawn. On a page with a symbol grid
//! module the turn snaps to quarter turns, so symbols stay on the grid.

use serde_json::json;
use vizcraft_diagram::arrange::rigid::{self, Rigid};
use vizcraft_diagram::{self as dg, Document, Page, Point, Rect};

use super::{Drag, HANDLES, MIN_SIZE, ROTATE_GAP, SNAP_DEG, run};
use crate::CraftApp;

/// A selection of more than one item, with the union of their bounds and routes.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// The selected ids on the page, in selection order.
    pub ids: Vec<String>,
    /// The frame the handles sit on.
    pub bounds: Rect,
}

/// The selection as a group, when it holds more than one item on `page` and none of them is a
/// part a sequence page places itself.
pub fn selection(app: &CraftApp, page: &Page) -> Option<Group> {
    let ids: Vec<String> = app.session.sel.items.iter().filter(|id| page.contains_id(id)).cloned().collect();
    if ids.len() < 2 {
        return None;
    }
    let all = dg::arrange::with_descendants(page, &ids);
    if all.iter().filter_map(|id| page.shape(id)).any(super::sequence::frame_fixed) {
        return None;
    }
    let bounds = bounds_of(page, &ids)?;
    Some(Group { ids, bounds })
}

/// Union of the bounds of the shapes among `ids` and the drawn routes of the connectors among
/// them, `None` when nothing has a finite extent.
pub fn bounds_of(page: &Page, ids: &[String]) -> Option<Rect> {
    let shapes = dg::arrange::bounds_of(page, ids);
    let lines = page
        .connectors
        .iter()
        .filter(|c| ids.contains(&c.id))
        .filter_map(|c| Rect::bounding(dg::route_polyline(page, c).into_iter().filter(|p| p.is_finite())));
    shapes.into_iter().chain(lines).reduce(|a, b| a.union(&b)).filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()))
}

/// Which group handles may be used.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Allowed {
    /// Each resize handle, in [`HANDLES`] order.
    pub resize: [bool; 8],
    /// The rotate handle.
    pub rotate: bool,
    /// A resize keeps the ratio (a selected shape has an aspect lock).
    pub aspect: bool,
}

/// The handles the locks on the shapes among `ids` (and inside them) and document protection
/// leave usable (see the module docs).
pub fn allowed(doc: &Document, page: &Page, ids: &[String]) -> Allowed {
    if super::drag::protected(doc, page) {
        return Allowed::default();
    }
    let all = dg::arrange::with_descendants(page, ids);
    let shapes: Vec<&dg::Shape> = all.iter().filter_map(|id| page.shape(id)).collect();
    if shapes.iter().any(|s| s.locks.position) {
        return Allowed::default();
    }
    let scaling = shapes.iter().filter(|s| !rigid::fixed_size(s));
    let (mut sized, mut aspect) = (true, false);
    for s in scaling {
        sized &= !(s.locks.width || s.locks.height);
        aspect |= s.locks.aspect;
    }
    let mut resize = [false; 8];
    for (i, (hx, hy)) in HANDLES.iter().enumerate() {
        let corner = *hx != 0.5 && *hy != 0.5;
        resize[i] = sized && (corner || !aspect);
    }
    Allowed { resize, rotate: !shapes.iter().any(|s| s.locks.rotation), aspect }
}

/// Resize handle `i` of a group frame `r`, in world coordinates.
pub fn handle_world(r: &Rect, i: usize) -> Option<Point> {
    let (hx, hy) = HANDLES.get(i).copied()?;
    Some(Point::new(r.x0 + hx * r.width(), r.y0 + hy * r.height()))
}

/// The rotate handle of a group frame `r`: above the middle of its top edge.
pub fn rotate_handle_world(r: &Rect, zoom: f32) -> Point {
    let gap = f64::from(ROTATE_GAP) / f64::from(zoom.max(0.01));
    Point::new((r.x0 + r.x1) / 2.0, r.y0 - gap)
}

/// Whether a group turn on `page` snaps to quarter turns whatever Shift says: the page has a
/// symbol grid module.
pub fn quarter_only(page: &Page) -> bool {
    dg::snap::module_step(page).is_some()
}

/// The scale a drag of resize handle `handle` of frame `r` to `to` makes, about the opposite
/// handle. With `keep_aspect` a corner scales both sides by the larger factor and an edge scales
/// the other side to match. The new frame size is whole world units and never under
/// [`MIN_SIZE`]; a side with no extent keeps a factor of one.
pub fn resize_rigid(r: &Rect, handle: usize, to: Point, keep_aspect: bool) -> Option<Rigid> {
    let (hx, hy) = HANDLES.get(handle).copied()?;
    if !to.is_finite() {
        return None;
    }
    let (w, h) = (r.width(), r.height());
    let about = Point::new(r.x0 + (1.0 - hx) * w, r.y0 + (1.0 - hy) * h);
    let factor = |extent: f64, along: f64, sign: f64| {
        if extent < 1e-6 {
            return 1.0;
        }
        (((along * sign).round()).max(MIN_SIZE)) / extent
    };
    let mut sx = if hx == 0.5 { 1.0 } else { factor(w, to.x - about.x, (hx - 0.5).signum()) };
    let mut sy = if hy == 0.5 { 1.0 } else { factor(h, to.y - about.y, (hy - 0.5).signum()) };
    if keep_aspect {
        if hx != 0.5 && hy != 0.5 {
            let k = sx.max(sy);
            (sx, sy) = (k, k);
        } else if hx == 0.5 {
            sx = sy;
        } else {
            sy = sx;
        }
    }
    let t = Rigid::scaled(sx, sy, about);
    t.is_valid().then_some(t)
}

/// The turn, in degrees clockwise, a drag of the rotate handle of frame `r` to `to` makes: the
/// handle points at the pointer. `quarter` snaps to right angles, else `snap` to [`SNAP_DEG`].
pub fn rotate_deg(r: &Rect, to: Point, snap: bool, quarter: bool) -> f64 {
    let v = to - r.center();
    let mut a = v.y.atan2(v.x).to_degrees() + 90.0;
    if quarter {
        a = (a / 90.0).round() * 90.0;
    } else if snap {
        a = (a / SNAP_DEG).round() * SNAP_DEG;
    }
    let a = a.rem_euclid(360.0);
    if a.is_finite() { a } else { 0.0 }
}

/// The transform a group drag makes on `page`, `None` for any other drag.
pub fn rigid_of(page: &Page, drag: &Drag) -> Option<Rigid> {
    match drag {
        Drag::ResizeGroup { bounds, handle, to, keep_aspect, ids } => {
            let aspect = *keep_aspect || allowed_aspect(page, ids);
            resize_rigid(bounds, *handle, *to, aspect)
        }
        Drag::RotateGroup { bounds, to, snap, .. } => Some(Rigid::turned(rotate_deg(bounds, *to, *snap, quarter_only(page)), bounds.center())),
        Drag::Move { ids, from, to } if ids.len() > 1 => {
            let d = super::drag::move_delta(*from, *to);
            Some(Rigid::moved(d.x, d.y))
        }
        _ => None,
    }
}

/// Whether a selected shape that scales has an aspect lock.
fn allowed_aspect(page: &Page, ids: &[String]) -> bool {
    dg::arrange::with_descendants(page, ids).iter().filter_map(|id| page.shape(id)).any(|s| !rigid::fixed_size(s) && s.locks.aspect)
}

/// Apply `t` to `ids` on the preview copy `p`, as `shape.transform` will. Returns what moved, for
/// the preview's rerouting.
pub fn preview(p: &mut Page, ids: &[String], t: &Rigid) -> Vec<String> {
    let module = dg::snap::module_step(p);
    let out = rigid::apply(p, ids, t, module);
    out.shapes.into_iter().chain(out.carried).chain(out.rerouted).collect()
}

/// The `shape.transform` params for `t` on `ids`: only the parts it uses.
pub fn params(ids: &[String], t: &Rigid) -> serde_json::Value {
    let mut v = json!({"ids": ids});
    if let Some(o) = v.as_object_mut() {
        if t.dx != 0.0 || t.dy != 0.0 {
            o.insert("move".into(), json!({"dx": t.dx, "dy": t.dy}));
        }
        if t.sx != 1.0 || t.sy != 1.0 {
            o.insert("scale".into(), json!({"sx": t.sx, "sy": t.sy, "about": {"x": t.scale_about.x, "y": t.scale_about.y}}));
        }
        if t.deg != 0.0 {
            o.insert("rotate".into(), json!({"angle": t.deg, "about": {"x": t.rotate_about.x, "y": t.rotate_about.y}}));
        }
    }
    v
}

/// Run `shape.transform` with `params` as one undo step named `label` (Move, Resize, Rotate),
/// so Undo names the gesture the way it does for a single shape.
pub(super) fn run_named(app: &mut CraftApp, label: &str, params: serde_json::Value) {
    run(app, "edit.batch", json!({"label": label, "commands": [{"command": "shape.transform", "params": params}]}));
}

/// Commit a finished group resize or rotate as one `shape.transform`.
pub(super) fn finish(app: &mut CraftApp, drag: &Drag) {
    let Some(page) = app.session.doc.page() else { return };
    let (label, ids) = match drag {
        Drag::ResizeGroup { ids, .. } => ("Resize", ids),
        Drag::RotateGroup { ids, .. } => ("Rotate", ids),
        _ => return,
    };
    let Some(t) = rigid_of(page, drag).filter(|t| !t.is_identity()) else { return };
    run_named(app, label, params(ids, &t));
}

/// The status bar's angle while a rotate handle is dragged, single shape or group.
pub fn status(app: &CraftApp) -> Option<String> {
    let page = app.session.doc.page()?;
    let deg = match app.canvas.drag.as_ref()? {
        Drag::RotateGroup { bounds, to, snap, .. } => rotate_deg(bounds, *to, *snap, quarter_only(page)),
        Drag::Rotate { id, to, snap } => super::rotate_angle(page.shape(id)?, *to, *snap),
        _ => return None,
    };
    Some(format!("Angle {}\u{b0}", deg.round()))
}

/// The frame of a group drag (or of the group with no drag), for drawing: its four corners, its
/// eight handles and the rotate handle with the point on the frame it hangs from, all in world
/// coordinates and turned and scaled with the drag in progress.
pub struct Frame {
    /// Corners, clockwise from the top left.
    pub corners: [Point; 4],
    /// Resize handles in [`HANDLES`] order.
    pub handles: Vec<Point>,
    /// The middle of the top edge, and the rotate handle above it.
    pub rotate: (Point, Point),
}

/// The frame of `r` under `t` at `zoom` (the rotate handle keeps its screen distance).
pub fn frame(r: &Rect, t: &Rigid, zoom: f32) -> Frame {
    let m = t.matrix();
    let corners = [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)].map(|p| m.apply(p));
    let handles = (0..HANDLES.len()).filter_map(|i| handle_world(r, i)).map(|p| m.apply(p)).collect();
    let top = m.apply(Point::new((r.x0 + r.x1) / 2.0, r.y0));
    // The handle hangs off the turned top edge, ROTATE_GAP screen points out.
    let gap = f64::from(ROTATE_GAP) / f64::from(zoom.max(0.01));
    let up = (top - m.apply(r.center())).normalized().unwrap_or(Point::new(0.0, -1.0));
    Frame { corners, handles, rotate: (top, top + Point::new(up.x * gap, up.y * gap)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corner_scales_about_the_opposite_corner_and_shift_keeps_the_ratio() {
        let r = Rect::new(100.0, 100.0, 300.0, 200.0);
        // Bottom-right handle (4) dragged to (500, 250): twice as wide, half as tall again.
        let t = resize_rigid(&r, 4, Point::new(500.0, 250.0), false).unwrap();
        assert_eq!((t.sx, t.sy, t.scale_about), (2.0, 1.5, Point::new(100.0, 100.0)));
        let t = resize_rigid(&r, 4, Point::new(500.0, 250.0), true).unwrap();
        assert_eq!((t.sx, t.sy), (2.0, 2.0));
        // The top edge (1) moves the top, about the bottom edge; with Shift the width follows.
        let t = resize_rigid(&r, 1, Point::new(0.0, 0.0), false).unwrap();
        assert_eq!((t.sx, t.sy, t.scale_about.y), (1.0, 2.0, 200.0));
        assert_eq!(resize_rigid(&r, 1, Point::new(0.0, 0.0), true).map(|t| t.sx), Some(2.0));
        // Past the anchor clamps to the smallest size instead of flipping.
        let t = resize_rigid(&r, 4, Point::new(0.0, 0.0), false).unwrap();
        assert!(t.sx > 0.0 && t.sy > 0.0);
        assert!(resize_rigid(&r, 9, Point::ZERO, false).is_none());
    }

    #[test]
    fn the_turn_points_the_handle_at_the_pointer_and_snaps() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        let c = r.center();
        assert_eq!(rotate_deg(&r, c + Point::new(0.0, -50.0), false, false), 0.0);
        assert!((rotate_deg(&r, c + Point::new(50.0, 0.0), false, false) - 90.0).abs() < 1e-9);
        assert_eq!(rotate_deg(&r, c + Point::new(50.0, -40.0), true, false), 45.0);
        assert_eq!(rotate_deg(&r, c + Point::new(50.0, -40.0), false, true), 90.0);
        assert_eq!(rotate_deg(&r, c + Point::new(f64::NAN, 0.0), false, false), 0.0);
    }

    #[test]
    fn locks_hide_the_handles_they_stop() {
        let mut page = Page::new("p1", "Page 1");
        page.shapes.push(dg::Shape::new("a", dg::ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
        page.shapes.push(dg::Shape::new("b", dg::ShapeKind::Rectangle, 20.0, 0.0, 10.0, 10.0));
        let ids = vec!["a".to_string(), "b".to_string()];
        let doc = Document::new();
        assert_eq!(allowed(&doc, &page, &ids), Allowed { resize: [true; 8], rotate: true, aspect: false });
        if let Some(s) = page.shape_mut("b") {
            s.locks.aspect = true;
        }
        let a = allowed(&doc, &page, &ids);
        assert_eq!((a.resize, a.aspect), ([true, false, true, false, true, false, true, false], true));
        if let Some(s) = page.shape_mut("b") {
            s.locks.rotation = true;
        }
        assert!(!allowed(&doc, &page, &ids).rotate);
        if let Some(s) = page.shape_mut("a") {
            s.locks.position = true;
        }
        assert_eq!(allowed(&doc, &page, &ids), Allowed::default());
    }
}
