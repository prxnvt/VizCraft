//! Rulers and guide dragging. The rulers run along the top and left of the canvas in the page's
//! units, follow pan and zoom, mark the selection's extent and the pointer. Dragging out of a
//! ruler makes a guide (`guide.add`); dragging a guide moves it (`guide.move`) and dragging it
//! off the canvas, back onto a ruler, deletes it (`guide.delete`). One drag is one command.
//!
//! The guide being dragged lives in egui's temporary memory ([`guide_drag`]) rather than in
//! [`super::CanvasState`], so the overlay can draw it without this module owning canvas state.

use egui::epaint::TextShape;
use egui::{Align2, Id, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;
use vizcraft_diagram::{self as dg, GuideOrientation, snap};

use super::Viewport2D;
use crate::CraftApp;
use crate::theme::{Tokens, regular};

/// Thickness of each ruler in screen points.
pub const RULER_W: f32 = 18.0;
/// Screen distance either side of a guide that grabs it.
const GUIDE_GRAB: f32 = 4.0;
/// Smallest gap between minor ticks, and between labels, in screen points.
const TICK_MIN_PX: f64 = 6.0;
const LABEL_MIN_PX: f64 = 56.0;

/// A guide being dragged: a new one out of a ruler (`index` is `None`) or an existing one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuideDrag {
    /// A vertical guide (constant x); otherwise horizontal.
    pub vertical: bool,
    /// Where it is now, in page units.
    pub position: f64,
    /// The guide being moved; `None` for one being made.
    pub index: Option<usize>,
    /// The pointer is off the canvas: releasing now deletes (or, for a new one, drops) it.
    pub off: bool,
}

fn drag_id() -> Id {
    Id::new("vizcraft.guide_drag")
}

/// The guide drag in progress, if any.
pub fn guide_drag(ctx: &egui::Context) -> Option<GuideDrag> {
    ctx.data(|d| d.get_temp::<GuideDrag>(drag_id()))
}

fn set_guide_drag(ctx: &egui::Context, g: Option<GuideDrag>) {
    ctx.data_mut(|d| match g {
        Some(g) => {
            d.insert_temp(drag_id(), g);
        }
        None => d.remove::<GuideDrag>(drag_id()),
    });
}

/// The part of `full` the drawing gets: all of it, or what the rulers leave when they show.
pub fn canvas_rect(app: &CraftApp, full: Rect) -> Rect {
    if app.session.view.aids.rulers { Rect::from_min_max(full.min + vec2(RULER_W, RULER_W), full.max) } else { full }
}

/// A guide position for the pointer at `p`: the world coordinate, snapped to the grid when
/// grid snapping is on.
fn guide_position(app: &CraftApp, vp: &Viewport2D, p: Pos2, vertical: bool) -> f64 {
    let w = vp.screen_to_world(p);
    let raw = f64::from(if vertical { w.x } else { w.y });
    let aids = &app.session.view.aids;
    let Some(page) = app.session.doc.page() else { return raw.round() };
    match aids.snap_tolerance(vp.zoom) {
        Some(tol) if aids.snap_grid => {
            let step = snap::grid_step(page.units, f64::from(vp.zoom), crate::canvas::draw::overlays::GRID_MIN_PX);
            let t = snap::Targets { grid: Some(step), ..Default::default() };
            let s = snap::snap_point(dg::Point::new(raw, raw), &t, tol);
            raw + if vertical { s.offset.x } else { s.offset.y }
        }
        _ => raw.round(),
    }
}

/// Rulers and guide dragging for this frame. `full` is the whole canvas area, rulers included.
pub fn show(app: &mut CraftApp, ui: &Ui, full: Rect, vp: &Viewport2D, t: &Tokens) {
    let ctx = ui.ctx().clone();
    let pointer = ctx.pointer_latest_pos();
    let mut start: Option<GuideDrag> = None;
    let aids = app.session.view.aids;
    if aids.rulers {
        let top = Rect::from_min_max(pos2(vp.screen_rect.min.x, full.min.y), pos2(full.max.x, vp.screen_rect.min.y));
        let left = Rect::from_min_max(pos2(full.min.x, vp.screen_rect.min.y), pos2(vp.screen_rect.min.x, full.max.y));
        paint(app, ui, full, top, left, vp, t, pointer);
        // A drag out of the top ruler makes a horizontal guide, out of the left one a vertical guide.
        for (r, vertical, name) in [(top, false, "top"), (left, true, "left")] {
            let resp = ui.interact(r, Id::new("vizcraft.ruler").with(name), Sense::drag());
            if resp.hovered() {
                ctx.set_cursor_icon(if vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
            }
            if resp.drag_started_by(egui::PointerButton::Primary) && aids.guides {
                start = Some(GuideDrag { vertical, position: 0.0, index: None, off: true });
            }
        }
    }
    if aids.guides
        && let Some(page) = app.session.doc.page()
    {
        let dragging = guide_drag(&ctx).and_then(|g| g.index);
        for (i, g) in page.guides.iter().enumerate().take(dg::page_fields::MAX_GUIDES) {
            let vertical = g.orientation == GuideOrientation::Vertical;
            let at = vp.world_to_screen(pos2(g.position as f32, g.position as f32));
            let r = if vertical {
                Rect::from_min_max(pos2(at.x - GUIDE_GRAB, vp.screen_rect.min.y), pos2(at.x + GUIDE_GRAB, vp.screen_rect.max.y))
            } else {
                Rect::from_min_max(pos2(vp.screen_rect.min.x, at.y - GUIDE_GRAB), pos2(vp.screen_rect.max.x, at.y + GUIDE_GRAB))
            };
            if !r.intersects(vp.screen_rect) || dragging == Some(i) {
                continue;
            }
            let resp = ui.interact(r.intersect(vp.screen_rect), Id::new("vizcraft.guide").with(i), Sense::drag());
            if resp.hovered() {
                ctx.set_cursor_icon(if vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
            }
            if resp.drag_started_by(egui::PointerButton::Primary) {
                start = Some(GuideDrag { vertical, position: g.position, index: Some(i), off: false });
            }
        }
    }
    if let Some(s) = start {
        set_guide_drag(&ctx, Some(s));
    }
    let Some(mut g) = guide_drag(&ctx) else { return };
    if let Some(p) = pointer {
        g.off = !vp.screen_rect.contains(p);
        if !g.off {
            g.position = guide_position(app, vp, p, g.vertical);
        }
    }
    if ctx.input(|i| i.pointer.primary_down()) {
        set_guide_drag(&ctx, Some(g));
        return;
    }
    set_guide_drag(&ctx, None);
    finish(app, g);
}

/// Commit a finished guide drag as one command.
fn finish(app: &mut CraftApp, g: GuideDrag) {
    let current = g.index.and_then(|i| app.session.doc.page().and_then(|p| p.guides.get(i)).map(|x| x.position));
    let _ = match (g.index, g.off) {
        (None, true) => return,
        (None, false) => {
            let orientation = if g.vertical { "vertical" } else { "horizontal" };
            app.run("guide.add", json!({"orientation": orientation, "position": g.position}))
        }
        (Some(i), true) => app.run("guide.delete", json!({"index": i})),
        (Some(i), false) => {
            if current == Some(g.position) {
                return;
            }
            app.run("guide.move", json!({"index": i, "position": g.position}))
        }
    };
}

/// Ticks along one ruler: `(screen position, is a labelled tick, label)` for every tick between
/// screen coordinates `lo` and `hi`; `origin` is the screen coordinate of page 0.
fn ticks(units: dg::Units, zoom: f64, origin: f32, lo: f32, hi: f32) -> Vec<(f32, bool, String)> {
    let unit = snap::unit_size(units);
    let minor = snap::grid_step(units, zoom, TICK_MIN_PX);
    let major = snap::grid_step(units, zoom, LABEL_MIN_PX);
    let (w0, w1) = (f64::from(lo - origin) / zoom, f64::from(hi - origin) / zoom);
    let mut out = Vec::new();
    if !(minor > 0.0 && w0.is_finite() && w1.is_finite()) {
        return out;
    }
    let first = (w0 / minor).floor() as i64;
    let last = (w1 / minor).ceil() as i64;
    // A screen holds a few hundred ticks at most; this caps a broken zoom.
    for k in first..=last.min(first.saturating_add(2000)) {
        let w = k as f64 * minor;
        let labelled = ((w / major).round() * major - w).abs() < minor * 1e-3;
        let x = origin + (w * zoom) as f32;
        let label = if labelled { fmt_units(w / unit) } else { String::new() };
        out.push((x, labelled, label));
    }
    out
}

/// A ruler number: whole numbers plainly, others with up to three decimals and no trailing zeros.
fn fmt_units(v: f64) -> String {
    // Rounded first so a value a hair below zero reads 0, not -0.
    let v = (v * 1000.0).round() / 1000.0;
    let v = if v == 0.0 { 0.0 } else { v };
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

#[allow(clippy::too_many_arguments)]
fn paint(app: &CraftApp, ui: &Ui, full: Rect, top: Rect, left: Rect, vp: &Viewport2D, t: &Tokens, pointer: Option<Pos2>) {
    let painter = ui.painter_at(full);
    let Some(page) = app.session.doc.page() else { return };
    let zoom = f64::from(vp.zoom);
    let origin = vp.world_to_screen(pos2(0.0, 0.0));
    let corner = Rect::from_min_max(full.min, pos2(left.max.x, top.max.y));
    for r in [top, left, corner] {
        painter.rect_filled(r, 0.0, t.panel);
    }
    // The selection's extent, shaded on both rulers.
    let sel = app.session.sel.items.iter().filter_map(|id| page.shape(id)).map(|s| s.bounds()).reduce(|a, b| a.union(&b));
    if let Some(b) = sel {
        let a = vp.world_to_screen(pos2(b.x0 as f32, b.y0 as f32));
        let z = vp.world_to_screen(pos2(b.x1 as f32, b.y1 as f32));
        painter.with_clip_rect(top).rect_filled(Rect::from_x_y_ranges(a.x..=z.x, top.y_range()), 0.0, t.selection);
        painter.with_clip_rect(left).rect_filled(Rect::from_x_y_ranges(left.x_range(), a.y..=z.y), 0.0, t.selection);
    }
    let font = regular(9.0);
    let tick = Stroke::new(1.0, t.text_dim);
    let top_p = painter.with_clip_rect(top);
    for (x, labelled, label) in ticks(page.units, zoom, origin.x, top.min.x, top.max.x) {
        let len = if labelled { RULER_W } else { RULER_W * 0.3 };
        top_p.line_segment([pos2(x, top.max.y - len), pos2(x, top.max.y)], tick);
        if labelled {
            top_p.text(pos2(x + 2.0, top.min.y + 1.0), Align2::LEFT_TOP, label, font.clone(), t.text_dim);
        }
    }
    let left_p = painter.with_clip_rect(left);
    for (y, labelled, label) in ticks(page.units, zoom, origin.y, left.min.y, left.max.y) {
        let len = if labelled { RULER_W } else { RULER_W * 0.3 };
        left_p.line_segment([pos2(left.max.x - len, y), pos2(left.max.x, y)], tick);
        if labelled {
            // Turned a quarter anticlockwise, reading upwards, as on a drawing board.
            let galley = ui.ctx().fonts_mut(|f| f.layout_no_wrap(label, font.clone(), t.text_dim));
            left_p.add(TextShape::new(pos2(left.min.x + 1.0, y - 2.0), galley, t.text_dim).with_angle(-std::f32::consts::FRAC_PI_2));
        }
    }
    // Where the pointer is.
    if let Some(p) = pointer.filter(|p| vp.screen_rect.contains(*p)) {
        let mark = Stroke::new(1.0, t.accent);
        top_p.line_segment([pos2(p.x, top.min.y), pos2(p.x, top.max.y)], mark);
        left_p.line_segment([pos2(left.min.x, p.y), pos2(left.max.x, p.y)], mark);
    }
    let edge = Stroke::new(1.0, t.border);
    painter.line_segment([pos2(full.min.x, top.max.y), pos2(full.max.x, top.max.y)], edge);
    painter.line_segment([pos2(left.max.x, full.min.y), pos2(left.max.x, full.max.y)], edge);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_numbers_read_cleanly() {
        assert_eq!(fmt_units(0.0), "0");
        assert_eq!(fmt_units(-0.0000001), "0");
        assert_eq!(fmt_units(2.0), "2");
        assert_eq!(fmt_units(0.25), "0.25");
        assert_eq!(fmt_units(-1.5), "-1.5");
    }

    #[test]
    fn ticks_label_whole_units_at_100_percent() {
        // Inches at 100%: labels need 56 points, so one per inch; minor ticks every sixteenth.
        let t = ticks(dg::Units::Inches, 1.0, 0.0, 0.0, 300.0);
        let labels: Vec<&str> = t.iter().filter(|x| x.1).map(|x| x.2.as_str()).collect();
        assert_eq!(labels, ["0", "1", "2", "3"]);
        assert!(t.len() > 30, "{} ticks", t.len());
        assert!(ticks(dg::Units::Inches, f64::NAN, 0.0, 0.0, 300.0).len() < 3000);
    }
}
