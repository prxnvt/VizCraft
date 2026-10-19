//! Link glyphs and comment markers: a small link badge at the top right of every shape that has
//! hyperlinks, and, while the Comments pane is open, a numbered marker at the anchor of each
//! comment on the page. Both are drawn above the shapes and are not part of the document or of
//! any export.

use egui::{Align2, FontId, Painter, Rect, Stroke, pos2, vec2};
use vizcraft_diagram::Page;

use super::super::Viewport2D;
use crate::CraftApp;
use crate::icons;
use crate::theme::Tokens;

/// Side of the link badge, in screen points.
const BADGE: f32 = 14.0;
/// Smallest shape width, in screen points, that still gets a badge (a smaller one would hide it).
const MIN_SHAPE_PX: f32 = 28.0;
/// Width and height of a comment marker, in screen points.
const MARKER: egui::Vec2 = egui::vec2(18.0, 14.0);

/// Draw the badges and, when the Comments pane is open, the comment markers.
pub fn draw(app: &CraftApp, painter: &Painter, vp: &Viewport2D, page: &Page, t: &Tokens) {
    let visible = vp.visible_world_rect().inflate(4.0);
    for s in page.shapes.iter().filter(|s| !s.links.is_empty()) {
        let b = s.bounds();
        if !b.intersects(&visible) || (b.width() as f32) * vp.zoom < MIN_SHAPE_PX {
            continue;
        }
        let corner = vp.world_to_screen(pos2(b.x1 as f32, b.y0 as f32));
        let r = Rect::from_center_size(corner + vec2(-BADGE / 2.0 - 2.0, BADGE / 2.0 + 2.0), vec2(BADGE, BADGE));
        painter.rect(r, 3.0, t.panel, Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
        icons::paint(painter, r.shrink(1.5), "link", t.accent, t.accent);
    }
    if !app.ui.panes.is_open("comments") {
        return;
    }
    for (r, _, number) in markers(page, &app.session.doc.comments, vp) {
        if !painter.clip_rect().intersects(r) {
            continue;
        }
        painter.rect_filled(r, 3.0, t.accent);
        painter.text(r.center(), Align2::CENTER_CENTER, number.to_string(), FontId::proportional(10.0), t.on_accent);
    }
}

/// The screen rectangle, comment id and number of each comment marker on `page`: at the top left
/// of the anchor shape, or of the page for a page comment, side by side where several share one.
/// The canvas draws these and `interact::links` hit-tests them, so the two always agree.
pub fn markers(page: &Page, comments: &[vizcraft_diagram::Comment], vp: &Viewport2D) -> Vec<(Rect, String, usize)> {
    let mut at_anchor: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for (number, c) in comments.iter().filter(|c| c.anchor.page == page.id).enumerate() {
        let anchor = c.anchor.shape.as_deref().and_then(|id| page.shape(id));
        let (key, origin) = match anchor {
            Some(sh) => (sh.id.clone(), vp.world_to_screen(pos2(sh.bounds().x0 as f32, sh.bounds().y0 as f32))),
            None => {
                let f = page.frame();
                (String::new(), vp.world_to_screen(pos2(f.x0 as f32, f.y0 as f32)))
            }
        };
        let k = at_anchor.entry(key).or_insert(0);
        let r = Rect::from_min_size(origin + vec2(2.0 + *k as f32 * (MARKER.x + 2.0), -MARKER.y - 2.0), MARKER);
        *k += 1;
        out.push((r, c.id.clone(), number + 1));
    }
    out
}

/// Ask the Comments pane to scroll to the thread of comment `id` when it next draws (a click on
/// the comment's marker does this). Kept in egui's memory, so it survives until the pane runs.
pub fn reveal(ctx: &egui::Context, id: &str) {
    ctx.memory_mut(|m| m.data.insert_temp(egui::Id::new("comments_reveal"), id.to_string()));
}

/// The comment the pane was asked to scroll to, if any.
pub fn revealing(ctx: &egui::Context) -> Option<String> {
    ctx.memory_mut(|m| m.data.get_temp::<String>(egui::Id::new("comments_reveal")))
}

/// The pane has scrolled to the thread: forget the request.
pub fn reveal_done(ctx: &egui::Context) {
    ctx.memory_mut(|m| m.data.remove::<String>(egui::Id::new("comments_reveal")));
}
