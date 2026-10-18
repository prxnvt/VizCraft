//! Drags: handle geometry (resize and rotate), the preview page a drag draws from, and the one
//! command a finished drag commits.

use serde_json::json;
use vizcraft_diagram::{self as dg, Connector, End, Page, Point, Shape, snap};

use super::{Drag, HANDLES, MIN_SIZE, PREVIEW_ID, ROTATE_GAP, SNAP_DEG, connect_ok, end_json, marquee_ids, round, run, target_end};
use crate::CraftApp;

/// Resize handle `i` of `s` in world coordinates.
pub fn handle_world(s: &Shape, i: usize) -> Option<Point> {
    let (hx, hy) = HANDLES.get(i).copied()?;
    Some(s.transform().apply(Point::new(hx * s.w, hy * s.h)))
}

/// The rotate handle of `s` in world coordinates (above the top edge, turning with the shape).
pub fn rotate_handle_world(s: &Shape, zoom: f32) -> Point {
    let gap = f64::from(ROTATE_GAP) / f64::from(zoom.max(0.01));
    s.transform().apply(Point::new(s.w / 2.0, -gap))
}

/// The frame `(x, y, w, h)` a resize handle drag to `to` gives, keeping the opposite handle in
/// place (rotation included). With `keep_aspect` a corner scales both sides by the larger
/// factor and an edge scales the other side to match.
pub fn resize_frame(s: &Shape, handle: usize, to: Point, keep_aspect: bool) -> Option<(f64, f64, f64, f64)> {
    let (hx, hy) = HANDLES.get(handle).copied()?;
    let (ax, ay) = (1.0 - hx, 1.0 - hy);
    let t = s.transform();
    let local = t.inverse()?.apply(to);
    if !local.is_finite() {
        return None;
    }
    let anchor = Point::new(ax * s.w, ay * s.h);
    let d = local - anchor;
    let (sx, sy) = ((hx - ax).signum(), (hy - ay).signum());
    let mut w = if hx == 0.5 { s.w } else { (d.x * sx).max(MIN_SIZE) };
    let mut h = if hy == 0.5 { s.h } else { (d.y * sy).max(MIN_SIZE) };
    if keep_aspect {
        let (ow, oh) = (s.w.max(1e-6), s.h.max(1e-6));
        if hx != 0.5 && hy != 0.5 {
            let k = (w / ow).max(h / oh);
            (w, h) = (ow * k, oh * k);
        } else if hx == 0.5 {
            w = ow * (h / oh);
        } else {
            h = oh * (w / ow);
        }
    }
    let ox = if hx == 0.5 { 0.0 } else { sx * w / 2.0 };
    let oy = if hy == 0.5 { 0.0 } else { sy * h / 2.0 };
    let c = t.apply(anchor + Point::new(ox, oy));
    let (x, y) = (c.x - w / 2.0, c.y - h / 2.0);
    [x, y, w, h].iter().all(|v| v.is_finite()).then_some((x, y, w, h))
}

/// The absolute angle a rotate handle drag to `to` gives, snapped with `snap`.
pub fn rotate_angle(s: &Shape, to: Point, snap: bool) -> f64 {
    let v = to - s.center();
    let mut a = v.y.atan2(v.x).to_degrees() + 90.0;
    if snap {
        a = (a / SNAP_DEG).round() * SNAP_DEG;
    }
    let a = a.rem_euclid(360.0);
    if a.is_finite() { a } else { s.angle_deg }
}

/// The movement of a move drag, in whole world units.
pub fn move_delta(from: Point, to: Point) -> Point {
    Point::new(round(to.x - from.x), round(to.y - from.y))
}

pub(super) fn offset_end(e: &mut End, d: Point) {
    if let End::Free { x, y } = e {
        *x += d.x;
        *y += d.y;
    }
}

/// A copy of `page` with `drag` applied, for drawing; `None` when the drag changes nothing on
/// the page (a marquee).
pub fn preview_page(page: &Page, drag: &Drag, zoom: f32) -> Option<Page> {
    let mut p = page.clone();
    match drag {
        Drag::Marquee { .. } | Drag::TextBox { .. } => return None,
        Drag::TextBlock { id, grab, to, shift } => super::text_block::preview(&mut p, id, grab, *to, *shift),
        Drag::Move { ids, from, to } => {
            let d = move_delta(*from, *to);
            dg::arrange::translate(&mut p, ids, d.x, d.y);
            for c in p.connectors.iter_mut().filter(|c| ids.contains(&c.id)) {
                offset_end(&mut c.start, d);
                offset_end(&mut c.end, d);
            }
        }
        Drag::Resize { id, handle, to, keep_aspect } => {
            let frame = page.shape(id).and_then(|s| resize_frame(s, *handle, *to, *keep_aspect));
            if let Some((x, y, w, h)) = frame {
                dg::arrange::set_frame(&mut p, id, x, y, w, h);
            }
        }
        Drag::Rotate { id, to, snap } => {
            if let Some(a) = page.shape(id).map(|s| rotate_angle(s, *to, *snap)) {
                dg::arrange::set_angle(&mut p, id, a);
            }
        }
        Drag::Connect { from, to } => {
            let end = target_end(page, *to, zoom);
            p.connectors.push(Connector::new(PREVIEW_ID, from.clone(), end));
        }
        Drag::Reglue { id, start, to } => {
            let end = target_end(page, *to, zoom);
            if let Some(c) = p.connector_mut(id) {
                if *start {
                    c.start = end;
                } else {
                    c.end = end;
                }
            }
        }
    }
    Some(p)
}

/// The shape a connector drag would glue to, for highlighting.
pub fn drag_target(page: &Page, drag: &Drag, zoom: f32) -> Option<End> {
    match drag {
        Drag::Connect { to, .. } | Drag::Reglue { to, .. } => Some(target_end(page, *to, zoom)),
        _ => None,
    }
}

/// Commit a finished drag as one command.
pub(super) fn finish(app: &mut CraftApp, drag: Drag, zoom: f32) {
    let Some(page) = app.session.doc.page() else { return };
    match drag {
        Drag::Marquee { from, to, add } => {
            let ids = marquee_ids(page, &dg::Rect::from_points(from, to));
            if ids.is_empty() {
                if !add && !app.session.sel.items.is_empty() {
                    run(app, "edit.selectNone", json!({}));
                }
            } else {
                run(app, "edit.select", json!({"ids": ids, "add": add}));
            }
        }
        Drag::Move { ids, from, to } => {
            let d = move_delta(from, to);
            // Connectors glued at both ends do not move on their own; dragging only those would
            // record an undo step that changes nothing.
            let movable = ids.iter().any(|id| {
                page.shape(id).is_some()
                    || page.connector(id).is_some_and(|c| matches!(c.start, End::Free { .. }) || matches!(c.end, End::Free { .. }))
            });
            if movable && (d.x != 0.0 || d.y != 0.0) {
                run(app, "shape.move", json!({"ids": ids, "dx": d.x, "dy": d.y}));
            }
        }
        Drag::Resize { id, handle, to, keep_aspect } => {
            if let Some((x, y, w, h)) = page.shape(&id).and_then(|s| resize_frame(s, handle, to, keep_aspect)) {
                let (w, h) = (round(w).max(MIN_SIZE), round(h).max(MIN_SIZE));
                run(app, "shape.resize", json!({"id": id, "x": round(x), "y": round(y), "w": w, "h": h}));
            }
        }
        Drag::Rotate { id, to, snap } => {
            if let Some(s) = page.shape(&id) {
                let a = rotate_angle(s, to, snap);
                if (a - s.angle_deg).abs() > 1e-6 {
                    run(app, "shape.rotate", json!({"ids": [id], "angle": a}));
                }
            }
        }
        Drag::Connect { from, to } => {
            let end = target_end(page, to, zoom);
            if connect_ok(&from, &end) {
                run(app, "connector.add", json!({"from": end_json(&from), "to": end_json(&end)}));
            }
        }
        Drag::TextBox { from, to } => super::tools::add_text_box(app, from, Some(dg::Rect::from_points(from, to))),
        Drag::TextBlock { id, grab, to, shift } => super::text_block::finish(app, &id, &grab, to, shift),
        Drag::Reglue { id, start, to } => {
            let end = target_end(page, to, zoom);
            let current = page.connector(&id).map(|c| if start { &c.start } else { &c.end });
            if current == Some(&end) {
                return;
            }
            let mut p = end_json(&end);
            if let Some(o) = p.as_object_mut() {
                o.insert("id".into(), json!(id));
                o.insert("end".into(), json!(if start { "start" } else { "end" }));
            }
            run(app, "connector.glue", p);
        }
    }
}

// Snapping a move drag. Kept apart from the drag preview and commit above: the canvas calls
// `snap_move` once a frame after the pointer has updated the drag, and it shifts the drag's
// end so the preview, the overlay's alignment lines and the committed `shape.move` all agree.

/// The targets a move of `moving` snaps to on `page` at `zoom`, per the view's snap settings:
/// the grid, guides, and the bounds and connection points of the other visible shapes that
/// allow snapping.
pub fn snap_targets(page: &Page, moving: &[String], aids: &vizcraft_engine::cmd::view::Aids, zoom: f32) -> snap::Targets {
    let mut t = snap::Targets::default();
    if aids.snap_grid {
        t.grid = Some(snap::grid_step(page.units, f64::from(zoom), crate::canvas::draw::overlays::GRID_MIN_PX));
    }
    if aids.guides && aids.snap_guides {
        t.guides = page.guides.clone();
    }
    if aids.snap_shapes || aids.snap_points {
        // One rule with the layer pane: a shape on any layer that turns snapping off is no target.
        let others = page
            .shapes
            .iter()
            .filter(|s| !moving.contains(&s.id) && !s.parent.as_ref().is_some_and(|p| moving.contains(p)) && dg::layer::snappable(page, s));
        for s in others {
            if aids.snap_shapes {
                t.rects.push(s.bounds());
            }
            if aids.snap_points {
                t.points.extend(s.connection_points_world());
            }
        }
    }
    t
}

/// Bounds of the shapes a move drag carries, `None` when it carries none (connectors only).
pub fn moving_bounds(page: &Page, ids: &[String]) -> Option<dg::Rect> {
    page.shapes.iter().filter(|s| ids.contains(&s.id)).map(Shape::bounds).reduce(|a, b| a.union(&b))
}

/// The snap for moving `ids` by `delta`, or `None` with snapping off or nothing to snap.
/// `tol` overrides the view's reach (the overlay asks with a tiny one to find the lines a
/// position already sits on).
pub fn move_snap(
    page: &Page,
    ids: &[String],
    delta: Point,
    aids: &vizcraft_engine::cmd::view::Aids,
    zoom: f32,
    tol: Option<f64>,
) -> Option<snap::Snapped> {
    let reach = aids.snap_tolerance(zoom)?;
    let b = moving_bounds(page, ids)?;
    let moved = dg::Rect::new(b.x0 + delta.x, b.y0 + delta.y, b.x1 + delta.x, b.y1 + delta.y);
    Some(snap::snap_rect(moved, &snap_targets(page, ids, aids, zoom), tol.unwrap_or(reach)))
}

/// Shift the end of a move drag in progress so the items land snapped. Alt (Option) held turns
/// snapping off for the drag, as in other diagramming tools.
pub fn snap_move(app: &mut CraftApp, zoom: f32, alt: bool) {
    if alt {
        return;
    }
    let Some(page) = app.session.doc.page() else { return };
    let Some(Drag::Move { ids, from, to }) = &app.canvas.drag else { return };
    let Some(s) = move_snap(page, ids, *to - *from, &app.session.view.aids, zoom, None) else { return };
    if let Some(Drag::Move { to, .. }) = &mut app.canvas.drag {
        *to += s.offset;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_skips_shapes_on_a_layer_that_turns_snapping_off() {
        let mut page = Page::new("p1", "Page 1");
        page.layers = vec![
            dg::Layer { id: "on".into(), snap: true, ..dg::Layer::default() },
            dg::Layer { id: "off".into(), snap: false, ..dg::Layer::default() },
        ];
        for (id, layers) in [("a", vec!["on"]), ("b", vec!["on", "off"]), ("c", vec![])] {
            let mut s = Shape::new(id, dg::ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
            s.layers = layers.into_iter().map(String::from).collect();
            page.shapes.push(s);
        }
        let aids = vizcraft_engine::cmd::view::Aids { snap_shapes: true, ..Default::default() };
        let t = snap_targets(&page, &["c".to_string()], &aids, 1.0);
        assert_eq!(t.rects.len(), 1, "only `a` is a target: `b` has a no-snap layer, `c` is moving");
    }

    fn rect_shape(angle: f64) -> Shape {
        let mut s = Shape::new("s1", dg::ShapeKind::Rectangle, 100.0, 100.0, 80.0, 40.0);
        s.angle_deg = angle;
        s
    }

    #[test]
    fn corner_resize_keeps_the_opposite_corner() {
        let s = rect_shape(0.0);
        // Bottom-right handle (4) dragged out by 20, 10: the top-left corner stays.
        let (x, y, w, h) = resize_frame(&s, 4, Point::new(200.0, 150.0), false).unwrap();
        assert_eq!((x, y, w, h), (100.0, 100.0, 100.0, 50.0));
        // Top-left handle (0) dragged in: the bottom-right corner stays.
        let (x, y, w, h) = resize_frame(&s, 0, Point::new(120.0, 110.0), false).unwrap();
        assert_eq!((x, y, w, h), (120.0, 110.0, 60.0, 30.0));
        // An edge handle changes one side only.
        let (x, y, w, h) = resize_frame(&s, 3, Point::new(300.0, 0.0), false).unwrap();
        assert_eq!((x, y, w, h), (100.0, 100.0, 200.0, 40.0));
        // Shift on a corner keeps the aspect ratio.
        let (_, _, w, h) = resize_frame(&s, 4, Point::new(260.0, 150.0), true).unwrap();
        assert!((w / h - 2.0).abs() < 1e-9, "{w} x {h}");
        // Dragging past the anchor clamps to the minimum size instead of flipping.
        let (_, _, w, _) = resize_frame(&s, 4, Point::new(0.0, 150.0), false).unwrap();
        assert_eq!(w, MIN_SIZE);
    }

    #[test]
    fn rotated_resize_keeps_the_anchor_in_world_space() {
        let s = rect_shape(90.0);
        let anchor = handle_world(&s, 0).unwrap();
        let to = handle_world(&s, 4).unwrap() + Point::new(-10.0, 25.0);
        let (x, y, w, h) = resize_frame(&s, 4, to, false).unwrap();
        let mut r = s.clone();
        (r.x, r.y, r.w, r.h) = (x, y, w, h);
        let after = handle_world(&r, 0).unwrap();
        assert!(anchor.distance(after) < 1e-6, "{anchor:?} moved to {after:?}");
    }

    #[test]
    fn rotation_angle_points_the_handle_at_the_pointer() {
        let s = rect_shape(0.0);
        let c = s.center();
        assert!((rotate_angle(&s, c + Point::new(0.0, -50.0), false) - 0.0).abs() < 1e-9);
        assert!((rotate_angle(&s, c + Point::new(50.0, 0.0), false) - 90.0).abs() < 1e-9);
        assert!((rotate_angle(&s, c + Point::new(50.0, -45.0), true) - 45.0).abs() < 1e-9);
        assert_eq!(rotate_angle(&s, c + Point::new(5.0, -100.0), true), 0.0);
    }
}
