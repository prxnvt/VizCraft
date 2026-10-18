//! The diagram canvas: the active page (over its background pages) drawn through [`Viewport2D`] on a pasteboard, with selection,
//! handles, connector drawing, in-place text editing and drops from the Shapes pane.
//!
//! - [`draw`] paints the document (sheet, shapes, connectors, text) in document colours, and
//!   the selection chrome on top (`draw/overlays.rs`).
//! - [`interact`] turns pointer and keys into commands; drags are previewed in [`CanvasState`]
//!   and commit one command on release. In-place text editing is `interact/edit_text.rs`.
//! - [`tess`] triangulates concave outlines for filling.
//! - The page tab strip along the bottom edge is `crate::page_tabs`.
//!
//! This module holds the viewport, the canvas state, pan and zoom, insertion and the per-frame
//! [`show`] that runs input and painting. Chrome (selection, handles, hover, marquee, connection
//! points) uses `theme::Tokens` only.

pub mod draw;
pub mod interact;
pub mod rulers;
pub mod tess;

use egui::{Pos2, Rect, Response, Ui, Vec2};
use serde_json::{Value, json};
use vizcraft_diagram::{self as dg, Hit, Page};

use crate::CraftApp;
use crate::theme::Tokens;
use draw::point;
pub use interact::{Drag, TextEditing};

/// 2D viewport coordinates and projections between screen space and world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport2D {
    /// Screen-space bounding rectangle allocated for the canvas.
    pub screen_rect: Rect,
    /// Center of the viewport in world-space coordinates.
    pub pan: Pos2,
    /// Zoom scale factor (1.0 = 100%).
    pub zoom: f32,
}

impl Viewport2D {
    /// Create a new 2D viewport projection.
    pub fn new(screen_rect: Rect, pan: [f32; 2], zoom: f32) -> Self {
        Self { screen_rect, pan: Pos2::new(pan[0], pan[1]), zoom: zoom.max(0.01) }
    }

    /// Convert a screen-space position to world-space coordinates.
    pub fn screen_to_world(&self, screen: Pos2) -> Pos2 {
        self.pan + (screen - self.screen_rect.center()) / self.zoom
    }

    /// Convert a world-space position to screen-space coordinates.
    pub fn world_to_screen(&self, world: Pos2) -> Pos2 {
        self.screen_rect.center() + (world - self.pan) * self.zoom
    }

    /// Convert a screen-space vector to world space.
    pub fn screen_to_world_vec(&self, screen_vec: Vec2) -> Vec2 {
        screen_vec / self.zoom
    }

    /// Convert a world-space vector to screen space.
    pub fn world_to_screen_vec(&self, world_vec: Vec2) -> Vec2 {
        world_vec * self.zoom
    }

    /// Visible bounds in world coordinates: `(top_left, bottom_right)`.
    pub fn visible_world_bounds(&self) -> (Pos2, Pos2) {
        let min = self.screen_to_world(self.screen_rect.min);
        let max = self.screen_to_world(self.screen_rect.max);
        (min, max)
    }

    /// The visible part of the world as a model rectangle.
    pub fn visible_world_rect(&self) -> dg::Rect {
        let (a, b) = self.visible_world_bounds();
        dg::Rect::from_points(point(a), point(b))
    }

    /// Whether a world-space point is inside the visible viewport.
    pub fn is_world_point_visible(&self, world: Pos2) -> bool {
        let (min, max) = self.visible_world_bounds();
        let min_x = min.x.min(max.x);
        let max_x = min.x.max(max.x);
        let min_y = min.y.min(max.y);
        let max_y = min.y.max(max.y);
        world.x >= min_x && world.x <= max_x && world.y >= min_y && world.y <= max_y
    }
}

/// Canvas UI state: what is being dragged or edited, what is under the pointer, and caches.
/// Nothing here is document state; a drag only reaches the document when it ends.
#[derive(Default)]
pub struct CanvasState {
    /// The canvas's screen rectangle in the last frame (tests and the Shapes pane use it).
    pub rect: Option<Rect>,
    /// The drag in progress.
    pub drag: Option<Drag>,
    /// In-place text editing in progress.
    pub editing: Option<TextEditing>,
    /// Where the caret was when the last edit ended (item id, character index), for inserting a
    /// symbol there after the editor closed.
    pub last_caret: Option<(String, usize)>,
    /// The shape or connector under the pointer.
    pub hover: Option<Hit>,
    /// The shape whose connection points show (under or near the pointer).
    pub hover_shape: Option<String>,
    /// The shape or connector the armed format painter copies from (`Tool::FormatPainter`).
    pub painter_from: Option<String>,
    /// The view has been centred on the page once.
    centered: bool,
    /// A diagram was opened before the canvas was laid out; fit it once the size is known.
    pub(crate) fit_pending: bool,
    cache: draw::FillCache,
}

impl CanvasState {
    /// The viewport of the last frame for this view, if the canvas has been laid out.
    /// Shapes with a cached fill mesh, for the performance trace.
    pub fn fill_cache_len(&self) -> usize {
        self.cache.len()
    }

    pub fn viewport(&self, view: &vizcraft_engine::ViewState) -> Option<Viewport2D> {
        self.rect.map(|r| Viewport2D::new(r, view.pan, view.zoom))
    }
}

/// Handle mouse drag and scroll interactions for 2D pan and zoom: middle drag, Space or Cmd with
/// a left drag, plain scroll pans; pinch, or Cmd (Ctrl) with scroll, zooms about the pointer.
pub fn handle_pan_and_zoom(ui: &Ui, response: &Response, pan: &mut [f32; 2], zoom: &mut f32) -> bool {
    let mut changed = false;

    // Middle-click drag, or left-click drag with Space or Cmd (Ctrl off macOS) held. Space is
    // free for this because no command is bound to it (see `keys::is_shortcut_key`).
    let (command_down, space_down) = ui.input(|i| (i.modifiers.command, i.key_down(egui::Key::Space)));
    let pan_modifier = command_down || space_down;
    let is_panning = response.dragged_by(egui::PointerButton::Middle) || (pan_modifier && response.dragged_by(egui::PointerButton::Primary));

    // The open hand says a left drag will pan; the closed hand says it is panning.
    if is_panning {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if space_down && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    if is_panning {
        let delta = response.drag_delta();
        if delta != Vec2::ZERO {
            let world_delta = delta / (*zoom).max(0.01);
            pan[0] -= world_delta.x;
            pan[1] -= world_delta.y;
            changed = true;
        }
    }

    // Pinch, and Cmd (Ctrl) with scroll, arrive as egui's zoom factor: egui turns a scroll with the
    // zoom modifier into `zoom_delta` and reports no scroll for it. Zoom by that factor, so the
    // gesture's size sets the step, anchored at the pointer.
    let (scroll_delta, zoom_delta) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
    if response.hovered() && zoom_delta.is_finite() && (zoom_delta - 1.0).abs() > 1e-4 {
        let new_zoom = (*zoom * zoom_delta).clamp(0.1, 10.0);
        if (new_zoom - *zoom).abs() > 1e-6 {
            if let Some(hover_pos) = response.hover_pos() {
                let rect = response.rect;
                let vp = Viewport2D::new(rect, *pan, *zoom);
                let cursor_world = vp.screen_to_world(hover_pos);
                let new_pan_pos = cursor_world - (hover_pos - rect.center()) / new_zoom;
                pan[0] = new_pan_pos.x;
                pan[1] = new_pan_pos.y;
            }
            *zoom = new_zoom;
            changed = true;
        }
    } else if response.hovered() && scroll_delta != Vec2::ZERO {
        // Plain scroll (and Shift+scroll, which egui turns sideways) moves the page with the wheel.
        let world_delta = scroll_delta / (*zoom).max(0.01);
        pan[0] -= world_delta.x;
        pan[1] -= world_delta.y;
        changed = true;
    }

    changed
}

/// The pan that shows `page` in a canvas of `size` points at `zoom`: centred on each axis where
/// it fits, otherwise its top or left edge a small margin in.
pub fn initial_pan(page: &Page, size: Vec2, zoom: f32) -> [f32; 2] {
    let zoom = zoom.max(0.01);
    let margin = 24.0 / zoom;
    let axis = |extent: f64, view: f32| {
        let extent = extent as f32;
        let view = view / zoom;
        if extent + 2.0 * margin <= view { extent / 2.0 } else { view / 2.0 - margin }
    };
    [axis(page.width, size.x), axis(page.height, size.y)]
}

/// A compact summary of the document and canvas for `ui.inspect`.
pub fn summary(app: &CraftApp) -> Value {
    let page = app.session.doc.page();
    json!({
        "page": page.map(|p| json!({"id": p.id, "width": p.width, "height": p.height})),
        "shapes": page.map_or(0, |p| p.shapes.len()),
        "connectors": page.map_or(0, |p| p.connectors.len()),
        "selected": &app.session.sel.items,
        "hover": app.canvas.hover.as_ref().map(|h| match h { Hit::Shape(id) | Hit::Connector(id) => id.clone() }),
        "editing": app.canvas.editing.as_ref().map(|e| e.id().to_string()),
        "dragging": app.canvas.drag.as_ref().map(|d| match d {
            Drag::Marquee { .. } => "marquee",
            Drag::Lasso { .. } => "lasso",
            Drag::Move { .. } => "move",
            Drag::Resize { .. } => "resize",
            Drag::Rotate { .. } => "rotate",
            Drag::Connect { .. } => "connect",
            Drag::Reglue { .. } => "reglue",
            Drag::TextBox { .. } => "textBox",
            Drag::TextBlock { .. } => "textBlock",
        }),
        "canvas": app.canvas.rect.map(|r| [r.min.x, r.min.y, r.width(), r.height()]),
    })
}

/// Insert master `stencil` centred on world point `at`.
pub fn insert_master(app: &mut CraftApp, stencil: &str, at: dg::Point) {
    let Some(m) = vizcraft_stencils::master(stencil) else { return };
    let (mut x, mut y) = ((at.x - m.w / 2.0).round(), (at.y - m.h / 2.0).round());
    // Repeated clicks would stack copies exactly on top of each other; step each one down-right.
    if let Some(page) = app.session.doc.page() {
        for _ in 0..50 {
            if !page.shapes.iter().any(|s| (s.x - x).abs() < 0.5 && (s.y - y).abs() < 0.5) {
                break;
            }
            x += 20.0;
            y += 20.0;
        }
    }
    interact::membership::insert(app, stencil, x, y, m.w, m.h);
}

/// Where a click in the Shapes pane inserts: the centre of the visible part of the page, or the
/// page centre when the page is out of view or the canvas has not been laid out.
pub fn insert_point(app: &CraftApp) -> dg::Point {
    let Some(page) = app.session.doc.page() else { return dg::Point::ZERO };
    let pr = page.rect();
    let visible = app.canvas.viewport(&app.session.view).map(|vp| vp.visible_world_rect());
    match visible {
        Some(v) if v.intersects(&pr) => dg::Rect::new(v.x0.max(pr.x0), v.y0.max(pr.y0), v.x1.min(pr.x1), v.y1.min(pr.y1)).center(),
        _ => pr.center(),
    }
}

/// Draw the central canvas area and handle its input.
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    // The page tab strip takes the bottom edge; rulers and the canvas share the rest.
    let full = ui.available_rect_before_wrap();
    let strip_h = crate::page_tabs::STRIP_H.min(full.height());
    {
        let _t = vizcraft_engine::trace::span("canvas.pageTabs");
        crate::page_tabs::show(app, ui, Rect::from_min_max(Pos2::new(full.min.x, full.max.y - strip_h), full.max));
    }
    let (above, _) = ui.allocate_exact_size(Vec2::new(full.width(), full.height() - strip_h), egui::Sense::hover());
    let rect = rulers::canvas_rect(app, above);
    let response = ui.interact(rect, ui.id().with("canvas"), egui::Sense::click_and_drag());
    app.canvas.rect = Some(rect);
    if rect.width() > 1.0 && rect.height() > 1.0 {
        // Only a real size: a fit into a collapsed canvas would drop to the smallest zoom.
        app.session.view.canvas = Some([rect.width(), rect.height()]);
    }
    if !app.canvas.centered && rect.width() > 1.0 && rect.height() > 1.0 {
        app.canvas.centered = true;
        if app.session.view.pan == [0.0, 0.0]
            && let Some(page) = app.session.doc.page()
        {
            app.session.view.pan = initial_pan(page, rect.size(), app.session.view.zoom);
        }
        if std::mem::take(&mut app.canvas.fit_pending) {
            app.fit_if_cut_off();
        }
    }
    handle_pan_and_zoom(ui, &response, &mut app.session.view.pan, &mut app.session.view.zoom);
    let vp = Viewport2D::new(rect, app.session.view.pan, app.session.view.zoom);

    {
        let _t = vizcraft_engine::trace::span("canvas.interact");
        interact::keys(app, ui.ctx());
        interact::pointer(app, ui, &response, &vp);
        interact::drag::snap_move(app, vp.zoom, ui.input(|i| i.modifiers.alt));
    }
    if let Some(payload) = response.dnd_release_payload::<crate::panes::MasterDrag>()
        && let Some(p) = ui.ctx().pointer_interact_pos().or(response.hover_pos())
    {
        insert_master(app, &payload.0, point(vp.screen_to_world(p)));
    }
    // A shape or connector deleted under the editor (undo, an agent) ends the edit.
    if let Some(ed) = &app.canvas.editing
        && !app.session.doc.page().is_some_and(|p| p.contains_id(ed.id()))
    {
        app.canvas.editing = None;
    }

    {
        let _t = vizcraft_engine::trace::span("canvas.paint");
        paint(app, ui, &vp, &t);
    }
    {
        let _t = vizcraft_engine::trace::span("canvas.rulers");
        rulers::show(app, ui, above, &vp, &t);
    }
    let _t = vizcraft_engine::trace::span("canvas.textOverlay");
    interact::edit_text::text_overlay(app, ui, &vp, &t);
}

fn paint(app: &mut CraftApp, ui: &Ui, vp: &Viewport2D, t: &Tokens) {
    let painter = ui.painter_at(vp.screen_rect);
    let Some(real) = app.session.doc.page() else { return };
    let preview = {
        let _t = vizcraft_engine::trace::span("paint.dragPreview");
        app.canvas.drag.as_ref().and_then(|d| interact::preview_page(real, d, vp.zoom))
    };
    let page = preview.as_ref().unwrap_or(real);
    {
        let _t = vizcraft_engine::trace::span("paint.sheetAndGrid");
        draw::sheet(&painter, vp, page, t);
        draw::overlays::underlay(app, &painter, vp, page, t);
    }
    let visible = vp.visible_world_rect().inflate(4.0);
    let editing = app.canvas.editing.as_ref().filter(|e| e.compartment.is_none()).map(|e| e.id().to_string());
    // Fields in shape text expand against the active page, also on its background pages.
    let fields = dg::text_fields::FieldCtx::new(&app.session.doc, app.session.doc.active_index(), Some((crate::now_ms() / 1000.0) as u64));
    // Background pages under the active one, deepest first. Drawn only: hit testing and every
    // command look at the active page, so nothing here can be selected or edited.
    {
        let _t = vizcraft_engine::trace::span("paint.backgroundPages");
        draw::shapes::background_pages(&painter, vp, &fields, &mut app.canvas.cache);
    }
    // Hidden shapes are not drawn; a layer colour tints the shapes on it.
    let shapes_span = vizcraft_engine::trace::span("paint.shapes");
    for s in page.shapes.iter().filter(|s| dg::layer::visible(page, s)) {
        if s.bounds().intersects(&visible) {
            let tinted = dg::layer::tinted(page, s);
            let s = dg::text_fields::expand_shape(&tinted, &fields);
            // A callout's leader just under the callout, over whatever it points across.
            draw::containers::leader(&painter, vp, page, &s);
            draw::shape(&painter, vp, &s, &mut app.canvas.cache, editing.as_deref() != Some(s.id.as_str()));
        }
    }
    app.canvas.cache.end_frame();
    drop(shapes_span);
    // Page-level geometry, so connectors with line jumps hop over the ones drawn before them.
    let geometry = {
        let _t = vizcraft_engine::trace::span("paint.connectorGeometry");
        dg::jumps::page_geometry(page)
    };
    {
        let _t = vizcraft_engine::trace::span("paint.connectors");
        for (c, g) in page.connectors.iter().zip(geometry) {
            if draw::connector_bounds(&g).is_some_and(|b| b.intersects(&visible)) {
                draw::connector(&painter, vp, c, &g, editing.as_deref() != Some(c.id.as_str()));
            }
        }
    }
    let _t = vizcraft_engine::trace::span("paint.overlays");
    draw::overlays::overlay(app, &painter, vp, page, real, t);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_projections_round_trip() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(800.0, 600.0));
        let vp = Viewport2D::new(rect, [25.0, -10.0], 1.5);

        let screen_pt = Pos2::new(250.0, 180.0);
        let world_pt = vp.screen_to_world(screen_pt);
        let back_to_screen = vp.world_to_screen(world_pt);
        assert!((back_to_screen.x - screen_pt.x).abs() < 1e-4);
        assert!((back_to_screen.y - screen_pt.y).abs() < 1e-4);

        let screen_vec = Vec2::new(45.0, -30.0);
        let world_vec = vp.screen_to_world_vec(screen_vec);
        let back_to_screen_vec = vp.world_to_screen_vec(world_vec);
        assert!((back_to_screen_vec.x - screen_vec.x).abs() < 1e-4);
        assert!((back_to_screen_vec.y - screen_vec.y).abs() < 1e-4);

        let center_world = vp.screen_to_world(rect.center());
        assert!((center_world.x - 25.0).abs() < 1e-4);
        assert!((center_world.y - (-10.0)).abs() < 1e-4);
        assert!(vp.is_world_point_visible(center_world));
    }

    /// Drag across a bare canvas with `button` and `modifiers` held, and Space too when `space`;
    /// returns the resulting pan.
    fn drag_pan(button: egui::PointerButton, modifiers: egui::Modifiers, space: bool) -> [f32; 2] {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(400.0, 300.0)).build_ui_state(
            |ui, pan: &mut [f32; 2]| {
                let (_, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                let mut zoom = 1.0;
                handle_pan_and_zoom(ui, &response, pan, &mut zoom);
            },
            [0.0, 0.0],
        );
        let start = Pos2::new(200.0, 150.0);
        let send = |h: &mut egui_kittest::Harness<'_, [f32; 2]>, event: egui::Event| {
            h.input_mut().events.push(event);
            h.run();
        };
        send(&mut h, egui::Event::ModifiersChanged(modifiers));
        let space_event = |pressed| egui::Event::Key { key: egui::Key::Space, physical_key: None, pressed, repeat: false, modifiers };
        if space {
            send(&mut h, space_event(true));
        }
        send(&mut h, egui::Event::PointerMoved(start));
        send(&mut h, egui::Event::PointerButton { pos: start, button, pressed: true, modifiers });
        let mut pos = start;
        for _ in 0..4 {
            pos += Vec2::new(15.0, 10.0);
            send(&mut h, egui::Event::PointerMoved(pos));
        }
        send(&mut h, egui::Event::PointerButton { pos, button, pressed: false, modifiers });
        if space {
            send(&mut h, space_event(false));
        }
        *h.state()
    }

    #[test]
    fn command_left_drag_and_middle_drag_pan() {
        for (button, modifiers) in [(egui::PointerButton::Primary, egui::Modifiers::COMMAND), (egui::PointerButton::Middle, egui::Modifiers::NONE)] {
            let pan = drag_pan(button, modifiers, false);
            // Dragging right and down moves the view's world center left and up.
            assert!(pan[0] < -20.0 && pan[1] < -10.0, "{button:?} with {modifiers:?} panned to {pan:?}");
        }
    }

    /// Run `events` over a bare 400 by 300 canvas at zoom 1 and pan 0; returns (pan, zoom).
    fn gesture(events: Vec<egui::Event>) -> ([f32; 2], f32) {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(400.0, 300.0)).build_ui_state(
            |ui, (pan, zoom): &mut ([f32; 2], f32)| {
                let (_, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                handle_pan_and_zoom(ui, &response, pan, zoom);
            },
            ([0.0, 0.0], 1.0),
        );
        for e in events {
            h.input_mut().events.push(e);
            h.run();
        }
        *h.state()
    }

    #[test]
    fn pinch_and_command_scroll_zoom_about_the_pointer() {
        let at = Pos2::new(300.0, 100.0);
        // A pinch: egui's zoom event, as macOS trackpads send it.
        let (pan, zoom) = gesture(vec![egui::Event::PointerMoved(at), egui::Event::Zoom(1.25)]);
        assert!((zoom - 1.25).abs() < 1e-3, "pinch zoomed to {zoom}");
        // The page point under the pointer stays under it.
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0));
        let before = Viewport2D::new(rect, [0.0, 0.0], 1.0).screen_to_world(at);
        let after = Viewport2D::new(rect, pan, zoom).screen_to_world(at);
        assert!((before - after).length() < 0.5, "{before:?} drifted to {after:?}");
        // Cmd (Ctrl) with the wheel: egui reports it as zoom, not scroll.
        let wheel = |y| egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: Vec2::new(0.0, y),
            modifiers: egui::Modifiers::COMMAND,
            phase: egui::TouchPhase::Move,
        };
        let cmd = egui::Event::ModifiersChanged(egui::Modifiers::COMMAND);
        let (_, zoom_in) = gesture(vec![egui::Event::PointerMoved(at), cmd.clone(), wheel(1.0)]);
        let (_, zoom_out) = gesture(vec![egui::Event::PointerMoved(at), cmd, wheel(-1.0)]);
        assert!(zoom_in > 1.0 && zoom_out < 1.0, "Cmd+wheel zoomed to {zoom_in} and {zoom_out}");
        // A plain wheel pans and leaves the zoom alone.
        let plain = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -4.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        };
        let (pan, zoom) = gesture(vec![egui::Event::PointerMoved(at), plain]);
        assert_eq!(zoom, 1.0);
        assert!(pan[1] > 0.0, "scrolling down moves the view down the page: {pan:?}");
    }

    #[test]
    fn plain_left_drag_does_not_pan() {
        assert_eq!(drag_pan(egui::PointerButton::Primary, egui::Modifiers::NONE, false), [0.0, 0.0]);
    }

    #[test]
    fn space_left_drag_pans() {
        let pan = drag_pan(egui::PointerButton::Primary, egui::Modifiers::NONE, true);
        assert!(pan[0] < -20.0 && pan[1] < -10.0, "Space+drag panned to {pan:?}");
    }
}
