//! The Crop tool (Picture > Size > Crop): with one picture selected, its frame shows eight crop
//! handles and the whole picture's extent as a faint outline. Dragging a handle moves that edge of
//! the visible part within the picture; the picture keeps its scale, so the shape's frame grows
//! or shrinks with the crop. The release commits one `edit.batch` of `shape.resize` and
//! `picture.crop`, so a crop is one undo step. Anywhere else the tool behaves like the pointer.

use egui::{Color32, Pos2, Response, Stroke, Ui};
use serde_json::json;
use vizcraft_diagram::shape_fields::CropRect;
use vizcraft_diagram::{Point, Shape};

use super::{GRAB, HANDLES};
use crate::canvas::Viewport2D;
use crate::canvas::draw::{point, pos};
use crate::theme::Tokens;
use crate::{CraftApp, Tool};

/// A crop handle drag in progress.
#[derive(Clone, Debug, PartialEq)]
struct CropDrag {
    id: String,
    handle: usize,
    to: Point,
}

fn key() -> egui::Id {
    egui::Id::new("vizcraft.cropDrag")
}

/// The selected picture shape the tool works on.
fn target(app: &CraftApp) -> Option<&Shape> {
    let page = app.session.doc.page()?;
    super::single_shape(app, page).filter(|s| s.image.is_some())
}

/// A world point in the shape's unrotated frame.
fn local(s: &Shape, w: Point) -> Point {
    s.transform().inverse().map(|t| t.apply(w)).unwrap_or(Point::new(w.x - s.x, w.y - s.y))
}

/// The whole picture's rectangle in the shape's frame: `(x0, y0, x1, y1)`.
fn full(s: &Shape, c: CropRect) -> (f64, f64, f64, f64) {
    let (cw, ch) = (c.w.max(0.001), c.h.max(0.001));
    let (iw, ih) = (s.w / cw, s.h / ch);
    let (x0, y0) = (-c.x * iw, -c.y * ih);
    (x0, y0, x0 + iw, y0 + ih)
}

/// The frame (in the shape's frame: left, top, right, bottom) and crop after dragging `handle`
/// to `to` (a point in the shape's frame).
fn cropped(s: &Shape, c: CropRect, handle: usize, to: Point) -> ((f64, f64, f64, f64), CropRect) {
    let (fx0, fy0, fx1, fy1) = full(s, c);
    let (mut l, mut t, mut r, mut b) = (0.0, 0.0, s.w, s.h);
    let (hx, hy) = HANDLES.get(handle).copied().unwrap_or((1.0, 1.0));
    if hx == 0.0 {
        l = to.x.clamp(fx0, r - 1.0);
    } else if hx == 1.0 {
        r = to.x.clamp(l + 1.0, fx1);
    }
    if hy == 0.0 {
        t = to.y.clamp(fy0, b - 1.0);
    } else if hy == 1.0 {
        b = to.y.clamp(t + 1.0, fy1);
    }
    let (iw, ih) = ((fx1 - fx0).max(1e-9), (fy1 - fy0).max(1e-9));
    let mut crop = CropRect { x: (l - fx0) / iw, y: (t - fy0) / ih, w: (r - l) / iw, h: (b - t) / ih };
    crop.sanitize();
    ((l, t, r, b), crop)
}

/// Pointer handling for the Crop tool; true when it took the pointer this frame.
pub fn pointer(app: &mut CraftApp, ui: &Ui, response: &Response, vp: &Viewport2D) -> bool {
    if app.tool != Tool::Crop {
        return false;
    }
    let ctx = ui.ctx().clone();
    let Some(s) = target(app).cloned() else {
        ctx.data_mut(|d| d.remove::<CropDrag>(key()));
        return false;
    };
    let Some(img) = s.image.clone() else { return false };
    let t = s.transform();
    let screen = |x: f64, y: f64| vp.world_to_screen(pos(t.apply(Point::new(x, y))));
    let mut drag: Option<CropDrag> = ctx.data(|d| d.get_temp::<CropDrag>(key())).filter(|d| d.id == s.id);
    let mut took = drag.is_some();
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(o) = ui.input(|i| i.pointer.press_origin())
        && let Some(h) = (0..HANDLES.len()).find(|i| {
            let (hx, hy) = HANDLES.get(*i).copied().unwrap_or_default();
            screen(hx * s.w, hy * s.h).distance(o) <= GRAB * 1.5
        })
    {
        drag = Some(CropDrag { id: s.id.clone(), handle: h, to: point(vp.screen_to_world(o)) });
        took = true;
    }
    if let Some(d) = drag.as_mut()
        && response.dragged_by(egui::PointerButton::Primary)
        && let Some(p) = response.interact_pointer_pos()
    {
        d.to = point(vp.screen_to_world(p));
    }
    let to_local = drag.as_ref().map(|d| local(&s, d.to));
    let (frame, crop) = match (&drag, to_local) {
        (Some(d), Some(l)) => cropped(&s, img.crop, d.handle, l),
        _ => ((0.0, 0.0, s.w, s.h), img.crop),
    };
    if response.drag_stopped() && drag.is_some() {
        let (l, tp, r, b) = frame;
        let centre = t.apply(Point::new((l + r) / 2.0, (tp + b) / 2.0));
        let (w, h) = (r - l, b - tp);
        let _ = app.run(
            "edit.batch",
            json!({"label": "Crop", "commands": [
                {"command": "shape.resize", "params": {"id": s.id, "x": centre.x - w / 2.0, "y": centre.y - h / 2.0, "w": w, "h": h}},
                {"command": "picture.crop", "params": {"id": s.id, "crop": {"x": crop.x, "y": crop.y, "w": crop.w, "h": crop.h}}},
            ]}),
        );
        drag = None;
    }
    match &drag {
        Some(d) => ctx.data_mut(|m| {
            m.insert_temp(key(), d.clone());
        }),
        None => ctx.data_mut(|m| {
            m.remove::<CropDrag>(key());
        }),
    }
    paint(&ctx, vp, &s, img.crop, frame, &Tokens::get(&ctx));
    if response.hovered() {
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    took
}

/// The picture's whole extent (faint), the crop frame and its handles, over the page.
fn paint(ctx: &egui::Context, vp: &Viewport2D, s: &Shape, crop: CropRect, frame: (f64, f64, f64, f64), t: &Tokens) {
    let painter =
        ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("vizcraft.cropOverlay"))).with_clip_rect(vp.screen_rect);
    let tr = s.transform();
    let quad = |x0: f64, y0: f64, x1: f64, y1: f64| -> Vec<Pos2> {
        [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].iter().map(|&(x, y)| vp.world_to_screen(pos(tr.apply(Point::new(x, y))))).collect()
    };
    let (fx0, fy0, fx1, fy1) = full(s, crop);
    painter.add(egui::Shape::closed_line(quad(fx0, fy0, fx1, fy1), Stroke::new(1.0, t.accent.gamma_multiply(0.5))));
    let (l, tp, r, b) = frame;
    let pts = quad(l, tp, r, b);
    painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, t.accent)));
    for (hx, hy) in HANDLES {
        let p = vp.world_to_screen(pos(tr.apply(Point::new(l + hx * (r - l), tp + hy * (b - tp)))));
        // Crop handles are bars, not the round resize handles, so the two read differently.
        let horizontal = hy != 0.5;
        let (dx, dy) = if horizontal { (6.0, 2.0) } else { (2.0, 6.0) };
        let rect = egui::Rect::from_center_size(p, egui::vec2(dx * 2.0, dy * 2.0));
        painter.rect(rect, 0.0, Color32::BLACK, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Outside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vizcraft_diagram::ShapeKind;

    #[test]
    fn dragging_the_right_handle_in_crops_and_keeps_the_scale() {
        let s = Shape::new("p", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 50.0);
        let ((l, t, r, b), c) = cropped(&s, CropRect::FULL, 3, Point::new(60.0, 25.0));
        assert_eq!((l, t, r, b), (0.0, 0.0, 60.0, 50.0));
        assert!((c.w - 0.6).abs() < 1e-9 && c.x == 0.0 && c.h == 1.0);
        // Dragging back out past the picture's edge stops at the edge.
        let s2 = Shape::new("p", ShapeKind::Rectangle, 0.0, 0.0, 60.0, 50.0);
        let crop = CropRect { x: 0.0, y: 0.0, w: 0.6, h: 1.0 };
        let ((_, _, r, _), c) = cropped(&s2, crop, 3, Point::new(500.0, 25.0));
        assert!((r - 100.0).abs() < 1e-9 && (c.w - 1.0).abs() < 1e-9);
        // The top-left corner moves the origin.
        let ((l, t, _, _), c) = cropped(&s, CropRect::FULL, 0, Point::new(20.0, 10.0));
        assert_eq!((l, t), (20.0, 10.0));
        assert!((c.x - 0.2).abs() < 1e-9 && (c.y - 0.2).abs() < 1e-9);
    }
}
