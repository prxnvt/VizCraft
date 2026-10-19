//! Sequence pages on the canvas: lifelines are columns, messages are rows, and settle owns every
//! frame on the page, so the canvas edits the event order instead of geometry.
//!
//! - A press on a message, note or fragment drags it to another row ([`SeqDrag::Reorder`], which
//!   commits `sequence.move`); a press on a lifeline drags its column sideways
//!   ([`SeqDrag::Column`], `shape.move` with `dy` 0); a press on an operand separator drags it
//!   ([`SeqDrag::Separator`], one `edit.batch` of `sequence.removeOperand` and
//!   `sequence.addOperand`).
//! - With the Connector tool a drag from stem to stem draws a message at the row under the pointer
//!   ([`SeqDrag::Message`], `sequence.addMessage`), then edits its label.
//! - Dropping a lifeline or note master runs `sequence.addLifeline` or `sequence.addNote`
//!   ([`insert_master`]); a double-click on a fragment edits the guard of the operand under the
//!   pointer ([`double_click`], [`guard_editor`]).
//!
//! The preview of a reorder, column or separator drag is the engine's own settle pass run on a copy
//! of the page with the change applied, so what is drawn while dragging is what the release gives.
//! Every gesture ends in one command, so it is one undo step.

use egui::{Modifiers, Painter, Pos2, Stroke, Ui, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::sequence::{self as seq, Event, Part, Position, Sort};
use vizcraft_diagram::{self as dg, Hit, Page, Point, Shape, ShapeKind};

use super::tools::Start;
use super::{Drag, GRAB, HANDLES, HIT, handle_world, rotate_handle_world, run, single_shape};
use crate::canvas::Viewport2D;
use crate::canvas::draw::{point, pos};
use crate::theme::Tokens;
use crate::{CraftApp, Tool};

/// Pointer travel, in world units, below which a released drag is a click and runs nothing.
const MIN_TRAVEL: f64 = 3.0;

/// A drag on a Sequence page, in world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum SeqDrag {
    /// Moving message, note or fragment `id` to the row under the pointer.
    Reorder {
        /// The block dragged.
        id: String,
        /// Where the press was.
        from: Point,
        /// Where the pointer is.
        to: Point,
    },
    /// Moving lifeline `id`'s column sideways.
    Column {
        /// The lifeline.
        id: String,
        /// Where the press was.
        from: Point,
        /// Where the pointer is.
        to: Point,
    },
    /// Drawing a message from lifeline `from` at row `y` (the Connector tool).
    Message {
        /// The sending lifeline.
        from: String,
        /// The row it leaves at.
        y: f64,
        /// Where the pointer is.
        to: Point,
    },
    /// Moving the separator that starts operand `operand` of `fragment`.
    Separator {
        /// The fragment.
        fragment: String,
        /// The operand the separator starts (1 or more).
        operand: usize,
        /// Where the press was.
        from: Point,
        /// Where the pointer is.
        to: Point,
    },
}

impl SeqDrag {
    /// Follow the pointer to world point `w`.
    pub fn set_to(&mut self, w: Point) {
        match self {
            SeqDrag::Reorder { to, .. } | SeqDrag::Column { to, .. } | SeqDrag::Message { to, .. } | SeqDrag::Separator { to, .. } => *to = w,
        }
    }

    /// The name `ui.inspect` reports under `canvas.dragging`.
    pub fn name(&self) -> &'static str {
        match self {
            SeqDrag::Reorder { .. } => "reorder",
            SeqDrag::Column { .. } => "column",
            SeqDrag::Message { .. } => "message",
            SeqDrag::Separator { .. } => "separator",
        }
    }
}

/// The guard being edited in place.
#[derive(Clone, Debug, PartialEq)]
pub struct GuardEdit {
    /// The fragment.
    pub fragment: String,
    /// The operand whose guard is edited.
    pub operand: usize,
    /// The text so far, without brackets.
    pub text: String,
    /// The editor's top-left corner, in world coordinates.
    pub at: Point,
    /// Focus the field on the next frame.
    focus: bool,
}

/// Canvas state of Sequence pages: the guard editor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeqState {
    /// The guard being edited, if any.
    pub guard: Option<GuardEdit>,
}

fn on_sequence(page: &Page) -> bool {
    page.sequence.is_some()
}

fn is_lifeline(s: &Shape) -> bool {
    matches!(s.sequence, Some(Part::Lifeline { .. }))
}

/// Whether settle owns `s`'s frame (a lifeline, fragment, activation bar or destruction mark): it
/// gets no resize or rotate handles. Notes keep theirs, since their size feeds the layout.
pub fn frame_fixed(s: &Shape) -> bool {
    matches!(s.sequence, Some(Part::Lifeline { .. } | Part::Fragment { .. } | Part::Activation { .. } | Part::Destruction { .. }))
}

/// The page y of each operand separator of fragment `f`, with the operand it starts.
fn separators(f: &Shape) -> Vec<(usize, f64)> {
    match &f.sequence {
        Some(Part::Fragment { operands, .. }) => operands.iter().enumerate().skip(1).map(|(k, o)| (k, f.y + o.at)).collect(),
        _ => Vec::new(),
    }
}

/// What a press on a Sequence page starts before the ordinary tools are asked: the Connector tool
/// on a lifeline draws a message, the pointer on an operand separator drags it, and the pointer
/// on a hidden handle of a shape settle places moves it instead of resizing it. `None` lets the
/// ordinary tools decide.
pub(super) fn decide(app: &CraftApp, page: &Page, vp: &Viewport2D, at: Pos2, m: Modifiers) -> Option<Start> {
    if !on_sequence(page) {
        return None;
    }
    let w = point(vp.screen_to_world(at));
    let tol = f64::from(GRAB / vp.zoom);
    match app.tool {
        Tool::Connector => seq::lifeline_at(page, w, tol).map(|from| Start::Drag(Drag::Sequence(SeqDrag::Message { from, y: w.y, to: w }))),
        Tool::Pointer | Tool::Lasso => {
            for f in page.shapes.iter().rev().filter(|s| matches!(s.sequence, Some(Part::Fragment { .. }))) {
                if w.x < f.x - tol || w.x > f.x + f.w + tol {
                    continue;
                }
                if let Some((k, _)) = separators(f).into_iter().find(|(_, y)| (w.y - y).abs() <= tol) {
                    return Some(Start::Drag(Drag::Sequence(SeqDrag::Separator { fragment: f.id.clone(), operand: k, from: w, to: w })));
                }
            }
            let on_hidden_handle = single_shape(app, page).filter(|s| frame_fixed(s)).is_some_and(|s| {
                let near = |h: Point| vp.world_to_screen(pos(h)).distance(at) <= GRAB;
                near(rotate_handle_world(s, vp.zoom)) || (0..HANDLES.len()).any(|i| handle_world(s, i).is_some_and(near))
            });
            if !on_hidden_handle {
                return None;
            }
            Some(match dg::hit(page, w, f64::from(HIT / vp.zoom)) {
                Some(Hit::Shape(id)) => Start::SelectAndMove(super::select::press_target(page, &id, &app.session.sel.items), m.shift),
                Some(Hit::Connector(id)) => Start::SelectAndMove(id, m.shift),
                None => Start::Drag(Drag::Marquee { from: w, to: w, add: m.shift }),
            })
        }
        _ => None,
    }
}

/// The drag a press-and-move on the selection starts on a Sequence page: one selected message,
/// note or fragment reorders, one selected lifeline moves its column. `None` for anything else
/// (an ordinary move).
pub fn move_drag(app: &CraftApp, o: Point) -> Option<Drag> {
    let page = app.session.doc.page().filter(|p| on_sequence(p))?;
    let [id] = app.session.sel.items.as_slice() else { return None };
    let reorder = || Some(Drag::Sequence(SeqDrag::Reorder { id: id.clone(), from: o, to: o }));
    if let Some(c) = page.connector(id) {
        return if c.message.is_some() { reorder() } else { None };
    }
    match page.shape(id)?.sequence {
        Some(Part::Lifeline { .. }) => Some(Drag::Sequence(SeqDrag::Column { id: id.clone(), from: o, to: o })),
        Some(Part::Note { .. } | Part::Fragment { .. }) => reorder(),
        _ => None,
    }
}

/// Move the separator starting operand `operand` of `fragment` so `target` starts it, on `page`'s
/// events and operands (what the committed `edit.batch` does).
fn move_separator(page: &mut Page, fragment: &str, operand: usize, target: &str) -> Result<(), String> {
    let events = &mut page.sequence.as_mut().ok_or("the page has no interaction")?.events;
    seq::remove_operand(events, fragment, operand)?;
    let k = seq::add_operand(events, fragment, target)?;
    if let Some(Part::Fragment { operands, .. }) = page.shape_mut(fragment).and_then(|s| s.sequence.as_mut())
        && operand < operands.len()
    {
        let o = operands.remove(operand);
        operands.insert(k.min(operands.len()), o);
    }
    Ok(())
}

/// The block that starts operand `operand` of `fragment` now.
fn operand_start(page: &Page, fragment: &str, operand: usize) -> Option<String> {
    let events = &page.sequence.as_ref()?.events;
    let sep = events.iter().enumerate().filter(|(_, e)| matches!(e, Event::Else(f) if f == fragment)).nth(operand.checked_sub(1)?)?.0;
    match events.iter().skip(sep + 1).find(|e| !matches!(e, Event::Activate(_) | Event::Deactivate(_)))? {
        Event::Message(x) | Event::Note(x) | Event::Open(x) => Some(x.clone()),
        _ => None,
    }
}

/// The page a sequence drag draws from: the change applied to a copy and settled, so the preview
/// is what the release gives. `None` draws the page as it is (a message drag, or a drop that
/// changes nothing).
pub fn preview(page: &Page, d: &SeqDrag) -> Option<Page> {
    let mut p = page.clone();
    match d {
        SeqDrag::Reorder { id, from, to } => {
            if from.distance(*to) < MIN_TRAVEL {
                return None;
            }
            let (target, _) = seq::drop_position(page, to.y, Some(id))?;
            seq::move_block(&mut p.sequence.as_mut()?.events, id, &target).ok()?;
        }
        SeqDrag::Column { id, from, to } => {
            let s = p.shape_mut(id)?;
            s.x += to.x - from.x;
        }
        SeqDrag::Message { .. } => return None,
        SeqDrag::Separator { fragment, operand, to, .. } => {
            let target = seq::separator_target(page, fragment, *operand, to.y)?;
            if operand_start(page, fragment, *operand).as_deref() == Some(target.as_str()) {
                return None;
            }
            move_separator(&mut p, fragment, *operand, &target).ok()?;
        }
    }
    seq::settle_page(&mut p, true);
    Some(p)
}

/// A position as `sequence.*` params: `before`, `after` or `in`.
fn position_json(p: &Position) -> Value {
    match p {
        Position::Before(id) => json!({"before": id}),
        Position::After(id) => json!({"after": id}),
        Position::In { fragment, operand } => json!({"in": {"fragment": fragment, "operand": operand}}),
        Position::End => json!({}),
    }
}

/// Merge the keys of object `extra` into object `v`.
fn merged(mut v: Value, extra: Value) -> Value {
    if let (Some(o), Value::Object(e)) = (v.as_object_mut(), extra) {
        o.extend(e);
    }
    v
}

/// Commit a finished sequence drag as one command.
pub fn finish(app: &mut CraftApp, d: SeqDrag, zoom: f32) {
    let Some(page) = app.session.doc.page() else { return };
    match d {
        SeqDrag::Reorder { id, from, to } => {
            if from.distance(to) < MIN_TRAVEL {
                return;
            }
            let Some((target, _)) = seq::drop_position(page, to.y, Some(&id)) else { return };
            let mut events = page.sequence.as_ref().map(|q| q.events.clone()).unwrap_or_default();
            let before = events.clone();
            // A drop on the block itself, or one that leaves the order as it was, runs nothing.
            if target == Position::End || seq::move_block(&mut events, &id, &target).is_err() || events == before {
                return;
            }
            run(app, "sequence.move", merged(json!({"id": id}), position_json(&target)));
        }
        SeqDrag::Column { id, from, to } => {
            let dx = (to.x - from.x).round();
            if dx != 0.0 {
                run(app, "shape.move", json!({"ids": [id], "dx": dx, "dy": 0}));
            }
        }
        SeqDrag::Message { from, y, to } => {
            let Some(target) = seq::lifeline_at(page, to, f64::from(GRAB / zoom.max(0.01))) else {
                app.status("Release a message on a lifeline");
                return;
            };
            let sort = Sort::from_connector_type(&app.connector_kind).unwrap_or_default();
            let mut params = json!({"from": from, "to": target, "sort": sort.name()});
            if let Some((target, _)) = seq::drop_position(page, y, None) {
                params = merged(params, position_json(&target));
            }
            if let Ok(r) = app.run("sequence.addMessage", params)
                && let Some(id) = r.get("id").and_then(Value::as_str)
            {
                let id = id.to_string();
                run(app, "edit.select", json!({"ids": [id]}));
                super::start_editing(app, Hit::Connector(id));
            }
        }
        SeqDrag::Separator { fragment, operand, from, to } => {
            if (to.y - from.y).abs() < MIN_TRAVEL {
                return;
            }
            let Some(target) = seq::separator_target(page, &fragment, operand, to.y) else { return };
            if operand_start(page, &fragment, operand).as_deref() == Some(target.as_str()) {
                return;
            }
            let guard = match page.shape(&fragment).and_then(|s| s.sequence.as_ref()) {
                Some(Part::Fragment { operands, .. }) => operands.get(operand).map(|o| o.guard.clone()).unwrap_or_default(),
                _ => String::new(),
            };
            run(
                app,
                "edit.batch",
                json!({"label": "Move Operand Separator", "commands": [
                    {"command": "sequence.removeOperand", "params": {"id": fragment, "operand": operand}},
                    {"command": "sequence.addOperand", "params": {"id": fragment, "before": target, "guard": guard}},
                ]}),
            );
        }
    }
}

/// Head box of lifeline `s`, in world coordinates.
fn head_rect(s: &Shape) -> dg::Rect {
    let head = if s.kind == ShapeKind::ActorLifeline { seq::ACTOR_HEAD } else { seq::HEAD }.min(s.h);
    dg::Rect::new(s.x, s.y, s.x + s.w, s.y + head)
}

/// A lifeline's outline: its head box and a line along its stem.
fn lifeline_outline(painter: &Painter, vp: &Viewport2D, s: &Shape, st: Stroke) {
    let h = head_rect(s);
    let r = egui::Rect::from_two_pos(vp.world_to_screen(pos(Point::new(h.x0, h.y0))), vp.world_to_screen(pos(Point::new(h.x1, h.y1))));
    painter.rect_stroke(r, 0.0, st, egui::StrokeKind::Middle);
    let cx = s.x + s.w / 2.0;
    painter.line_segment(
        [vp.world_to_screen(pos(Point::new(cx, h.y1))), vp.world_to_screen(pos(Point::new(cx, s.y + s.h)))],
        Stroke::new(st.width + 1.0, st.color),
    );
}

/// The accent line across the lifelines at row `y` where a dragged or drawn block would land.
fn insertion_line(painter: &Painter, vp: &Viewport2D, page: &Page, y: f64, st: Stroke) {
    let cols = page.shapes.iter().filter(|s| is_lifeline(s));
    let (x0, x1) = cols.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), s| (a.min(s.x), b.max(s.x + s.w)));
    if !x0.is_finite() {
        return;
    }
    let (a, b) = (vp.world_to_screen(pos(Point::new(x0, y))), vp.world_to_screen(pos(Point::new(x1, y))));
    painter.line_segment([a, b], st);
    for p in [a, b] {
        painter.line_segment([p - vec2(0.0, 4.0), p + vec2(0.0, 4.0)], st);
    }
}

/// Over a Sequence page: the outline of the hovered and selected lifelines (head and stem), and
/// while a sequence drag runs its insertion line, rubber message and target stem.
pub fn overlay(app: &CraftApp, painter: &Painter, vp: &Viewport2D, page: &Page, real: &Page, t: &Tokens) {
    if !on_sequence(real) {
        return;
    }
    let drag = app.canvas.drag.as_ref();
    let hovered = match (&app.canvas.hover, drag) {
        (Some(Hit::Shape(id)), None) => Some(seq::owner(page, id)),
        _ => None,
    };
    for s in page.shapes.iter().filter(|s| is_lifeline(s)) {
        let selected = app.session.sel.items.contains(&s.id);
        if selected {
            lifeline_outline(painter, vp, s, Stroke::new(1.0, t.accent));
        } else if hovered.as_deref() == Some(s.id.as_str()) {
            lifeline_outline(painter, vp, s, Stroke::new(1.0, t.accent.gamma_multiply(0.6)));
        }
    }
    let Some(Drag::Sequence(d)) = drag else { return };
    let st = Stroke::new(2.0, t.accent);
    match d {
        SeqDrag::Reorder { id, to, .. } => {
            if let Some((_, y)) = seq::drop_position(real, to.y, Some(id)) {
                insertion_line(painter, vp, real, y, st);
            }
        }
        SeqDrag::Message { from, y, to } => {
            let Some(a) = real.shape(from) else { return };
            let ax = a.x + a.w / 2.0;
            let target = seq::lifeline_at(real, *to, f64::from(GRAB / vp.zoom)).and_then(|id| real.shape(&id).cloned());
            let row = real.sequence.as_ref().map_or(36.0, |q| q.row);
            let pts: Vec<Point> = if target.as_ref().is_some_and(|s| s.id == *from) {
                let x = ax + seq::ACTIVATION_W + 30.0;
                vec![Point::new(ax, *y), Point::new(x, *y), Point::new(x, y + row / 2.0), Point::new(ax, y + row / 2.0)]
            } else {
                vec![Point::new(ax, *y), Point::new(to.x, *y)]
            };
            let screen: Vec<Pos2> = pts.iter().map(|p| vp.world_to_screen(pos(*p))).collect();
            painter.add(egui::Shape::line(screen, Stroke::new(1.5, t.accent)));
            if let Some(s) = &target {
                lifeline_outline(painter, vp, s, Stroke::new(1.5, t.accent));
            }
            if let Some((_, line)) = seq::drop_position(real, *y, None) {
                insertion_line(painter, vp, real, line, Stroke::new(1.0, t.accent.gamma_multiply(0.7)));
            }
        }
        SeqDrag::Separator { fragment, to, .. } => {
            if let Some(f) = real.shape(fragment) {
                let (a, b) = (vp.world_to_screen(pos(Point::new(f.x, to.y))), vp.world_to_screen(pos(Point::new(f.x + f.w, to.y))));
                painter.extend(egui::Shape::dashed_line(&[a, b], st, 6.0, 4.0));
            }
        }
        SeqDrag::Column { .. } => {}
    }
}

/// A lifeline name for a new column: "Lifeline N", N one past the count on the page.
fn new_lifeline_name(page: &Page) -> String {
    format!("Lifeline {}", page.shapes.iter().filter(|s| is_lifeline(s)).count() + 1)
}

/// Add a lifeline at column `at` (the end when `None`) and edit its name in place.
fn add_lifeline(app: &mut CraftApp, actor: bool, at: Option<usize>) -> Result<Value, String> {
    let name = app.session.doc.page().map(new_lifeline_name).unwrap_or_default();
    let mut params = json!({"name": name, "actor": actor});
    if let Some(at) = at {
        params["at"] = json!(at);
    }
    let r = app.run("sequence.addLifeline", params)?;
    if let Some(id) = r.get("id").and_then(Value::as_str) {
        let id = id.to_string();
        run(app, "edit.select", json!({"ids": [id]}));
        super::start_editing(app, Hit::Shape(id));
    }
    Ok(r)
}

/// Add a note over lifeline `over` at `position` and edit its text in place.
fn add_note(app: &mut CraftApp, over: &str, position: Value) -> Result<Value, String> {
    let r = app.run("sequence.addNote", merged(json!({"text": "Note", "over": over}), position))?;
    if let Some(id) = r.get("id").and_then(Value::as_str) {
        let id = id.to_string();
        run(app, "edit.select", json!({"ids": [id]}));
        super::start_editing(app, Hit::Shape(id));
    }
    Ok(r)
}

/// Insert master `stencil` at world point `at` on a Sequence page: a lifeline master adds a
/// column there, a note master a note over the nearest lifeline at the row there. Returns whether
/// it handled the master (false off a Sequence page and for other masters).
pub fn insert_master(app: &mut CraftApp, stencil: &str, at: Point) -> bool {
    let Some(page) = app.session.doc.page().filter(|p| on_sequence(p)) else { return false };
    match stencil {
        "uml.lifeline" | "uml.actorLifeline" => {
            let col = seq::column_at(page, at.x);
            let _ = add_lifeline(app, stencil == "uml.actorLifeline", Some(col));
            true
        }
        "uml.note" => {
            let nearest = page
                .shapes
                .iter()
                .filter(|s| is_lifeline(s))
                .min_by(|a, b| (a.x + a.w / 2.0 - at.x).abs().total_cmp(&(b.x + b.w / 2.0 - at.x).abs()))
                .map(|s| s.id.clone());
            let Some(over) = nearest else {
                app.status("Add a lifeline before a note");
                return true;
            };
            let position = seq::drop_position(page, at.y, None).map_or(json!({}), |(p, _)| position_json(&p));
            let _ = add_note(app, &over, position);
            true
        }
        _ => false,
    }
}

/// On a Sequence page a double-click on a fragment edits the guard of the operand under the
/// pointer, and one on an activation bar or destruction mark edits its lifeline's name. Returns
/// whether it took the double-click.
pub fn double_click(app: &mut CraftApp, vp: &Viewport2D, p: Pos2) -> bool {
    let Some(page) = app.session.doc.page().filter(|p| on_sequence(p)) else { return false };
    let w = point(vp.screen_to_world(p));
    let Some(Hit::Shape(id)) = dg::hit(page, w, f64::from(HIT / vp.zoom)) else { return false };
    let owner = seq::owner(page, &id);
    if owner != id {
        super::start_editing(app, Hit::Shape(owner));
        return true;
    }
    let Some(f) = page.shape(&id) else { return false };
    let Some(Part::Fragment { operands, .. }) = &f.sequence else { return false };
    let k = operands.iter().enumerate().rev().find(|(_, o)| f.y + o.at <= w.y).map_or(0, |(k, _)| k);
    let text = operands.get(k).map(|o| o.guard.clone()).unwrap_or_default();
    let gx = f.container.as_ref().map_or(8.0, |c| c.margin);
    let at = Point::new(f.x + gx, f.y + operands.get(k).map_or(0.0, |o| o.at));
    app.canvas.drag = None;
    app.canvas.sequence.guard = Some(GuardEdit { fragment: id, operand: k, text, at, focus: true });
    true
}

/// The one-line guard editor over the canvas while a guard is edited: Enter (or a click away)
/// commits `sequence.setFragment` with only that guard changed, Escape cancels.
pub fn guard_editor(app: &mut CraftApp, ui: &Ui, vp: &Viewport2D) {
    let Some(mut g) = app.canvas.sequence.guard.take() else { return };
    let Some(Part::Fragment { operands, .. }) = app.session.doc.page().and_then(|p| p.shape(&g.fragment)).and_then(|s| s.sequence.clone()) else {
        return;
    };
    let mut done: Option<bool> = None;
    egui::Area::new(ui.id().with("sequence-guard-editor")).fixed_pos(vp.world_to_screen(pos(g.at))).order(egui::Order::Foreground).show(
        ui.ctx(),
        |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let out = egui::TextEdit::singleline(&mut g.text).desired_width(180.0).hint_text("guard").show(ui);
                let r = out.response.response.clone();
                if std::mem::take(&mut g.focus) {
                    // The whole guard starts selected, so typing replaces it.
                    r.request_focus();
                    let mut state = out.state.clone();
                    let chars = g.text.chars().count();
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(chars))));
                    state.store(ui.ctx(), r.id);
                }
                if r.lost_focus() {
                    done = Some(!ui.input(|i| i.key_pressed(egui::Key::Escape)));
                }
            });
        },
    );
    match done {
        Some(true) => {
            let mut guards: Vec<String> = operands.iter().map(|o| o.guard.clone()).collect();
            if let Some(slot) = guards.get_mut(g.operand)
                && *slot != g.text.trim()
            {
                *slot = g.text.trim().to_string();
                run(app, "sequence.setFragment", json!({"id": g.fragment, "guards": guards}));
            }
        }
        Some(false) => {}
        None => app.canvas.sequence.guard = Some(g),
    }
}

/// The selected lifeline, if one is.
fn selected_lifeline(app: &CraftApp) -> Option<String> {
    let page = app.session.doc.page()?;
    app.session.sel.items.iter().find(|id| page.shape(id).is_some_and(is_lifeline)).cloned()
}

/// `ui.sequenceLifeline` and `ui.sequenceNote`; `None` for any other id. Off a Sequence page they
/// say so in the status bar and change nothing.
pub fn ui_command(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if !matches!(id, "ui.sequenceLifeline" | "ui.sequenceNote") {
        return None;
    }
    let Some(page) = app.session.doc.page().filter(|p| on_sequence(p)) else {
        app.status("The page has no interaction (Insert > Diagram > New Sequence Diagram adds one)");
        return Some(Ok(json!({"id": null})));
    };
    Some(match id {
        "ui.sequenceLifeline" => {
            let actor = p.get("actor").and_then(Value::as_bool).unwrap_or(false);
            // Right of the selected lifeline, else at the end.
            let at = selected_lifeline(app).and_then(|l| {
                let x = page.shape(&l).map(|s| s.x + s.w / 2.0)?;
                Some(seq::column_at(page, x) + 1)
            });
            add_lifeline(app, actor, at)
        }
        _ => {
            let sel = app.session.sel.items.clone();
            let message = sel.iter().find_map(|i| page.connector(i).filter(|c| c.message.is_some()));
            let (over, position) = match (selected_lifeline(app), message) {
                (Some(l), _) => (Some(l), json!({})),
                (None, Some(c)) => (c.end.shape().or(c.start.shape()).map(str::to_string), json!({"after": c.id})),
                (None, None) => (None, json!({})),
            };
            match over {
                Some(o) => add_note(app, &o, position),
                None => {
                    app.status("Select a lifeline or a message to put a note beside");
                    Ok(json!({"id": null}))
                }
            }
        }
    })
}

/// What `ui.inspect` reports under `canvas.sequence`: whether the page is a Sequence page, the
/// guard being edited, and while a sequence drag runs, where its block would land.
pub fn summary(app: &CraftApp) -> Value {
    let page = app.session.doc.page();
    let active = page.is_some_and(on_sequence);
    let guard = app.canvas.sequence.guard.as_ref().map(|g| json!({"fragment": g.fragment, "operand": g.operand, "text": g.text}));
    let drop = match (&app.canvas.drag, page) {
        (Some(Drag::Sequence(SeqDrag::Reorder { id, to, .. })), Some(p)) => seq::drop_position(p, to.y, Some(id)),
        (Some(Drag::Sequence(SeqDrag::Message { y, .. })), Some(p)) => seq::drop_position(p, *y, None),
        _ => None,
    };
    json!({"active": active, "guard": guard, "drop": drop.map(|(p, y)| merged(json!({"y": y}), position_json(&p)))})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_settle_owns_take_no_handles_but_notes_do() {
        let mut s = Shape::new("x", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
        assert!(!frame_fixed(&s));
        for part in [
            Part::Lifeline { destroyed: false },
            Part::Fragment { operator: seq::Operator::Alt, operands: Vec::new(), covers: Vec::new() },
            Part::Activation { lifeline: "l".into(), depth: 1 },
            Part::Destruction { lifeline: "l".into() },
        ] {
            s.sequence = Some(part);
            assert!(frame_fixed(&s));
        }
        s.sequence = Some(Part::Note { over: Vec::new(), side: seq::Side::Over });
        assert!(!frame_fixed(&s));
        assert_eq!(
            merged(json!({"a": 1}), position_json(&Position::In { fragment: "f".into(), operand: 2 })),
            json!({"a": 1, "in": {"fragment": "f", "operand": 2}})
        );
    }
}
