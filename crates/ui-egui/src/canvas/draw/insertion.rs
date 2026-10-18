//! Insertion chrome: the four handle arrows outside the hovered shape and, while the magic
//! connector is dragged, the ghost of the connector and of the shape it would drop (or the
//! outline of the shape it would connect to). Colours come from `theme::Tokens` only.
//!
//! The interaction runs before the page is painted, so this draws on its own layer above the
//! canvas, clipped to it, instead of in the canvas painter (which the shapes would cover).

use egui::{Pos2, Shape, Stroke, vec2};
use vizcraft_diagram::Point;
use vizcraft_engine::suggest::{self, Direction};

use super::pos;
use crate::CraftApp;
use crate::canvas::Viewport2D;
use crate::canvas::interact::insertion::{HANDLE_RADIUS, drop_target, handle_centre, screen_frame};
use crate::theme::Tokens;

/// An arrow head of `size` at `c` pointing `dir`.
fn arrow(c: Pos2, dir: Direction, size: f32) -> Vec<Pos2> {
    let (ux, uy) = dir.unit();
    let (ux, uy) = (ux as f32, uy as f32);
    // Tip ahead of the centre, base behind it, across the direction.
    let tip = c + vec2(ux, uy) * size;
    let back = c - vec2(ux, uy) * size * 0.6;
    let across = vec2(-uy, ux) * size * 0.9;
    vec![tip, back + across, back - across]
}

/// Paint the handles of the shown shape and the magic connector's ghost.
pub fn paint(app: &CraftApp, ctx: &egui::Context, vp: &Viewport2D) {
    let t = Tokens::get(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("insertion_overlay"))).with_clip_rect(vp.screen_rect);
    let hover = ctx.pointer_hover_pos();
    if let Some(id) = &app.insertion.shown
        && let Some(r) = screen_frame(app, vp, id)
    {
        for d in Direction::ALL {
            let c = handle_centre(r, d);
            let hot = hover.is_some_and(|p| p.distance(c) <= HANDLE_RADIUS);
            if hot {
                painter.circle_filled(c, HANDLE_RADIUS, t.hover);
            }
            let fill = if hot { t.accent } else { t.accent.gamma_multiply(0.75) };
            painter.add(Shape::convex_polygon(arrow(c, d, 6.0), fill, Stroke::NONE));
        }
    }
    let Some(d) = &app.insertion.drag else { return };
    let Some(page) = app.session.doc.page() else { return };
    let Some(src) = page.shape(&d.source) else { return };
    let from = pos(src.bounds().center());
    let to = pos(d.to);
    let line = Stroke::new(1.5, t.accent);
    painter.extend(Shape::dashed_line(&[vp.world_to_screen(from), vp.world_to_screen(to)], line, 6.0, 4.0));
    let tol = f64::from(crate::canvas::interact::GRAB / vp.zoom.max(0.01));
    if let Some(target) = drop_target(page, &d.source, d.to, tol)
        && let Some(r) = screen_frame(app, vp, &target)
    {
        painter.rect_stroke(r.expand(3.0), 4.0, Stroke::new(2.0, t.accent), egui::StrokeKind::Outside);
        return;
    }
    // The shape the drop would make, centred on the pointer, at its real size.
    let first = suggest::next(&app.session.doc, &d.source).into_iter().next();
    let (w, h) = first.and_then(|s| suggest::size_of(&s.type_id)).unwrap_or((src.w, src.h));
    let a = vp.world_to_screen(pos(Point::new(d.to.x - w / 2.0, d.to.y - h / 2.0)));
    let b = vp.world_to_screen(pos(Point::new(d.to.x + w / 2.0, d.to.y + h / 2.0)));
    let ghost = egui::Rect::from_two_pos(a, b);
    painter.rect_filled(ghost, 3.0, t.accent.gamma_multiply(0.12));
    painter.rect_stroke(ghost, 3.0, line, egui::StrokeKind::Middle);
}
