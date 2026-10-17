//! The central canvas area. Today a bare pan-and-zoom surface; the diagram canvas (shapes,
//! connectors, selection handles, snapping) is built here.
//!
//! Provides [`Viewport2D`] for 2D pan/zoom math and pointer interaction handling.

use egui::{Pos2, Rect, Response, Ui, Vec2};

use crate::CraftApp;
use crate::theme::Tokens;

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

/// Handle mouse drag and scroll interactions for 2D pan and zoom.
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

    // Ctrl+scroll or Cmd+scroll zooms anchored at mouse pointer
    let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
    let modifiers = ui.input(|i| i.modifiers);
    if response.hovered() && modifiers.command && scroll_delta.y.abs() > 0.0 {
        let factor = if scroll_delta.y > 0.0 { 1.1 } else { 0.9 };
        let new_zoom = (*zoom * factor).clamp(0.1, 10.0);
        if (new_zoom - *zoom).abs() > 1e-4 {
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
    }

    changed
}

/// Draw the central canvas area.
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(t.canvas).show(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        handle_pan_and_zoom(ui, &response, &mut app.session.view.pan, &mut app.session.view.zoom);
        let _ = rect;
    });
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
