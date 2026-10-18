//! The Text Block tool (`ui.tool {"tool": "textBlock"}`): drag a shape's text area to move it,
//! its handles to resize it and its rotate handle to turn it; a double-click puts the text back in
//! the frame. Each finished drag runs one `text.setBlock`.
//!
//! The geometry reuses the shape handles: the text block is treated as a shape of its own (its
//! world frame and total rotation, [`block_shape`]), dragged like one, and turned back into a
//! [`TextBlock`] relative to the shape ([`block_from`]).

use egui::{Pos2, Rect, Stroke, vec2};
use serde_json::json;
use vizcraft_diagram::{self as dg, Page, Point, Shape, TextBlock};

use super::{Drag, HANDLES, MIN_SIZE, handle_world, near, resize_frame, rotate_angle, rotate_handle_world, run, single_shape};
use crate::CraftApp;
use crate::canvas::Viewport2D;
use crate::canvas::draw::{point, pos};
use crate::theme::Tokens;

/// What part of the text block a drag holds.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockGrab {
    /// The block itself, grabbed at `from`.
    Move { from: Point },
    /// Resize handle `handle` (index into [`HANDLES`]).
    Resize { handle: usize },
    /// The rotate handle.
    Rotate,
}

/// The text block of `s` as a shape of its own: its world frame and total rotation.
pub fn block_shape(s: &Shape) -> Shape {
    let (local, deg) = s.text_rect();
    let c = s.transform().apply(local.center());
    let (w, h) = (local.width(), local.height());
    Shape { x: c.x - w / 2.0, y: c.y - h / 2.0, w, h, angle_deg: (s.angle_deg + deg).rem_euclid(360.0), ..Shape::default() }
}

/// The [`TextBlock`] of `s` that puts its text where `b` (a [`block_shape`]) is.
pub fn block_from(s: &Shape, b: &Shape) -> Option<TextBlock> {
    let lc = s.transform().inverse()?.apply(b.center());
    let tb = TextBlock {
        dx: lc.x - s.w / 2.0,
        dy: lc.y - s.h / 2.0,
        w: b.w.max(MIN_SIZE),
        h: b.h.max(MIN_SIZE),
        angle_deg: (b.angle_deg - s.angle_deg).rem_euclid(360.0),
    };
    [tb.dx, tb.dy, tb.w, tb.h, tb.angle_deg].iter().all(|v| v.is_finite()).then_some(tb)
}

/// The block a drag to `to` gives (`shift` keeps the aspect or snaps the angle).
pub fn dragged(s: &Shape, grab: &BlockGrab, to: Point, shift: bool) -> Option<TextBlock> {
    let mut b = block_shape(s);
    match grab {
        BlockGrab::Move { from } => {
            let d = to - *from;
            b.x += d.x.round();
            b.y += d.y.round();
        }
        BlockGrab::Resize { handle } => {
            let (x, y, w, h) = resize_frame(&b, *handle, to, shift)?;
            (b.x, b.y, b.w, b.h) = (x, y, w, h);
        }
        BlockGrab::Rotate => b.angle_deg = rotate_angle(&b, to, shift),
    }
    block_from(s, &b)
}

/// Whether world point `w` is inside the text block of `s`.
fn inside(s: &Shape, w: Point) -> bool {
    let b = block_shape(s);
    b.transform().inverse().map(|t| t.apply(w)).is_some_and(|l| l.x >= 0.0 && l.y >= 0.0 && l.x <= b.w && l.y <= b.h)
}

/// What a press at `at` starts with the Text Block tool: a handle or the block of the selected
/// shape, else the block of the shape under the pointer (selected first).
pub(super) fn decide(app: &mut CraftApp, vp: &Viewport2D, at: Pos2, shift: bool) -> Option<Drag> {
    let w = point(vp.screen_to_world(at));
    let page = app.session.doc.page()?;
    if let Some(s) = single_shape(app, page) {
        let b = block_shape(s);
        if near(vp, rotate_handle_world(&b, vp.zoom), at) {
            return Some(Drag::TextBlock { id: s.id.clone(), grab: BlockGrab::Rotate, to: w, shift });
        }
        if let Some(i) = (0..HANDLES.len()).find(|i| handle_world(&b, *i).is_some_and(|h| near(vp, h, at))) {
            return Some(Drag::TextBlock { id: s.id.clone(), grab: BlockGrab::Resize { handle: i }, to: w, shift });
        }
        if inside(s, w) {
            return Some(Drag::TextBlock { id: s.id.clone(), grab: BlockGrab::Move { from: w }, to: w, shift });
        }
    }
    let id = dg::shape_at(page, w, f64::from(super::HIT / vp.zoom))?.id.clone();
    if app.session.sel.items != [id.clone()] {
        run(app, "edit.select", json!({"ids": [id]}));
    }
    Some(Drag::TextBlock { id, grab: BlockGrab::Move { from: w }, to: w, shift })
}

/// `page` with the drag's block applied to shape `id`, for the preview.
pub fn preview(page: &mut Page, id: &str, grab: &BlockGrab, to: Point, shift: bool) {
    if let Some(s) = page.shape_mut(id)
        && let Some(tb) = dragged(s, grab, to, shift)
    {
        s.text_block = Some(tb);
    }
}

/// Commit a finished drag as `text.setBlock` (nothing when the block did not change).
pub(super) fn finish(app: &mut CraftApp, id: &str, grab: &BlockGrab, to: Point, shift: bool) {
    let Some(s) = app.session.doc.page().and_then(|p| p.shape(id)) else { return };
    let Some(tb) = dragged(s, grab, to, shift) else { return };
    let before = s.text_block.unwrap_or(TextBlock { dx: 0.0, dy: 0.0, w: s.w, h: s.h, angle_deg: 0.0 });
    let round = |v: f64| (v * 100.0).round() / 100.0;
    let same = |a: f64, b: f64| (a - b).abs() < 1e-6;
    if same(tb.dx, before.dx) && same(tb.dy, before.dy) && same(tb.w, before.w) && same(tb.h, before.h) && same(tb.angle_deg, before.angle_deg) {
        return;
    }
    run(
        app,
        "text.setBlock",
        json!({"id": id, "dx": round(tb.dx), "dy": round(tb.dy), "w": round(tb.w), "h": round(tb.h), "angle": round(tb.angle_deg)}),
    );
}

/// A double-click with the Text Block tool: the text of the shape under the pointer (or the
/// selected one) goes back to the frame.
pub(super) fn double_click(app: &mut CraftApp, vp: &Viewport2D, at: Pos2) {
    let w = point(vp.screen_to_world(at));
    let Some(page) = app.session.doc.page() else { return };
    let id = dg::shape_at(page, w, f64::from(super::HIT / vp.zoom)).or_else(|| single_shape(app, page)).map(|s| s.id.clone());
    if let Some(id) = id
        && page.shape(&id).is_some_and(|s| s.text_block.is_some())
    {
        run(app, "text.setBlock", json!({"id": id, "reset": true}));
    }
}

/// The selected shape's text block with its handles, while the tool is on.
pub fn overlay(app: &CraftApp, painter: &egui::Painter, vp: &Viewport2D, page: &Page, t: &Tokens) {
    if app.tool != crate::Tool::TextBlock {
        return;
    }
    let Some(s) = single_shape(app, page) else { return };
    let b = block_shape(s);
    let tr = b.transform();
    let corners: Vec<Pos2> = [(0.0, 0.0), (b.w, 0.0), (b.w, b.h), (0.0, b.h), (0.0, 0.0)]
        .iter()
        .map(|&(x, y)| vp.world_to_screen(pos(tr.apply(Point::new(x, y)))))
        .collect();
    painter.extend(egui::Shape::dashed_line(&corners, Stroke::new(1.0, t.accent), 4.0, 3.0));
    let top = vp.world_to_screen(pos(tr.apply(Point::new(b.w / 2.0, 0.0))));
    let rot = vp.world_to_screen(pos(rotate_handle_world(&b, vp.zoom)));
    painter.line_segment([top, rot], Stroke::new(1.0, t.accent));
    painter.circle(rot, 4.5, t.accent, Stroke::new(1.5, t.page));
    for i in 0..HANDLES.len() {
        if let Some(h) = handle_world(&b, i) {
            let c = vp.world_to_screen(pos(h));
            painter.rect(Rect::from_center_size(c, vec2(7.0, 7.0)), 3.5, t.page, Stroke::new(1.0, t.accent), egui::StrokeKind::Middle);
        }
    }
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::ShapeKind;

    use super::*;

    #[test]
    fn the_block_shape_round_trips_on_a_rotated_shape() {
        let mut s = Shape::new("s", ShapeKind::Rectangle, 10.0, 20.0, 100.0, 60.0);
        s.angle_deg = 30.0;
        s.text_block = Some(TextBlock { dx: 12.0, dy: -7.0, w: 40.0, h: 20.0, angle_deg: 15.0 });
        let b = block_shape(&s);
        assert!((b.angle_deg - 45.0).abs() < 1e-9);
        let tb = block_from(&s, &b).unwrap();
        let want = s.text_block.unwrap();
        for (a, w) in [(tb.dx, want.dx), (tb.dy, want.dy), (tb.w, want.w), (tb.h, want.h), (tb.angle_deg, want.angle_deg)] {
            assert!((a - w).abs() < 1e-9, "{tb:?} vs {want:?}");
        }
    }

    #[test]
    fn drags_move_resize_and_turn_the_block() {
        let s = Shape::new("s", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
        let moved = dragged(&s, &BlockGrab::Move { from: Point::new(50.0, 30.0) }, Point::new(60.0, 25.0), false).unwrap();
        assert_eq!((moved.dx, moved.dy, moved.w, moved.h), (10.0, -5.0, 100.0, 60.0));
        // The right edge handle pulled in by 20 keeps the left edge: centre moves left by 10.
        let sized = dragged(&s, &BlockGrab::Resize { handle: 3 }, Point::new(80.0, 30.0), false).unwrap();
        assert!((sized.w - 80.0).abs() < 1e-9 && (sized.dx + 10.0).abs() < 1e-9, "{sized:?}");
        let turned = dragged(&s, &BlockGrab::Rotate, Point::new(150.0, 30.0), false).unwrap();
        assert!((turned.angle_deg - 90.0).abs() < 1e-9, "{turned:?}");
    }
}
