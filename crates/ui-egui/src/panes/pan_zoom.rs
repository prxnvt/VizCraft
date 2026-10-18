//! The Pan and Zoom pane: a minimap of the page frame (on an infinite board, the drawing's bounds)
//! with every shape as a filled rectangle and the area the canvas shows as a frame. Dragging the
//! frame pans the canvas; dragging a corner resizes it, which zooms; a click or drag outside it
//! centres it there. All changes go through `view.pan` and `view.zoom`.

use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use serde_json::json;
use vizcraft_engine::cmd::view::{MAX_ZOOM, MIN_ZOOM};

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::theme::Tokens;

/// The Pan and Zoom pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("panZoom", "Pan and Zoom", Side::Right, 260.0, pan_zoom);

/// Shapes drawn in the minimap; a larger drawing shows its first shapes only.
const MAX_SHAPES: usize = 5000;
/// Distance from a corner of the frame that grabs it, in screen points.
const GRAB: f32 = 9.0;
/// Room around the world rectangle, as a share of its size.
const PAD: f32 = 0.06;
/// Tallest the minimap grows, as a multiple of its width.
const MAX_ASPECT: f32 = 1.2;

/// A uniform mapping between world coordinates and the minimap's screen rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Map {
    /// Where the minimap is drawn.
    pub area: Rect,
    /// The world rectangle it shows.
    pub world: Rect,
    scale: f32,
}

impl Map {
    /// Fit `world` into `area`, centred, keeping proportions.
    pub fn new(area: Rect, world: Rect) -> Map {
        let sx = area.width() / world.width().max(f32::EPSILON);
        let sy = area.height() / world.height().max(f32::EPSILON);
        let scale = sx.min(sy);
        Map { area, world, scale: if scale.is_finite() && scale > 0.0 { scale } else { 1.0 } }
    }
    /// A world point on the screen.
    pub fn to_screen(self, p: Pos2) -> Pos2 {
        self.area.center() + (p - self.world.center()) * self.scale
    }
    /// A screen point in the world.
    pub fn to_world(self, p: Pos2) -> Pos2 {
        self.world.center() + (p - self.area.center()) / self.scale
    }
    /// A world rectangle on the screen.
    pub fn rect_to_screen(self, r: Rect) -> Rect {
        Rect::from_two_pos(self.to_screen(r.min), self.to_screen(r.max))
    }
}

/// The world rectangle the minimap shows: the page frame and the visible area, with a margin, so
/// both are always in view.
pub fn world_rect(frame: Rect, visible: Rect) -> Rect {
    let r = frame.union(visible);
    r.expand2(r.size() * PAD)
}

/// The area the canvas shows, in world coordinates: centred on `pan`, `canvas` points across at
/// `zoom`.
pub fn visible_rect(pan: [f32; 2], zoom: f32, canvas: [f32; 2]) -> Rect {
    let half = vec2(canvas[0], canvas[1]) / (2.0 * zoom.max(0.01));
    Rect::from_center_size(pos2(pan[0], pan[1]), half * 2.0)
}

/// Drag the frame's `corner` (screen) to `pointer` (world) while the opposite corner `fixed`
/// stays put, keeping the canvas's proportions: the new visible rectangle's zoom (a percentage)
/// and centre. The zoom is clamped to the limits and the frame follows the clamped size.
pub fn resize_by_corner(fixed: Pos2, pointer: Pos2, canvas: [f32; 2]) -> (f32, Pos2) {
    let aspect = if canvas[1] > 0.0 { (canvas[0] / canvas[1]).max(0.01) } else { 1.0 };
    let (dx, dy) = (pointer.x - fixed.x, pointer.y - fixed.y);
    // The larger of the two asks wins, so the frame never stays smaller than the pointer's box.
    let w = dx.abs().max(dy.abs() * aspect).max(1e-3);
    let pct = (canvas[0] / w * 100.0).clamp(MIN_ZOOM, MAX_ZOOM);
    let w = canvas[0] / (pct / 100.0);
    let h = w / aspect;
    let (sx, sy) = (if dx < 0.0 { -1.0 } else { 1.0 }, if dy < 0.0 { -1.0 } else { 1.0 });
    (pct, pos2(fixed.x + sx * w / 2.0, fixed.y + sy * h / 2.0))
}

/// What a drag in progress holds, kept in egui memory between frames. The map is frozen at the
/// start, because the frame moving would otherwise move the map under the pointer.
#[derive(Clone, Copy)]
struct Drag {
    map: Map,
    mode: Mode,
}

#[derive(Clone, Copy)]
enum Mode {
    /// Move the frame; `grab` is the pointer's offset from the frame's centre, in world units.
    Move { grab: Vec2 },
    /// Resize by a corner; `fixed` is the opposite corner, in world units.
    Resize { fixed: Pos2 },
}

fn drag_id() -> egui::Id {
    egui::Id::new("pan_zoom_drag")
}

/// Where the pane drew last frame (for tests and `ui.inspect`-style checks).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// The minimap's mapping.
    pub map: Map,
    /// The visible area as drawn, in screen coordinates.
    pub frame: Rect,
}

fn geometry_id() -> egui::Id {
    egui::Id::new("pan_zoom_geometry")
}

fn pan_zoom(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(canvas) = app.session.view.canvas else {
        ui.label(egui::RichText::new("The canvas is not shown").small().color(t.text_dim));
        return;
    };
    let Some(page) = app.session.doc.page() else { return };
    let frame_world = {
        let f = page.frame();
        Rect::from_min_max(pos2(f.x0 as f32, f.y0 as f32), pos2(f.x1 as f32, f.y1 as f32))
    };
    let visible = visible_rect(app.session.view.pan, app.session.view.zoom, canvas);
    let width = ui.available_width().max(40.0);
    let height = (width * MAX_ASPECT).min(ui.available_height()).max(60.0);
    let (area, resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let drag: Option<Drag> = ui.ctx().data(|d| d.get_temp(drag_id()));
    let map = match (drag, resp.dragged()) {
        (Some(d), true) => d.map,
        _ => Map::new(area.shrink(4.0), world_rect(frame_world, visible)),
    };
    let vis_screen = map.rect_to_screen(visible);

    // Input first, so what is drawn below is already the new view.
    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let corner = [vis_screen.left_top(), vis_screen.right_top(), vis_screen.left_bottom(), vis_screen.right_bottom()]
            .into_iter()
            .find(|c| c.distance(p) <= GRAB);
        let mode = match corner {
            Some(c) => {
                let opposite = pos2(vis_screen.center().x * 2.0 - c.x, vis_screen.center().y * 2.0 - c.y);
                Mode::Resize { fixed: map.to_world(opposite) }
            }
            None if vis_screen.contains(p) => Mode::Move { grab: map.to_world(p) - visible.center() },
            None => Mode::Move { grab: Vec2::ZERO },
        };
        ui.ctx().data_mut(|d| d.insert_temp(drag_id(), Drag { map, mode }));
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        set_pan(app, map.to_world(p));
    }
    if resp.dragged()
        && let (Some(d), Some(p)) = (drag.or_else(|| ui.ctx().data(|d| d.get_temp(drag_id()))), resp.interact_pointer_pos())
    {
        let world = d.map.to_world(p);
        match d.mode {
            Mode::Move { grab } => set_pan(app, world - grab),
            Mode::Resize { fixed } => {
                let (pct, center) = resize_by_corner(fixed, world, canvas);
                let _ = app.run("view.zoom", json!({"value": pct}));
                set_pan(app, center);
            }
        }
    }
    if resp.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<Drag>(drag_id()));
    }
    if let Some(p) = resp.hover_pos().filter(|_| !resp.dragged()) {
        let on_corner = [vis_screen.left_top(), vis_screen.right_top(), vis_screen.left_bottom(), vis_screen.right_bottom()]
            .iter()
            .any(|c| c.distance(p) <= GRAB);
        ui.ctx().set_cursor_icon(if on_corner {
            CursorIcon::ResizeNwSe
        } else if vis_screen.contains(p) {
            CursorIcon::Grab
        } else {
            CursorIcon::PointingHand
        });
    }

    // Draw from the state as it is now.
    let visible = visible_rect(app.session.view.pan, app.session.view.zoom, canvas);
    let vis_screen = map.rect_to_screen(visible);
    ui.ctx().data_mut(|d| d.insert_temp(geometry_id(), Geometry { map, frame: vis_screen }));
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 3.0, t.canvas);
    painter.rect_stroke(area, 3.0, Stroke::new(1.0, t.border), StrokeKind::Inside);
    let page_screen = map.rect_to_screen(frame_world);
    painter.rect_filled(page_screen, 0.0, t.page);
    painter.rect_stroke(page_screen, 0.0, Stroke::new(1.0, t.border_strong), StrokeKind::Middle);
    let fill = Color32::from_rgba_unmultiplied(t.accent.r(), t.accent.g(), t.accent.b(), 120);
    if let Some(page) = app.session.doc.page() {
        for sh in page.shapes.iter().take(MAX_SHAPES) {
            let r = map.rect_to_screen(Rect::from_min_size(pos2(sh.x as f32, sh.y as f32), vec2(sh.w as f32, sh.h as f32)));
            // A tiny shape still shows as a dot.
            let r = if r.width() < 1.5 || r.height() < 1.5 { Rect::from_center_size(r.center(), vec2(r.width().max(1.5), r.height().max(1.5))) } else { r };
            if r.is_finite() {
                painter.rect_filled(r, 0.0, fill);
            }
        }
    }
    let clipped = vis_screen.intersect(area);
    painter.rect_filled(clipped, 0.0, Color32::from_rgba_unmultiplied(t.accent.r(), t.accent.g(), t.accent.b(), 28));
    painter.rect_stroke(vis_screen, 0.0, Stroke::new(1.5, t.accent), StrokeKind::Middle);
    for c in [vis_screen.left_top(), vis_screen.right_top(), vis_screen.left_bottom(), vis_screen.right_bottom()] {
        painter.rect_filled(Rect::from_center_size(c, vec2(6.0, 6.0)), 1.0, t.accent);
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new(format!("{}%", (app.session.view.zoom * 100.0).round())).small().color(t.text_dim));
}

/// Centre the canvas on a world point through `view.pan`.
fn set_pan(app: &mut CraftApp, p: Pos2) {
    if p.x.is_finite() && p.y.is_finite() && app.session.view.pan != [p.x, p.y] {
        let _ = app.run("view.pan", json!({"x": p.x, "y": p.y}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use vizcraft_engine::Session;

    fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(pos2(x0, y0), pos2(x1, y1))
    }

    #[test]
    fn the_map_round_trips_and_keeps_proportions() {
        let m = Map::new(r(0.0, 0.0, 200.0, 100.0), r(0.0, 0.0, 400.0, 400.0));
        let p = pos2(123.0, 321.0);
        assert!((m.to_world(m.to_screen(p)) - p).length() < 1e-3);
        let s = m.rect_to_screen(r(0.0, 0.0, 400.0, 400.0));
        assert!((s.width() - s.height()).abs() < 1e-3, "a square stays square");
        assert!(m.area.contains_rect(s.shrink(0.01)));
        // A degenerate world does not divide by zero.
        let m = Map::new(r(0.0, 0.0, 10.0, 10.0), r(5.0, 5.0, 5.0, 5.0));
        assert!(m.to_screen(pos2(5.0, 5.0)).is_finite());
    }

    #[test]
    fn corner_resize_keeps_the_far_corner_and_the_canvas_proportions() {
        // A 1000 by 500 canvas at 100% shows 1000 by 500 world units. Dragging the bottom right
        // corner out to twice the width halves the zoom.
        let (pct, centre) = resize_by_corner(pos2(0.0, 0.0), pos2(2000.0, 1000.0), [1000.0, 500.0]);
        assert_eq!(pct, 50.0);
        assert_eq!(centre, pos2(1000.0, 500.0));
        // The pointer on the other side flips the frame to that side.
        let (_, centre) = resize_by_corner(pos2(0.0, 0.0), pos2(-2000.0, -1000.0), [1000.0, 500.0]);
        assert_eq!(centre, pos2(-1000.0, -500.0));
        // Limits clamp the zoom, and the frame follows.
        let (pct, centre) = resize_by_corner(pos2(0.0, 0.0), pos2(1.0, 1.0), [1000.0, 500.0]);
        assert_eq!(pct, MAX_ZOOM);
        assert_eq!(centre, pos2(125.0, 62.5));
        let (pct, _) = resize_by_corner(pos2(0.0, 0.0), pos2(1.0e9, 0.0), [1000.0, 500.0]);
        assert_eq!(pct, MIN_ZOOM);
        // Junk canvases do not panic.
        let _ = resize_by_corner(pos2(0.0, 0.0), pos2(5.0, 5.0), [0.0, 0.0]);
    }

    fn harness() -> Harness<'static, CraftApp> {
        let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default());
        app.run("ui.pane.panZoom", json!({"value": true})).unwrap();
        let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).with_max_steps(64).build_ui_state(
            |ui, app: &mut CraftApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
        h.run_steps(6);
        h
    }

    fn geometry(h: &Harness<'_, CraftApp>) -> Geometry {
        h.ctx.data(|d| d.get_temp(geometry_id())).expect("the pane drew")
    }

    #[test]
    fn dragging_the_frame_pans_the_canvas() {
        let mut h = harness();
        h.state_mut().run("view.zoom", json!({"value": 200})).unwrap();
        h.run_steps(3);
        let before = h.state().session.view.pan;
        let g = geometry(&h);
        let start = g.frame.center();
        h.drag_at(start);
        h.run_steps(2);
        for i in 1..=6 {
            h.hover_at(start + vec2(6.0 * i as f32, 3.0 * i as f32));
            h.run_steps(1);
        }
        h.drop_at(start + vec2(36.0, 18.0));
        h.run_steps(3);
        let after = h.state().session.view.pan;
        // The pointer moved 36 by 18 points on the minimap; the map says how far that is in the world.
        let moved = g.map.to_world(start + vec2(36.0, 18.0)) - g.map.to_world(start);
        let want = [before[0] + moved.x, before[1] + moved.y];
        // The release lands a frame after the last move, so the pan has followed the pointer to
        // within a step or so of where it was let go.
        let (got_x, got_y) = (after[0] - before[0], after[1] - before[1]);
        assert!(got_x > 0.0 && got_y > 0.0, "panned right and down: {before:?} -> {after:?}");
        assert!((got_x / (want[0] - before[0]) - 1.0).abs() < 0.25 && (got_y / (want[1] - before[1]) - 1.0).abs() < 0.25, "pan {before:?} -> {after:?}, wanted about {want:?}");
        assert_eq!(h.state().session.view.zoom, 2.0, "a move does not zoom");
    }

    #[test]
    fn dragging_a_corner_zooms_and_a_click_recentres() {
        let mut h = harness();
        h.state_mut().run("view.zoom", json!({"value": 200})).unwrap();
        h.run_steps(3);
        let g = geometry(&h);
        let corner = g.frame.right_bottom();
        h.drag_at(corner);
        h.run_steps(2);
        let to = g.frame.left_top() + (g.frame.size() * 1.5);
        for i in 1..=6 {
            h.hover_at(corner + (to - corner) * (i as f32 / 6.0));
            h.run_steps(1);
        }
        h.drop_at(to);
        h.run_steps(3);
        let zoom = h.state().session.view.zoom;
        assert!((zoom - 4.0 / 3.0).abs() < 0.1, "a frame 1.5 times wider shows 2/3 the zoom: {zoom}");
        // A click on the page far from the frame centres the frame there.
        let g = geometry(&h);
        let target = g.map.area.center() + vec2(-40.0, -30.0);
        let want = g.map.to_world(target);
        h.drag_at(target);
        h.run_steps(1);
        h.drop_at(target);
        h.run_steps(3);
        let pan = h.state().session.view.pan;
        assert!((pan[0] - want.x).abs() < 3.0 && (pan[1] - want.y).abs() < 3.0, "pan {pan:?} wanted {want:?}");
    }

    #[test]
    fn the_pane_follows_zoom_and_pan_changes_and_starts_closed() {
        let mut h = harness();
        let before = geometry(&h).frame;
        h.state_mut().run("view.zoom", json!({"value": 400})).unwrap();
        h.run_steps(3);
        let after = geometry(&h).frame;
        assert!(after.width() < before.width() || (after.width() - before.width()).abs() < 1.0, "zoomed in shows a smaller area");
        let fresh = crate::panes::PaneLayout::default();
        assert!(!fresh.is_open("panZoom"), "hidden until View > Show > Task Panes opens it");
    }
}
