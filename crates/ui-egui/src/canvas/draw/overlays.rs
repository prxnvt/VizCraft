//! Selection chrome over the page: hover outline, connection points, connector targets,
//! selected items, resize and rotate handles, and the marquee. Colours come from
//! `theme::Tokens`.

use egui::{Color32, Pos2, Rect, Stroke, vec2};
use vizcraft_diagram::{self as dg, End, Hit, Page};

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

/// Selection, handles, hover, connection points, connector targets and the marquee.
pub fn overlay(app: &CraftApp, painter: &egui::Painter, vp: &Viewport2D, page: &Page, real: &Page, t: &Tokens) {
    let accent = t.accent;
    let sel = &app.session.sel.items;
    let drag = app.canvas.drag.as_ref();
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
        (None, crate::Tool::Pointer | crate::Tool::Connector) => app.canvas.hover_shape.clone(),
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
    if let Some(s) = interact::single_shape(app, page)
        && !matches!(drag, Some(Drag::Move { .. }))
    {
        let top = vp.world_to_screen(pos(s.transform().apply(dg::Point::new(s.w / 2.0, 0.0))));
        let rot = vp.world_to_screen(pos(interact::rotate_handle_world(s, vp.zoom)));
        painter.line_segment([top, rot], Stroke::new(1.0, accent));
        painter.circle(rot, 4.5, accent, Stroke::new(1.5, t.page));
        for i in 0..interact::HANDLES.len() {
            if let Some(h) = interact::handle_world(s, i) {
                let c = vp.world_to_screen(pos(h));
                let r = Rect::from_center_size(c, vec2(7.0, 7.0));
                painter.rect(r, 1.0, t.page, Stroke::new(1.0, accent), egui::StrokeKind::Middle);
            }
        }
    }
    if let Some(r) = drag.and_then(interact::marquee_rect) {
        let r = Rect::from_two_pos(vp.world_to_screen(pos(dg::Point::new(r.x0, r.y0))), vp.world_to_screen(pos(dg::Point::new(r.x1, r.y1))));
        painter.rect(r, 0.0, t.selection.gamma_multiply(0.5), Stroke::new(1.0, accent), egui::StrokeKind::Inside);
    }
}
