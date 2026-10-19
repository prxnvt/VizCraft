//! Insertion on the canvas: four small arrows outside the sides of the hovered shape (after
//! [`HANDLE_DELAY`]), the magic connector dragged from one of them, the popover that lists the
//! choices when there are several, and the insert picker a double-click on empty canvas opens.
//!
//! Everything ends in an engine command: a handle click runs `insert.next`, a drop on another
//! shape `insert.connect`, a drop on empty canvas `insert.next` at the drop point. What may come
//! next is the engine's `suggest::next` and `suggest::connect`; this module only reads them.
//! The state lives in [`Insertion`] on the app, never in the document.

use egui::{Align2, Pos2, Response, Sense, Ui, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::{self as dg, Point};
use vizcraft_engine::suggest::{self, Direction, RelationshipChoice, Suggestion};

use super::super::Viewport2D;
use super::super::draw::{self, point};
use crate::theme::{Tokens, regular};
use crate::{CraftApp, Tool};

/// Seconds the pointer rests on a shape before its handles show.
pub const HANDLE_DELAY: f64 = 0.3;
/// How far outside a side a handle's centre sits, in screen points: clear of the rotate handle
/// (22 points above the frame) and of the resize handles on the frame.
pub const HANDLE_GAP: f32 = 38.0;
/// Radius within which a handle is hit, in screen points.
pub const HANDLE_RADIUS: f32 = 9.0;
/// Most rows a popover lists.
const MAX_ROWS: usize = 24;

/// A handle drag in progress: the magic connector from `source` leaving towards `dir`.
#[derive(Clone, Debug, PartialEq)]
pub struct HandleDrag {
    /// The shape the drag started from.
    pub source: String,
    /// The handle it started on.
    pub dir: Direction,
    /// The pointer, in world coordinates.
    pub to: Point,
}

/// The popover listing choices next to the canvas.
#[derive(Clone, Debug, PartialEq)]
pub enum Popover {
    /// What may follow `source`: a click on a row runs `insert.next`, at `drop` when the magic
    /// connector was dropped on empty canvas.
    Next {
        /// The shape the step follows.
        source: String,
        /// Which way the step goes.
        dir: Direction,
        /// Where the step was dropped, if it was.
        drop: Option<Point>,
        /// Where the popover sits, in world coordinates.
        at: Point,
        /// The suggestions, best first.
        items: Vec<Suggestion>,
    },
    /// Which relationship joins `from` to `to`: a click on a row runs `insert.connect`.
    Connect {
        /// The source shape.
        from: String,
        /// The target shape.
        to: String,
        /// Where the popover sits, in world coordinates.
        at: Point,
        /// The choices, legal first.
        items: Vec<RelationshipChoice>,
    },
}

/// Insertion state for one window.
#[derive(Default)]
pub struct Insertion {
    /// The shape under the pointer and when the pointer arrived on it (input time, seconds).
    hover: Option<(String, f64)>,
    /// The shape whose handles show.
    pub shown: Option<String>,
    /// The magic connector being dragged.
    pub drag: Option<HandleDrag>,
    /// The open popover.
    pub popover: Option<Popover>,
    /// The open insert picker.
    pub picker: Option<crate::insert_picker::Picker>,
}

/// A side's handle centre for a shape whose screen frame is `r`.
pub fn handle_centre(r: egui::Rect, dir: Direction) -> Pos2 {
    match dir {
        Direction::Up => Pos2::new(r.center().x, r.top() - HANDLE_GAP),
        Direction::Right => Pos2::new(r.right() + HANDLE_GAP, r.center().y),
        Direction::Down => Pos2::new(r.center().x, r.bottom() + HANDLE_GAP),
        Direction::Left => Pos2::new(r.left() - HANDLE_GAP, r.center().y),
    }
}

/// The screen frame of shape `id` on the active page.
pub fn screen_frame(app: &CraftApp, vp: &Viewport2D, id: &str) -> Option<egui::Rect> {
    let s = app.session.doc.page()?.shape(id)?;
    let b = s.bounds();
    Some(egui::Rect::from_two_pos(vp.world_to_screen(draw::pos(Point::new(b.x0, b.y0))), vp.world_to_screen(draw::pos(Point::new(b.x1, b.y1)))))
}

/// The handle of the shown shape under screen point `p`.
pub fn handle_at(app: &CraftApp, vp: &Viewport2D, p: Pos2) -> Option<(String, Direction)> {
    let id = app.insertion.shown.as_ref()?;
    let r = screen_frame(app, vp, id)?;
    Direction::ALL.into_iter().find(|d| handle_centre(r, *d).distance(p) <= HANDLE_RADIUS).map(|d| (id.clone(), d))
}

/// Whether handles may show at all right now.
fn handles_allowed(app: &CraftApp) -> bool {
    matches!(app.tool, Tool::Pointer)
        && app.session.view.aids.auto_connect
        && app.canvas.drag.is_none()
        && app.canvas.editing.is_none()
        && app.dialog.is_none()
        && app.insertion.popover.is_none()
        && app.insertion.picker.is_none()
}

/// Shapes that get handles: anything but containers, lane sets and callouts, which frame or
/// point at other shapes rather than take part in a flow, and the parts of a sequence diagram,
/// which grow through the Sequence tab instead.
fn takes_handles(page: &dg::Page, id: &str) -> bool {
    page.shape(id).is_some_and(|s| !dg::container::is_container(s) && !dg::container::is_callout(s) && s.sequence.is_none())
}

/// Track which shape the pointer rests on and show its handles once it has rested long enough.
fn track_hover(app: &mut CraftApp, ui: &Ui, vp: &Viewport2D, hp: Option<Pos2>) {
    if !handles_allowed(app) {
        app.insertion.hover = None;
        app.insertion.shown = None;
        return;
    }
    // On a handle of the shown shape: keep it, whatever is under the pointer.
    if hp.is_some_and(|p| handle_at(app, vp, p).is_some()) {
        return;
    }
    let now = ui.input(|i| i.time);
    let under = match (&app.canvas.hover, app.session.doc.page()) {
        (Some(dg::Hit::Shape(id)), Some(page)) if takes_handles(page, id) => Some(id.clone()),
        _ => None,
    };
    let Some(id) = under else {
        app.insertion.hover = None;
        app.insertion.shown = None;
        return;
    };
    match &app.insertion.hover {
        Some((h, since)) if *h == id => {
            let rested = now - since;
            if rested >= HANDLE_DELAY {
                app.insertion.shown = Some(id);
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((HANDLE_DELAY - rested).max(0.0)));
            }
        }
        _ => {
            app.insertion.hover = Some((id, now));
            app.insertion.shown = None;
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(HANDLE_DELAY));
        }
    }
}

/// Run a command; failures land in the status bar through `CraftApp::run`.
fn run(app: &mut CraftApp, id: &str, params: Value) {
    let _ = app.run(id, params);
}

/// `insert.next` for suggestion `s` from `source`, at `drop` when given.
fn insert_next(app: &mut CraftApp, source: &str, dir: Direction, s: &Suggestion, drop: Option<Point>) {
    let mut p = json!({"id": source, "direction": dir.name(), "type": s.type_id, "relationship": s.relationship});
    if let (Some(d), Some(o)) = (drop, p.as_object_mut()) {
        o.insert("x".into(), json!(d.x));
        o.insert("y".into(), json!(d.y));
    }
    run(app, "insert.next", p);
}

/// A click on handle `dir` of `source`: the one suggestion runs at once, several open the popover.
pub fn click_handle(app: &mut CraftApp, vp: &Viewport2D, source: &str, dir: Direction) {
    let items = suggest::next(&app.session.doc, source);
    match items.as_slice() {
        [] => app.status("Nothing can follow this shape".to_string()),
        [one] => {
            let one = one.clone();
            insert_next(app, source, dir, &one, None);
        }
        _ => {
            let at = screen_frame(app, vp, source).map(|r| point(vp.screen_to_world(handle_centre(r, dir)))).unwrap_or(Point::ZERO);
            app.insertion.popover = Some(Popover::Next { source: source.to_string(), dir, drop: None, at, items });
        }
    }
}

/// The direction from the centre of `source` to `to`: the axis it leaves along most.
fn direction_to(page: &dg::Page, source: &str, to: Point) -> Direction {
    let c = page.shape(source).map(|s| s.bounds().center()).unwrap_or(to);
    let (dx, dy) = (to.x - c.x, to.y - c.y);
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 { Direction::Right } else { Direction::Left }
    } else if dy >= 0.0 {
        Direction::Down
    } else {
        Direction::Up
    }
}

/// The shape a magic connector dropped at `w` lands on: the topmost one there that is not the
/// source and takes part in flows.
pub fn drop_target(page: &dg::Page, source: &str, w: Point, tol: f64) -> Option<String> {
    let s = dg::shape_at(page, w, tol)?;
    (s.id != source && takes_handles(page, &s.id)).then(|| s.id.clone())
}

/// The magic connector released at `w`: on another shape it connects (one legal choice at once,
/// else the popover), on empty canvas it drops a new step there (one suggestion at once, else the
/// popover). Released on the source itself it does nothing.
pub fn finish_drag(app: &mut CraftApp, d: HandleDrag, zoom: f32) {
    let Some(page) = app.session.doc.page() else { return };
    let tol = f64::from(super::GRAB / zoom.max(0.01));
    if let Some(target) = drop_target(page, &d.source, d.to, tol) {
        let items = suggest::connect(&app.session.doc, &d.source, &target);
        let legal: Vec<&RelationshipChoice> = items.iter().filter(|c| c.legal).collect();
        if let [one] = legal.as_slice() {
            let rel = if one.id.is_empty() { Value::Null } else { json!(one.id) };
            run(app, "insert.connect", json!({"from": d.source, "to": target, "relationship": rel}));
        } else if !items.is_empty() {
            app.insertion.popover = Some(Popover::Connect { from: d.source, to: target, at: d.to, items });
        }
        return;
    }
    if page.shape(&d.source).is_some_and(|s| s.bounds().contains(d.to)) {
        return;
    }
    let dir = direction_to(page, &d.source, d.to);
    let items = suggest::next(&app.session.doc, &d.source);
    match items.as_slice() {
        [] => app.status("Nothing can follow this shape".to_string()),
        [one] => {
            let one = one.clone();
            insert_next(app, &d.source, dir, &one, Some(d.to));
        }
        _ => app.insertion.popover = Some(Popover::Next { source: d.source, dir, drop: Some(d.to), at: d.to, items }),
    }
}

/// Insertion's share of this frame's pointer handling, run after the hover is known and before
/// the canvas's own clicks and drags. Returns true when it took the pointer this frame (a handle
/// click or drag, a double-click on empty canvas), so the canvas does nothing more with it.
pub fn pointer(app: &mut CraftApp, ui: &Ui, response: &Response, vp: &Viewport2D) -> bool {
    let ctx = ui.ctx().clone();
    popover(app, &ctx, vp);
    crate::insert_picker::show(app, &ctx, vp);
    let hp = response.hover_pos();

    // A magic connector in progress owns the pointer until it is released.
    if app.insertion.drag.is_some() {
        if let Some(p) = response.interact_pointer_pos().or(hp)
            && let Some(d) = app.insertion.drag.as_mut()
        {
            d.to = point(vp.screen_to_world(p));
        }
        let released = response.drag_stopped() || !ui.input(|i| i.pointer.primary_down());
        if released && let Some(d) = app.insertion.drag.take() {
            finish_drag(app, d, vp.zoom);
        }
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        draw::insertion::paint(app, &ctx, vp);
        return true;
    }

    // The drag check comes before the hover update: the pointer may already have left the
    // handle by the frame the drag is recognised, which would hide the handles first.
    let mut took = false;
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        && let Some((source, dir)) = handle_at(app, vp, origin)
    {
        let to = point(vp.screen_to_world(response.interact_pointer_pos().unwrap_or(origin)));
        app.insertion.drag = Some(HandleDrag { source, dir, to });
        app.insertion.shown = None;
        took = true;
    } else if response.clicked_by(egui::PointerButton::Primary)
        && let Some((source, dir)) = response.interact_pointer_pos().and_then(|p| handle_at(app, vp, p))
    {
        click_handle(app, vp, &source, dir);
        app.insertion.shown = None;
        app.insertion.hover = None;
        took = true;
    } else if response.double_clicked_by(egui::PointerButton::Primary)
        && matches!(app.tool, Tool::Pointer | Tool::Lasso)
        && let (Some(p), Some(page)) = (response.interact_pointer_pos(), app.session.doc.page())
        && dg::hit(page, point(vp.screen_to_world(p)), f64::from(super::HIT / vp.zoom)).is_none()
    {
        crate::insert_picker::open(app, point(vp.screen_to_world(p)), "");
        took = true;
    }
    if !took {
        track_hover(app, ui, vp, hp);
    }
    if !took && hp.is_some_and(|p| handle_at(app, vp, p).is_some()) {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    draw::insertion::paint(app, &ctx, vp);
    took
}

/// One clickable popover row: a type icon, the label, a dimmer second label; reports its label
/// to the accessibility tree so tests and screen readers find it.
fn row(ui: &mut Ui, t: &Tokens, k: usize, master: Option<&str>, label: &str, detail: &str, dim: bool) -> bool {
    // An explicit id per row, so a click lands on the row it was aimed at even when the list
    // changes between frames.
    let (rect, _) = ui.allocate_exact_size(vec2(240.0, 26.0), Sense::hover());
    let resp = ui.interact(rect, egui::Id::new(("insertion_row", k)), Sense::click());
    // The relationship joins the label for screen readers and tests (`Decision (Flow)`), so a row
    // is told apart from the same type in the Shapes pane.
    let spoken = if detail.is_empty() { label.to_string() } else { format!("{label} ({detail})") };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &spoken));
    if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, t.hover);
    }
    let icon = egui::Rect::from_center_size(rect.left_center() + vec2(16.0, 0.0), vec2(18.0, 14.0));
    if let Some(m) = master.and_then(vizcraft_stencils::master) {
        crate::icons::paint_kind(ui.painter(), icon, m.kind, m.w as f32, m.h as f32, t.icon, t.accent);
    }
    let color = if dim { t.text_dim } else { t.text };
    let g = ui.painter().text(rect.left_center() + vec2(32.0, 0.0), Align2::LEFT_CENTER, label, regular(12.5), color);
    if !detail.is_empty() {
        ui.painter().text(g.right_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, detail, regular(11.0), t.text_dim);
    }
    resp.clicked()
}

/// The label of relationship type `r` for a popover row.
fn relationship_label(r: Option<&str>) -> String {
    r.and_then(|r| vizcraft_notation::catalog().relationship_type(r).map(|(_, t)| t.label.clone())).unwrap_or_default()
}

/// Show the open popover; a click on a row runs its command, Escape or a press outside closes it.
fn popover(app: &mut CraftApp, ctx: &egui::Context, vp: &Viewport2D) {
    let Some(pop) = app.insertion.popover.clone() else { return };
    let t = Tokens::get(ctx);
    let at = match &pop {
        Popover::Next { at, .. } | Popover::Connect { at, .. } => vp.world_to_screen(draw::pos(*at)) + vec2(12.0, 12.0),
    };
    let frame = egui::Frame::NONE.fill(t.menu).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(6.0).inner_margin(egui::Margin::same(4));
    let mut chosen: Option<usize> = None;
    let area = egui::Area::new(egui::Id::new("insertion_popover")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        frame.show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
            match &pop {
                Popover::Next { items, .. } => {
                    for (k, s) in items.iter().take(MAX_ROWS).enumerate() {
                        let master = vizcraft_notation::catalog().master_for_type(&s.type_id).or(Some(s.type_id.as_str()));
                        if row(ui, &t, k, master, &s.label, &relationship_label(s.relationship.as_deref()), false) {
                            chosen = Some(k);
                        }
                    }
                }
                Popover::Connect { items, .. } => {
                    for (k, c) in items.iter().take(MAX_ROWS).enumerate() {
                        let detail = if c.legal { "" } else { "not allowed" };
                        if row(ui, &t, k, None, &c.label, detail, !c.legal) {
                            chosen = Some(k);
                        }
                    }
                }
            }
        });
    });
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p)));
    if let Some(k) = chosen {
        app.insertion.popover = None;
        match pop {
            Popover::Next { source, dir, drop, items, .. } => {
                if let Some(s) = items.get(k) {
                    insert_next(app, &source, dir, s, drop);
                }
            }
            Popover::Connect { from, to, items, .. } => {
                if let Some(c) = items.get(k) {
                    let rel = if c.id.is_empty() { Value::Null } else { json!(c.id) };
                    run(app, "insert.connect", json!({"from": from, "to": to, "relationship": rel}));
                }
            }
        }
    } else if escape || outside {
        if escape {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
        app.insertion.popover = None;
    }
}

/// A compact summary for `ui.inspect`-style checks and tests.
pub fn summary(app: &CraftApp) -> Value {
    json!({
        "handles": app.insertion.shown,
        "dragging": app.insertion.drag.as_ref().map(|d| d.source.clone()),
        "popover": app.insertion.popover.as_ref().map(|p| match p {
            Popover::Next { items, .. } => json!({"kind": "next", "items": items.iter().map(|s| s.type_id.clone()).collect::<Vec<_>>()}),
            Popover::Connect { items, .. } => json!({"kind": "connect", "items": items.iter().map(|c| c.id.clone()).collect::<Vec<_>>()}),
        }),
        "picker": app.insertion.picker.is_some(),
    })
}
