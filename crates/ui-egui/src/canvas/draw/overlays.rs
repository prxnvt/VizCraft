//! Chrome over and under the page. Under the shapes ([`underlay`]): the grid and, on a fixed
//! board, page breaks.
//! Over them ([`overlay`]): guides, alignment lines while a move snaps, the hover outline,
//! connection points, connector targets, selected items, resize and rotate handles, and the
//! marquee. Colours come from `theme::Tokens`.

use egui::{Color32, Pos2, Rect, Stroke, vec2};
use vizcraft_diagram::{self as dg, End, GuideOrientation, Hit, Page, snap};

use super::pos;
use crate::CraftApp;
use crate::canvas::Viewport2D;
use crate::canvas::interact::{self, Drag};
use crate::theme::Tokens;

fn frame_screen(vp: &Viewport2D, s: &dg::Shape) -> Vec<Pos2> {
    let tr = s.transform();
    [(0.0, 0.0), (s.w, 0.0), (s.w, s.h), (0.0, s.h)].iter().map(|&(x, y)| vp.world_to_screen(pos(tr.apply(dg::Point::new(x, y))))).collect()
}

fn connection_marks(painter: &egui::Painter, vp: &Viewport2D, s: &dg::Shape, colour: Color32, hot: Option<usize>) {
    for (i, p) in s.connection_points_world().into_iter().enumerate() {
        let c = vp.world_to_screen(pos(p));
        let r = 3.0;
        let st = Stroke::new(1.5, colour);
        painter.line_segment([c - vec2(r, r), c + vec2(r, r)], st);
        painter.line_segment([c - vec2(r, -r), c + vec2(r, -r)], st);
        if hot == Some(i) {
            painter.circle_stroke(c, 6.0, Stroke::new(2.0, colour));
        }
    }
}

/// Smallest gap between grid lines on screen, in points. The grid (and grid snapping, and the
/// rulers' guide placement) uses the finest tidy step at least this far apart.
pub const GRID_MIN_PX: f64 = 12.0;

/// Paper size the page breaks assume: US Letter in the page's orientation. The model has no
/// paper size yet, so a page larger than this prints on several sheets.
fn paper(page: &Page) -> (f64, f64) {
    match page.orientation {
        dg::Orientation::Portrait => (850.0, 1100.0),
        _ => (1100.0, 850.0),
    }
}

/// Lines across `span` (screen coordinates along the other axis) at world positions `k * step`
/// from `lo` to `hi`, every `major`th darker.
#[allow(clippy::too_many_arguments)]
fn grid_lines(
    painter: &egui::Painter,
    vp: &Viewport2D,
    vertical: bool,
    lo: f64,
    hi: f64,
    step: f64,
    major: u32,
    span: (f32, f32),
    minor_c: Color32,
    major_c: Color32,
) {
    if !(step > 0.0 && lo.is_finite() && hi.is_finite()) {
        return;
    }
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    // The step keeps lines at least GRID_MIN_PX apart, so a screen holds a few hundred at most.
    for k in first..=last.min(first.saturating_add(4000)) {
        let w = (k as f64 * step) as f32;
        let c = if k.rem_euclid(i64::from(major.max(1))) == 0 { major_c } else { minor_c };
        let st = Stroke::new(1.0, c);
        if vertical {
            let x = vp.world_to_screen(egui::pos2(w, 0.0)).x;
            painter.line_segment([egui::pos2(x, span.0), egui::pos2(x, span.1)], st);
        } else {
            let y = vp.world_to_screen(egui::pos2(0.0, w)).y;
            painter.line_segment([egui::pos2(span.0, y), egui::pos2(span.1, y)], st);
        }
    }
}

/// The grid and page breaks, drawn on the sheet under the shapes.
pub fn underlay(app: &CraftApp, painter: &egui::Painter, vp: &Viewport2D, page: &Page, t: &Tokens) {
    let aids = &app.session.view.aids;
    let v = vp.visible_world_rect();
    let infinite = page.board.is_infinite();
    // The grid covers the whole viewport on an infinite board and the sheet on a fixed one.
    let pr = if infinite { v } else { page.frame() };
    let (x0, y0, x1, y1) = (v.x0.max(pr.x0), v.y0.max(pr.y0), v.x1.min(pr.x1), v.y1.min(pr.y1));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let a = vp.world_to_screen(pos(dg::Point::new(x0, y0)));
    let b = vp.world_to_screen(pos(dg::Point::new(x1, y1)));
    if aids.grid {
        let step = snap::grid_step(page.units, f64::from(vp.zoom), GRID_MIN_PX);
        let major = snap::major_every(page.units, step);
        let (minor_c, major_c) = (t.accent.gamma_multiply(0.10), t.accent.gamma_multiply(0.24));
        grid_lines(painter, vp, true, x0, x1, step, major, (a.y, b.y), minor_c, major_c);
        grid_lines(painter, vp, false, y0, y1, step, major, (a.x, b.x), minor_c, major_c);
    }
    if aids.page_breaks && !infinite {
        let (pw, ph) = paper(page);
        let c = t.accent.gamma_multiply(0.7);
        let dashed = |p0: Pos2, p1: Pos2| painter.extend(egui::Shape::dashed_line(&[p0, p1], Stroke::new(1.0, c), 6.0, 4.0));
        for k in 1..1000 {
            let x = pw * f64::from(k);
            if x >= pr.x1 {
                break;
            }
            let sx = vp.world_to_screen(pos(dg::Point::new(x, 0.0))).x;
            dashed(egui::pos2(sx, a.y), egui::pos2(sx, b.y));
        }
        for k in 1..1000 {
            let y = ph * f64::from(k);
            if y >= pr.y1 {
                break;
            }
            let sy = vp.world_to_screen(pos(dg::Point::new(0.0, y))).y;
            dashed(egui::pos2(a.x, sy), egui::pos2(b.x, sy));
        }
    }
}

/// Guides across the canvas, the one being dragged where the pointer has it, and a new one
/// being pulled out of a ruler.
fn guides(app: &CraftApp, painter: &egui::Painter, vp: &Viewport2D, page: &Page, t: &Tokens) {
    if !app.session.view.aids.guides {
        return;
    }
    let drag = crate::canvas::rulers::guide_drag(painter.ctx());
    let r = vp.screen_rect;
    let line = |vertical: bool, at: f64, c: Color32| {
        let p = vp.world_to_screen(egui::pos2(at as f32, at as f32));
        let st = Stroke::new(1.0, c);
        if vertical {
            painter.line_segment([egui::pos2(p.x, r.min.y), egui::pos2(p.x, r.max.y)], st);
        } else {
            painter.line_segment([egui::pos2(r.min.x, p.y), egui::pos2(r.max.x, p.y)], st);
        }
    };
    for (i, g) in page.guides.iter().enumerate() {
        if drag.is_some_and(|d| d.index == Some(i)) {
            continue;
        }
        line(g.orientation == GuideOrientation::Vertical, g.position, t.accent.gamma_multiply(0.8));
    }
    // Off the canvas, a dragged guide is about to go: draw it faint where it was.
    if let Some(d) = drag {
        let c = if d.off { t.accent.gamma_multiply(0.3) } else { t.accent };
        if !(d.off && d.index.is_none()) {
            line(d.vertical, d.position, c);
        }
    }
}

/// Alignment lines while a move drag sits snapped to guides, shapes or connection points.
fn align_lines(app: &CraftApp, painter: &egui::Painter, vp: &Viewport2D, real: &Page, t: &Tokens) {
    let Some(interact::Drag::Move { ids, from, to }) = app.canvas.drag.as_ref() else { return };
    if painter.ctx().input(|i| i.modifiers.alt) {
        return;
    }
    // The preview moves by the rounded delta; look for what that position touches.
    let d = interact::drag::move_delta(*from, *to);
    let Some(s) = interact::drag::move_snap(real, ids, d, &app.session.view.aids, vp.zoom, Some(0.5)) else { return };
    let st = Stroke::new(1.0, t.red);
    for l in &s.lines {
        let (a, b) = if l.vertical {
            (dg::Point::new(l.at, l.from), dg::Point::new(l.at, l.to))
        } else {
            (dg::Point::new(l.from, l.at), dg::Point::new(l.to, l.at))
        };
        let (a, b) = (vp.world_to_screen(pos(a)), vp.world_to_screen(pos(b)));
        let ext = if l.vertical { vec2(0.0, 8.0) } else { vec2(8.0, 0.0) };
        painter.extend(egui::Shape::dashed_line(&[a - ext, b + ext], st, 4.0, 3.0));
    }
}

/// While a wire drag is in progress: the pin, junction or wire point it would end on, and the
/// alignment lines of its end (see `interact::wire::align_lines`): module crosses in the accent
/// colour, lines to the pins it lines up with in red, as a move draws them.
fn wire_overlay(app: &CraftApp, painter: &egui::Painter, vp: &Viewport2D, real: &Page, t: &Tokens) {
    let Some(Drag::Wire { from, to, .. }) = app.canvas.drag.as_ref() else { return };
    for l in interact::wire::align_lines(real, from, *to, vp.zoom) {
        let (a, b) = if l.vertical {
            (dg::Point::new(l.at, l.from), dg::Point::new(l.at, l.to))
        } else {
            (dg::Point::new(l.from, l.at), dg::Point::new(l.to, l.at))
        };
        let (a, b) = (vp.world_to_screen(pos(a)), vp.world_to_screen(pos(b)));
        let colour = if l.kind == snap::Kind::Module { t.accent.gamma_multiply(0.7) } else { t.red };
        painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, colour), 4.0, 3.0));
    }
    for end in [from.clone(), interact::wire::target(real, *to, vp.zoom)] {
        match &end {
            interact::wire::WireEnd::Pin(s, i) => {
                if let Some(s) = real.shape(s) {
                    connection_marks(painter, vp, s, t.accent, Some(*i));
                }
            }
            interact::wire::WireEnd::Junction(_) | interact::wire::WireEnd::OnWire(..) => {
                if let Some(p) = end.pos(real) {
                    painter.circle_stroke(vp.world_to_screen(pos(p)), 6.0, Stroke::new(2.0, t.accent));
                }
            }
            interact::wire::WireEnd::Free(_) => {}
        }
    }
}

/// Selection, handles, hover, connection points, connector targets and the marquee.
pub fn overlay(app: &CraftApp, painter: &egui::Painter, vp: &Viewport2D, page: &Page, real: &Page, t: &Tokens) {
    let accent = t.accent;
    let sel = &app.session.sel.items;
    let drag = app.canvas.drag.as_ref();
    guides(app, painter, vp, page, t);
    align_lines(app, painter, vp, real, t);
    wire_overlay(app, painter, vp, real, t);
    // With Connection Points on, every visible shape shows its points, faintly.
    if app.session.view.aids.connection_points {
        let visible = vp.visible_world_rect();
        for s in page.shapes.iter().filter(|s| page.shape_visible(s) && s.bounds().intersects(&visible)) {
            connection_marks(painter, vp, s, accent.gamma_multiply(0.55), None);
        }
    }
    // Hover outline on a shape that is not selected.
    if drag.is_none()
        && let Some(Hit::Shape(id)) = &app.canvas.hover
        && !sel.contains(id)
        && let Some(s) = page.shape(id)
    {
        painter.add(egui::Shape::closed_line(frame_screen(vp, s), Stroke::new(1.0, accent.gamma_multiply(0.6))));
    }
    // Connection points: on the shape under the pointer, and on a connector drag's target.
    let target = drag.and_then(|d| interact::drag_target(real, d, vp.zoom));
    let show_points = match (drag, app.tool) {
        (Some(Drag::Connect { .. } | Drag::Reglue { .. }), _) => target.as_ref().and_then(End::shape).map(str::to_string),
        (None, crate::Tool::Pointer | crate::Tool::Lasso | crate::Tool::Connector | crate::Tool::Wire { .. }) => app.canvas.hover_shape.clone(),
        _ => None,
    };
    if let Some(s) = show_points.as_deref().and_then(|id| page.shape(id)) {
        let hot = match &target {
            Some(End::Glued { point, .. }) => *point,
            _ => None,
        };
        if target.as_ref().is_some_and(|e| e.shape().is_some()) {
            painter.add(egui::Shape::closed_line(frame_screen(vp, s), Stroke::new(2.0, accent)));
        }
        connection_marks(painter, vp, s, accent, hot);
    }
    // Selected shapes and connectors.
    for id in sel {
        if let Some(s) = page.shape(id) {
            painter.add(egui::Shape::closed_line(frame_screen(vp, s), Stroke::new(1.0, accent)));
        } else if let Some(c) = page.connector(id) {
            let g = dg::connector_geometry(page, c);
            let pts: Vec<Pos2> = g.polyline.iter().map(|p| vp.world_to_screen(pos(*p))).collect();
            painter.add(egui::Shape::line(pts, Stroke::new(3.0, accent.gamma_multiply(0.45))));
            for (p, glued) in [(g.ends.start.pos, g.ends.start.shape.is_some()), (g.ends.end.pos, g.ends.end.shape.is_some())] {
                let c = vp.world_to_screen(pos(p));
                painter.circle(c, 4.5, if glued { accent } else { t.page }, Stroke::new(1.5, accent));
            }
        }
    }
    interact::text_block::overlay(app, painter, vp, page, t);
    if let Some(s) = interact::single_shape(app, page)
        && !matches!(drag, Some(Drag::Move { .. }))
        && app.tool != crate::Tool::TextBlock
    {
        // Locks hide the handles they stop; document protection hides them all.
        let protected = interact::drag::protected(&app.session.doc, page);
        if !protected && interact::drag::rotate_allowed(s) {
            let top = vp.world_to_screen(pos(s.transform().apply(dg::Point::new(s.w / 2.0, 0.0))));
            let rot = vp.world_to_screen(pos(interact::rotate_handle_world(s, vp.zoom)));
            painter.line_segment([top, rot], Stroke::new(1.0, accent));
            painter.circle(rot, 4.5, accent, Stroke::new(1.5, t.page));
        }
        for i in (0..interact::HANDLES.len()).filter(|i| !protected && interact::drag::handle_allowed(s, *i)) {
            if let Some(h) = interact::handle_world(s, i) {
                let c = vp.world_to_screen(pos(h));
                let r = Rect::from_center_size(c, vec2(7.0, 7.0));
                painter.rect(r, 1.0, t.page, Stroke::new(1.0, accent), egui::StrokeKind::Middle);
            }
        }
    }
    if let Some(Drag::Lasso { points, .. }) = drag
        && points.len() > 1
    {
        let pts: Vec<Pos2> = points.iter().map(|p| vp.world_to_screen(pos(*p))).collect();
        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, accent)));
    }
    if let Some(r) = drag.and_then(interact::marquee_rect) {
        let r = Rect::from_two_pos(vp.world_to_screen(pos(dg::Point::new(r.x0, r.y0))), vp.world_to_screen(pos(dg::Point::new(r.x1, r.y1))));
        painter.rect(r, 0.0, t.selection.gamma_multiply(0.5), Stroke::new(1.0, accent), egui::StrokeKind::Inside);
    }
}
