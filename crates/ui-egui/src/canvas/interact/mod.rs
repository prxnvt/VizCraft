//! Pointer and keyboard interaction on the page: selection (click, Shift-click, marquee), move,
//! resize and rotate handles, drawing and regluing connectors, in-place text editing and arrow
//! key nudges.
//!
//! A drag is previewed in UI state ([`Drag`]) and drawn from a copy of the page with the drag
//! applied ([`preview_page`]); only the release runs a command, so one drag is one undo step and
//! glued connectors follow live without touching the document.
//!
//! Layout: this module holds the constants, [`Drag`] and the per-frame [`pointer`] dispatch and
//! cursor; [`select`] handles clicks and the marquee, [`drag`] handle geometry, previews and
//! commits, [`connect`] connector ends, [`tools`] what a press starts per tool, [`edit_text`]
//! in-place text editing, [`keys`] the canvas keys and [`membership`] container membership on
//! moves and drops.

pub mod connect;
pub mod drag;
pub mod edit_text;
pub mod keys;
pub mod membership;
pub mod painter;
pub mod select;
pub mod text_block;
pub mod tools;

use egui::{Key, Response, Ui};
use serde_json::{Value, json};
use vizcraft_diagram::{self as dg, End, Point};

use super::Viewport2D;
use super::draw::point;
use crate::{CraftApp, Tool};
pub use connect::target_end;
use connect::{connect_ok, end_json};
use drag::finish;
pub use drag::{drag_target, handle_world, preview_page, resize_frame, rotate_angle, rotate_handle_world};
pub use edit_text::{TextEditing, commit_editing, insert_at_caret, start_editing, start_editing_new};
pub use keys::keys;
use select::click;
pub use select::{lasso_ids, marquee_ids, marquee_rect, single_connector, single_shape};
use tools::{Start, decide, near};

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
    /// Freehand lasso selection through `points`; `add` keeps the current selection (Shift).
    Lasso { points: Vec<Point>, add: bool },
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
    /// Drawing a text box with the Text tool on empty page.
    TextBox { from: Point, to: Point },
    /// Moving, resizing or turning a shape's text block with the Text Block tool.
    TextBlock { id: String, grab: text_block::BlockGrab, to: Point, shift: bool },
}

fn round(v: f64) -> f64 {
    v.round()
}

/// Run a canvas command; failures land in the status bar through `CraftApp::run`.
fn run(app: &mut CraftApp, id: &str, params: Value) {
    let _ = app.run(id, params);
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
            (Some(o), Some(_)) if app.tool == Tool::TextBlock => text_block::decide(app, vp, o, m.shift).map(Start::Drag),
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
            Some(
                Drag::Marquee { to, .. } | Drag::Move { to, .. } | Drag::Connect { to, .. } | Drag::Reglue { to, .. } | Drag::TextBox { to, .. },
            ) => *to = w,
            Some(Drag::TextBlock { to, shift, .. }) => {
                *to = w;
                *shift = m.shift;
            }
            Some(Drag::Resize { to, keep_aspect, .. }) => {
                *to = w;
                *keep_aspect = m.shift;
            }
            Some(Drag::Rotate { to, snap, .. }) => {
                *to = w;
                *snap = m.shift;
            }
            Some(Drag::Lasso { points, .. }) => select::extend_lasso(points, w, zoom),
            None => {}
        }
    }
    if response.drag_stopped()
        && let Some(d) = app.canvas.drag.take()
    {
        finish(app, d, zoom);
    }
    if response.double_clicked_by(egui::PointerButton::Primary) && app.tool == Tool::TextBlock {
        if let Some(p) = response.interact_pointer_pos() {
            text_block::double_click(app, vp, p);
        }
    } else if response.double_clicked_by(egui::PointerButton::Primary) && matches!(app.tool, Tool::Pointer | Tool::Lasso) {
        if let (Some(p), Some(page)) = (response.interact_pointer_pos(), app.session.doc.page())
            && let Some(h) = dg::hit(page, point(vp.screen_to_world(p)), f64::from(HIT / zoom))
        {
            edit_text::start_editing_at(app, h, point(vp.screen_to_world(p)));
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
        (None, Tool::FormatPainter { .. }) => egui::CursorIcon::Copy,
        (None, Tool::TextBlock) => egui::CursorIcon::Move,
        (None, Tool::Pointer | Tool::Lasso) => {
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
                } else if dg::glue_shape_at(page, w, tol).is_some_and(|s| dg::connection_point_at(s, w, tol).is_some()) {
                    egui::CursorIcon::Crosshair
                } else if app.canvas.hover.is_some() {
                    egui::CursorIcon::Move
                } else {
                    return;
                }
            } else if dg::glue_shape_at(page, w, tol).is_some_and(|s| dg::connection_point_at(s, w, tol).is_some()) {
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
