//! Drags: handle geometry (resize and rotate), the preview page a drag draws from, and the one
//! command a finished drag commits.
//!
//! Shape locks (Developer > Protection) shape every drag here: a position-locked shape does not
//! move, a locked side is not resized (a corner handle then changes the free side only), the
//! aspect lock keeps the ratio, and a rotation-locked shape does not turn. Document protection
//! stops them all ([`protected`]). The engine refuses the same edits; the canvas never even
//! previews them.

use serde_json::json;
use vizcraft_diagram::{self as dg, Connector, Document, End, Page, Point, Shape, snap};

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

/// Whether document protection stops every edit of the shapes on `page`: Protect Document with
/// shapes on, or with backgrounds on and `page` a background page.
pub fn protected(doc: &Document, page: &Page) -> bool {
    doc.protection.shapes || (doc.protection.backgrounds && page.is_background)
}

/// What resize handle `handle` of `s` does under the shape's locks, in normalized frame
/// coordinates: a locked side acts as if the handle sat at its middle, so a corner changes only
/// the free side. `None` when the handle can change nothing (an edge of a locked side, both sides
/// locked, or a side locked together with the aspect).
fn effective_handle(s: &Shape, handle: usize) -> Option<(f64, f64)> {
    let (hx, hy) = HANDLES.get(handle).copied()?;
    let l = &s.locks;
    if (l.width || l.height) && l.aspect {
        return None;
    }
    let hx = if l.width { 0.5 } else { hx };
    let hy = if l.height { 0.5 } else { hy };
    (hx != 0.5 || hy != 0.5).then_some((hx, hy))
}

/// Whether resize handle `handle` of `s` can change the shape (the overlay hides it otherwise).
pub fn handle_allowed(s: &Shape, handle: usize) -> bool {
    effective_handle(s, handle).is_some() && !super::sequence::frame_fixed(s)
}

/// Whether `s` may be turned with the rotate handle.
pub fn rotate_allowed(s: &Shape) -> bool {
    !s.locks.rotation && !super::sequence::frame_fixed(s)
}

/// Whether a move drag may carry `ids` on `page`: none of them is a position-locked shape.
pub fn move_allowed(page: &Page, ids: &[String]) -> bool {
    !ids.iter().any(|id| page.shape(id).is_some_and(|s| s.locks.position))
}

/// The frame `(x, y, w, h)` a resize handle drag to `to` gives, keeping the opposite handle in
/// place (rotation included). With `keep_aspect` (or the shape's aspect lock) a corner scales
/// both sides by the larger factor and an edge scales the other side to match. A locked side
/// keeps its size; `None` when the locks leave the handle nothing to change.
pub fn resize_frame(s: &Shape, handle: usize, to: Point, keep_aspect: bool) -> Option<(f64, f64, f64, f64)> {
    let (hx, hy) = effective_handle(s, handle)?;
    let keep_aspect = keep_aspect || s.locks.aspect;
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
/// the page (a marquee). Connectors the drag can affect (glued to what moves, running over it, or
/// being drawn or reglued) are rerouted around the shapes once per frame with a small search
/// budget, so a shape is never seen being crossed while it moves; the command the drag commits
/// runs the real routing pass.
pub fn preview_page(page: &Page, drag: &Drag, zoom: f32) -> Option<Page> {
    let mut p = page.clone();
    let moving: Vec<String> = match drag {
        Drag::Marquee { .. } | Drag::Lasso { .. } | Drag::TextBox { .. } => return None,
        Drag::Sequence(d) => return super::sequence::preview(page, d),
        Drag::TextBlock { id, grab, to, shift } => {
            super::text_block::preview(&mut p, id, grab, *to, *shift);
            Vec::new()
        }
        Drag::Move { ids, .. } if !move_allowed(page, ids) => Vec::new(),
        Drag::Move { ids, from, to } => {
            let d = move_delta(*from, *to);
            dg::arrange::translate(&mut p, ids, d.x, d.y);
            for c in p.connectors.iter_mut().filter(|c| ids.contains(&c.id)) {
                offset_end(&mut c.start, d);
                offset_end(&mut c.end, d);
            }
            ids.clone()
        }
        Drag::Resize { id, handle, to, keep_aspect } => {
            let frame = page.shape(id).and_then(|s| resize_frame(s, *handle, *to, *keep_aspect));
            if let Some((x, y, w, h)) = frame {
                dg::arrange::set_frame(&mut p, id, x, y, w, h);
            }
            vec![id.clone()]
        }
        Drag::Rotate { id, to, snap } => {
            if let Some(a) = page.shape(id).filter(|s| rotate_allowed(s)).map(|s| rotate_angle(s, *to, *snap)) {
                dg::arrange::set_angle(&mut p, id, a);
            }
            vec![id.clone()]
        }
        Drag::Connect { from, to } => {
            let end = target_end(page, *to, zoom);
            p.connectors.push(Connector::new(PREVIEW_ID, from.clone(), end));
            vec![PREVIEW_ID.to_string()]
        }
        // Wires are not rerouted around shapes, so the preview needs no search.
        Drag::Wire { from, to, pipe } => return Some(super::wire::preview(page, from, *to, *pipe, zoom)),
        Drag::Reglue { id, start, to } => {
            let end = target_end(page, *to, zoom);
            if let Some(c) = p.connector_mut(id) {
                if *start {
                    c.start = end;
                } else {
                    c.end = end;
                }
            }
            vec![id.clone()]
        }
    };
    if !moving.is_empty() {
        dg::autoroute::preview(&mut p, &moving, dg::autoroute::PREVIEW_BUDGET);
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
    // Protection refuses every shape edit a drag commits (selection drags still run).
    let edits = matches!(drag, Drag::Move { .. } | Drag::Resize { .. } | Drag::Rotate { .. } | Drag::Sequence(_));
    if edits && protected(&app.session.doc, page) {
        app.status("The shapes are protected (Developer > Protect Document)");
        return;
    }
    match drag {
        Drag::Move { ids, .. } if !move_allowed(page, &ids) => {
            app.status("A selected shape is locked against moving (Developer > Protection)");
        }
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
        Drag::Lasso { points, add } => {
            let ids = super::lasso_ids(page, &points);
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
                super::membership::commit_move(app, &ids, d);
                // A device moved onto a wire is spliced into it.
                if let [one] = ids.as_slice() {
                    super::wire::splice_if_on_wire(app, one);
                }
            }
        }
        Drag::Resize { id, handle, to, keep_aspect } => {
            if let Some((x, y, w, h)) = page.shape(&id).and_then(|s| resize_frame(s, handle, to, keep_aspect)) {
                let (w, h) = (round(w).max(MIN_SIZE), round(h).max(MIN_SIZE));
                run(app, "shape.resize", json!({"id": id, "x": round(x), "y": round(y), "w": w, "h": h}));
            }
        }
        Drag::Rotate { id, to, snap } => {
            if let Some(s) = page.shape(&id).filter(|s| rotate_allowed(s)) {
                let a = rotate_angle(s, to, snap);
                if (a - s.angle_deg).abs() > 1e-6 {
                    run(app, "shape.rotate", json!({"ids": [id], "angle": a}));
                }
            }
        }
        Drag::Connect { from, to } => {
            let end = target_end(page, to, zoom);
            if connect_ok(&from, &end) {
                run(
                    app,
                    "connector.add",
                    json!({"from": end_json(&from), "to": end_json(&end), "kind": (!app.connector_kind.is_empty()).then(|| app.connector_kind.clone())}),
                );
            }
        }
        Drag::Wire { from, to, pipe } => super::wire::finish(app, from, to, pipe, zoom),
        Drag::Sequence(d) => super::sequence::finish(app, d, zoom),
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
