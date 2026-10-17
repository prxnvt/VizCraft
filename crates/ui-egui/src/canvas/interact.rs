//! Pointer and keyboard interaction on the page: selection (click, Shift-click, marquee), move,
//! resize and rotate handles, drawing and regluing connectors, in-place text editing and arrow
//! key nudges.
//!
//! A drag is previewed in UI state ([`Drag`]) and drawn from a copy of the page with the drag
//! applied ([`preview_page`]); only the release runs a command, so one drag is one undo step and
//! glued connectors follow live without touching the document.

use egui::{Key, Modifiers, Pos2, Response, Ui};
use serde_json::{Value, json};
use vizcraft_diagram::{self as dg, Connector, End, Hit, Page, Point, Shape};

use super::Viewport2D;
use super::draw::{point, pos};
use crate::{CraftApp, Tool, menus};

/// Screen distance (points) within which a handle or connection point is grabbed.
pub const GRAB: f32 = 6.0;
/// Hit tolerance for shapes and connectors, in screen points.
pub const HIT: f32 = 4.0;
/// How far above the frame the rotate handle sits, in screen points.
pub const ROTATE_GAP: f32 = 22.0;
/// Shift-rotation snaps to multiples of this many degrees.
pub const SNAP_DEG: f64 = 15.0;
/// Id of the connector drawn while a connector drag is in progress (never in the document).
pub const PREVIEW_ID: &str = "__preview";
/// Smallest size a resize makes, in world units (the engine clamps to the same floor).
const MIN_SIZE: f64 = 1.0;

/// The eight resize handles in normalized frame coordinates, clockwise from the top-left corner.
pub const HANDLES: [(f64, f64); 8] = [(0.0, 0.0), (0.5, 0.0), (1.0, 0.0), (1.0, 0.5), (1.0, 1.0), (0.5, 1.0), (0.0, 1.0), (0.0, 0.5)];

/// A drag in progress, in world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Drag {
    /// Rubber-band selection; `add` keeps the current selection (Shift).
    Marquee { from: Point, to: Point, add: bool },
    /// Moving the selected items.
    Move { ids: Vec<String>, from: Point, to: Point },
    /// Dragging resize handle `handle` (index into [`HANDLES`]) of a shape.
    Resize { id: String, handle: usize, to: Point, keep_aspect: bool },
    /// Dragging the rotate handle of a shape.
    Rotate { id: String, to: Point, snap: bool },
    /// Drawing a new connector from `from`.
    Connect { from: End, to: Point },
    /// Dragging one end of a connector to reglue it.
    Reglue { id: String, start: bool, to: Point },
}

/// In-place text editing of a shape or connector.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEditing {
    /// What is being edited.
    pub target: Hit,
    /// The text as typed so far.
    pub text: String,
    /// The text before editing, to skip a command when nothing changed.
    pub original: String,
    /// Focus the field on its first frame.
    pub(crate) focus: bool,
}

impl TextEditing {
    /// The id of the edited item.
    pub fn id(&self) -> &str {
        match &self.target {
            Hit::Shape(id) | Hit::Connector(id) => id,
        }
    }
}

/// The single selected shape, if exactly one item is selected and it is a shape.
pub fn single_shape<'a>(app: &CraftApp, page: &'a Page) -> Option<&'a Shape> {
    match app.session.sel.items.as_slice() {
        [id] => page.shape(id),
        _ => None,
    }
}

/// The single selected connector, if exactly one item is selected and it is a connector.
pub fn single_connector<'a>(app: &CraftApp, page: &'a Page) -> Option<&'a Connector> {
    match app.session.sel.items.as_slice() {
        [id] => page.connector(id),
        _ => None,
    }
}

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

/// Where a connector end dropped at `p` attaches: a connection point near `p` (point glue), else
/// the shape under `p` (dynamic glue), else nowhere (a free end).
pub fn target_end(page: &Page, p: Point, zoom: f32) -> End {
    let tol = f64::from(GRAB) / f64::from(zoom.max(0.01));
    match dg::shape_at(page, p, tol) {
        Some(s) => match dg::connection_point_at(s, p, tol * 1.5) {
            Some(i) => End::point(s.id.clone(), i),
            None => End::dynamic(s.id.clone()),
        },
        None => End::free(p),
    }
}

/// Whether a connector from `from` to `to` is worth adding: not a drop back onto the body of the
/// shape it started from.
fn connect_ok(from: &End, to: &End) -> bool {
    match (from, to) {
        (End::Glued { shape: a, .. }, End::Glued { shape: b, point: None }) => a != b,
        (End::Glued { shape: a, point: Some(i) }, End::Glued { shape: b, point: Some(j) }) => a != b || i != j,
        _ => true,
    }
}

fn round(v: f64) -> f64 {
    v.round()
}

/// The movement of a move drag, in whole world units.
fn move_delta(from: Point, to: Point) -> Point {
    Point::new(round(to.x - from.x), round(to.y - from.y))
}

fn offset_end(e: &mut End, d: Point) {
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
        Drag::Marquee { .. } => return None,
        Drag::Move { ids, from, to } => {
            let d = move_delta(*from, *to);
            for s in p.shapes.iter_mut().filter(|s| ids.contains(&s.id)) {
                s.x += d.x;
                s.y += d.y;
            }
            for c in p.connectors.iter_mut().filter(|c| ids.contains(&c.id)) {
                offset_end(&mut c.start, d);
                offset_end(&mut c.end, d);
            }
        }
        Drag::Resize { id, handle, to, keep_aspect } => {
            let frame = page.shape(id).and_then(|s| resize_frame(s, *handle, *to, *keep_aspect));
            if let (Some(s), Some((x, y, w, h))) = (p.shape_mut(id), frame) {
                (s.x, s.y, s.w, s.h) = (x, y, w, h);
            }
        }
        Drag::Rotate { id, to, snap } => {
            if let Some(s) = p.shape_mut(id) {
                s.angle_deg = rotate_angle(s, *to, *snap);
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

/// The marquee rectangle of a drag, if it is one.
pub fn marquee_rect(drag: &Drag) -> Option<dg::Rect> {
    match drag {
        Drag::Marquee { from, to, .. } => Some(dg::Rect::from_points(*from, *to)),
        _ => None,
    }
}

/// Whether segment `a`-`b` touches `r`.
fn segment_hits_rect(a: Point, b: Point, r: &dg::Rect) -> bool {
    if r.contains(a) || r.contains(b) {
        return true;
    }
    let corners = [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)];
    let crosses = |p: Point, q: Point| {
        let d1 = (b - a).cross(p - a);
        let d2 = (b - a).cross(q - a);
        let d3 = (q - p).cross(a - p);
        let d4 = (q - p).cross(b - p);
        d1 * d2 < 0.0 && d3 * d4 < 0.0
    };
    (0..4).any(|i| crosses(corners[i], corners[(i + 1) % 4]))
}

/// Ids a marquee over `r` selects: shapes whose outline bounds touch it and connectors whose
/// route crosses it, shapes first.
pub fn marquee_ids(page: &Page, r: &dg::Rect) -> Vec<String> {
    let shapes = page.shapes.iter().filter(|s| s.bounds().intersects(r)).map(|s| s.id.clone());
    let connectors = page
        .connectors
        .iter()
        .filter(|c| dg::route_polyline(page, c).windows(2).any(|w| matches!(w, [a, b] if segment_hits_rect(*a, *b, r))))
        .map(|c| c.id.clone());
    shapes.chain(connectors).collect()
}

/// What a press at `w` would start, decided against the page before any command runs.
enum Start {
    Drag(Drag),
    /// Select `id` (adding with Shift), then move the selection.
    SelectAndMove(String, bool),
}

fn near(vp: &Viewport2D, a: Point, screen: Pos2) -> bool {
    vp.world_to_screen(pos(a)).distance(screen) <= GRAB
}

fn decide(app: &CraftApp, page: &Page, vp: &Viewport2D, at: Pos2, m: Modifiers) -> Option<Start> {
    let w = point(vp.screen_to_world(at));
    let zoom = vp.zoom;
    let tol = f64::from(GRAB / zoom);
    match app.tool {
        Tool::Text => None,
        Tool::Connector => Some(Start::Drag(Drag::Connect { from: target_end(page, w, zoom), to: w })),
        Tool::Pointer => {
            if let Some(s) = single_shape(app, page) {
                if near(vp, rotate_handle_world(s, zoom), at) {
                    return Some(Start::Drag(Drag::Rotate { id: s.id.clone(), to: w, snap: m.shift }));
                }
                if let Some(i) = (0..HANDLES.len()).find(|i| handle_world(s, *i).is_some_and(|h| near(vp, h, at))) {
                    return Some(Start::Drag(Drag::Resize { id: s.id.clone(), handle: i, to: w, keep_aspect: m.shift }));
                }
            }
            if let Some(c) = single_connector(app, page) {
                let ends = dg::resolve_ends(page, c);
                for (start, p) in [(false, ends.end.pos), (true, ends.start.pos)] {
                    if near(vp, p, at) {
                        return Some(Start::Drag(Drag::Reglue { id: c.id.clone(), start, to: w }));
                    }
                }
            }
            if let Some(s) = dg::shape_at(page, w, tol)
                && let Some(i) = dg::connection_point_at(s, w, tol)
            {
                return Some(Start::Drag(Drag::Connect { from: End::point(s.id.clone(), i), to: w }));
            }
            match dg::hit(page, w, f64::from(HIT / zoom)) {
                Some(Hit::Shape(id) | Hit::Connector(id)) => Some(Start::SelectAndMove(id, m.shift)),
                None => Some(Start::Drag(Drag::Marquee { from: w, to: w, add: m.shift })),
            }
        }
    }
}

/// Run a canvas command; failures land in the status bar through `CraftApp::run`.
fn run(app: &mut CraftApp, id: &str, params: Value) {
    let _ = app.run(id, params);
}

fn end_json(e: &End) -> Value {
    serde_json::to_value(e).unwrap_or(Value::Null)
}

/// Commit a finished drag as one command.
fn finish(app: &mut CraftApp, drag: Drag, zoom: f32) {
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

/// Start editing the text of `target`.
pub fn start_editing(app: &mut CraftApp, target: Hit) {
    let Some(page) = app.session.doc.page() else { return };
    let text = match &target {
        Hit::Shape(id) => page.shape(id).map(|s| s.text.clone()),
        Hit::Connector(id) => page.connector(id).map(|c| c.text.clone()),
    };
    let Some(text) = text else { return };
    app.canvas.drag = None;
    app.canvas.editing = Some(TextEditing { target, original: text.clone(), text, focus: true });
}

/// Finish in-place editing: run `shape.setText` / `connector.setText` when the text changed.
pub fn commit_editing(app: &mut CraftApp) {
    let Some(ed) = app.canvas.editing.take() else { return };
    if ed.text == ed.original {
        return;
    }
    let cmd = match ed.target {
        Hit::Shape(_) => "shape.setText",
        Hit::Connector(_) => "connector.setText",
    };
    run(app, cmd, json!({"id": ed.id(), "text": ed.text}));
}

/// Click (press and release without dragging) at `at`.
fn click(app: &mut CraftApp, vp: &Viewport2D, at: Pos2, m: Modifiers) {
    let Some(page) = app.session.doc.page() else { return };
    let w = point(vp.screen_to_world(at));
    let hit = dg::hit(page, w, f64::from(HIT / vp.zoom));
    if app.tool == Tool::Text {
        if let Some(h) = hit {
            start_editing(app, h);
        }
        // There is no text-only master in the catalog yet, so a click on empty space adds nothing.
        return;
    }
    let sel = app.session.sel.items.clone();
    match hit {
        Some(Hit::Shape(id) | Hit::Connector(id)) if m.shift => {
            if sel.contains(&id) {
                let rest: Vec<String> = sel.into_iter().filter(|i| *i != id).collect();
                if rest.is_empty() {
                    run(app, "edit.selectNone", json!({}));
                } else {
                    run(app, "edit.select", json!({"ids": rest}));
                }
            } else {
                run(app, "edit.select", json!({"ids": [id], "add": true}));
            }
        }
        Some(Hit::Shape(id) | Hit::Connector(id)) => {
            if sel != [id.clone()] {
                run(app, "edit.select", json!({"ids": [id]}));
            }
        }
        None => {
            if !m.shift && !sel.is_empty() {
                run(app, "edit.selectNone", json!({}));
            }
        }
    }
}

/// Whether a primary drag on the canvas is a pan gesture instead (Space or Cmd held).
fn pan_modifier(ui: &Ui) -> bool {
    ui.input(|i| i.modifiers.command || i.key_down(Key::Space))
}

/// Pointer interaction for this frame: hover, clicks, drags and their commits.
pub fn pointer(app: &mut CraftApp, ui: &Ui, response: &Response, vp: &Viewport2D) {
    let m = ui.input(|i| i.modifiers);
    let zoom = vp.zoom;
    // Hover: what is under the pointer, and the shape whose connection points to show.
    app.canvas.hover = None;
    app.canvas.hover_shape = None;
    if let (Some(hp), Some(page)) = (response.hover_pos(), app.session.doc.page()) {
        let w = point(vp.screen_to_world(hp));
        app.canvas.hover = dg::hit(page, w, f64::from(HIT / zoom));
        app.canvas.hover_shape = dg::shape_at(page, w, f64::from(GRAB / zoom)).map(|s| s.id.clone());
    }

    if response.drag_started_by(egui::PointerButton::Primary) && !pan_modifier(ui) {
        commit_editing(app);
        let origin = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos());
        let start = match (origin, app.session.doc.page()) {
            (Some(o), Some(page)) => decide(app, page, vp, o, m),
            _ => None,
        };
        let o = origin.map(|o| point(vp.screen_to_world(o))).unwrap_or(Point::ZERO);
        app.canvas.drag = match start {
            Some(Start::Drag(d)) => Some(d),
            Some(Start::SelectAndMove(id, add)) => {
                if !app.session.sel.items.contains(&id) {
                    run(app, "edit.select", json!({"ids": [id], "add": add}));
                }
                Some(Drag::Move { ids: app.session.sel.items.clone(), from: o, to: o })
            }
            None => None,
        };
    }
    if response.dragged_by(egui::PointerButton::Primary)
        && let Some(p) = response.interact_pointer_pos()
    {
        let w = point(vp.screen_to_world(p));
        match &mut app.canvas.drag {
            Some(Drag::Marquee { to, .. } | Drag::Move { to, .. } | Drag::Connect { to, .. } | Drag::Reglue { to, .. }) => *to = w,
            Some(Drag::Resize { to, keep_aspect, .. }) => {
                *to = w;
                *keep_aspect = m.shift;
            }
            Some(Drag::Rotate { to, snap, .. }) => {
                *to = w;
                *snap = m.shift;
            }
            None => {}
        }
    }
    if response.drag_stopped()
        && let Some(d) = app.canvas.drag.take()
    {
        finish(app, d, zoom);
    }
    if response.double_clicked_by(egui::PointerButton::Primary) && app.tool == Tool::Pointer {
        if let (Some(p), Some(page)) = (response.interact_pointer_pos(), app.session.doc.page())
            && let Some(h) = dg::hit(page, point(vp.screen_to_world(p)), f64::from(HIT / zoom))
        {
            start_editing(app, h);
        }
    } else if response.clicked_by(egui::PointerButton::Primary)
        && let Some(p) = response.interact_pointer_pos()
    {
        commit_editing(app);
        click(app, vp, p, m);
    }
    cursor(app, ui, response, vp);
}

fn cursor(app: &CraftApp, ui: &Ui, response: &Response, vp: &Viewport2D) {
    if !response.hovered() || pan_modifier(ui) {
        return;
    }
    let icon = match (&app.canvas.drag, app.tool) {
        (Some(Drag::Move { .. }), _) => egui::CursorIcon::Grabbing,
        (Some(Drag::Connect { .. } | Drag::Reglue { .. }), _) => egui::CursorIcon::Crosshair,
        (Some(_), _) => return,
        (None, Tool::Connector) => egui::CursorIcon::Crosshair,
        (None, Tool::Text) => egui::CursorIcon::Text,
        (None, Tool::Pointer) => {
            let (Some(hp), Some(page)) = (response.hover_pos(), app.session.doc.page()) else { return };
            let w = point(vp.screen_to_world(hp));
            let tol = f64::from(GRAB / vp.zoom);
            if let Some(s) = single_shape(app, page) {
                if near(vp, rotate_handle_world(s, vp.zoom), hp) {
                    egui::CursorIcon::Alias
                } else if let Some(i) = (0..HANDLES.len()).find(|i| handle_world(s, *i).is_some_and(|h| near(vp, h, hp))) {
                    if i % 4 == 1 {
                        egui::CursorIcon::ResizeVertical
                    } else if i % 4 == 3 {
                        egui::CursorIcon::ResizeHorizontal
                    } else if i % 4 == 0 {
                        egui::CursorIcon::ResizeNwSe
                    } else {
                        egui::CursorIcon::ResizeNeSw
                    }
                } else if dg::shape_at(page, w, tol).is_some_and(|s| dg::connection_point_at(s, w, tol).is_some()) {
                    egui::CursorIcon::Crosshair
                } else if app.canvas.hover.is_some() {
                    egui::CursorIcon::Move
                } else {
                    return;
                }
            } else if dg::shape_at(page, w, tol).is_some_and(|s| dg::connection_point_at(s, w, tol).is_some()) {
                egui::CursorIcon::Crosshair
            } else if app.canvas.hover.is_some() {
                egui::CursorIcon::Move
            } else {
                return;
            }
        }
    };
    ui.ctx().set_cursor_icon(icon);
}

/// Canvas keys: Escape cancels a drag or clears the selection; arrows nudge the selection by one
/// unit (ten with Shift) through `shape.move`. Nothing fires while a dialog, a menu or a text
/// field has the keyboard.
pub fn keys(app: &mut CraftApp, ctx: &egui::Context) {
    if app.dialog.is_some() || menus::is_open(ctx) || ctx.egui_wants_keyboard_input() || app.canvas.editing.is_some() || app.backstage_open() {
        return;
    }
    let presses: Vec<(Key, Modifiers)> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
            .collect()
    });
    let (mut dx, mut dy) = (0.0, 0.0);
    for (key, m) in presses {
        if m.command || m.alt || m.ctrl {
            continue;
        }
        let step = if m.shift { 10.0 } else { 1.0 };
        match key {
            Key::Escape => {
                ctx.input_mut(|i| i.consume_key(m, key));
                if app.canvas.drag.take().is_none() && !app.session.sel.items.is_empty() {
                    run(app, "edit.selectNone", json!({}));
                }
            }
            Key::ArrowLeft => dx -= step,
            Key::ArrowRight => dx += step,
            Key::ArrowUp => dy -= step,
            Key::ArrowDown => dy += step,
            _ => continue,
        }
        if matches!(key, Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown) {
            ctx.input_mut(|i| i.consume_key(m, key));
        }
    }
    if (dx != 0.0 || dy != 0.0) && !app.session.sel.items.is_empty() {
        run(app, "shape.move", json!({"dx": dx, "dy": dy}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn marquee_touches_shapes_and_crossing_connectors() {
        let doc = vizcraft_engine::sample::sample_document();
        let page = doc.page().unwrap();
        let first = &page.shapes[0];
        let r = first.bounds().inflate(-5.0);
        let ids = marquee_ids(page, &r);
        assert_eq!(ids, vec![first.id.clone()]);
        let all = marquee_ids(page, &page.rect());
        assert_eq!(all.len(), page.shapes.len() + page.connectors.len());
        assert!(marquee_ids(page, &dg::Rect::from_xywh(5.0, 5.0, 10.0, 10.0)).is_empty());
    }

    #[test]
    fn connect_refuses_a_drop_on_its_own_shape() {
        assert!(!connect_ok(&End::point("s1", 0), &End::dynamic("s1")));
        assert!(!connect_ok(&End::point("s1", 0), &End::point("s1", 0)));
        assert!(connect_ok(&End::point("s1", 0), &End::point("s1", 2)));
        assert!(connect_ok(&End::point("s1", 0), &End::dynamic("s2")));
        assert!(connect_ok(&End::free(Point::ZERO), &End::free(Point::ZERO)));
    }
}
