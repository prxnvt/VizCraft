//! Drawing what containers, lane sets and callouts add to an ordinary shape: the heading band or
//! separator, a lane set's phase band, separators and titles, and callout leader lines (drawn
//! just before their callout, so they show over the shapes drawn earlier). The geometry comes
//! from `vizcraft_diagram::container` as [`Decor`] items, which SVG export draws the same way.

use egui::epaint::{PathShape, TextShape};
use egui::{Align2, Painter, Pos2, Stroke};
use vizcraft_diagram::container::{self as ct, Decor};
use vizcraft_diagram::{Page, Shape};

use super::text::font_for;
use super::{color, line_px, pos};
use crate::canvas::Viewport2D;
use crate::canvas::tess;

/// Dash and gap of a dashed decoration line, in world units.
const DASH: [f32; 2] = [5.0, 4.0];

/// The decorations of `s` (none for an ordinary shape), drawn over its outline and under its text.
pub fn decor(painter: &Painter, vp: &Viewport2D, s: &Shape) {
    if s.container.is_none() {
        return;
    }
    for d in ct::decor(s) {
        item(painter, vp, &d);
    }
}

/// The leader of callout `s` on `page` (nothing for other shapes), drawn just before the callout.
pub fn leader(painter: &Painter, vp: &Viewport2D, page: &Page, s: &Shape) {
    if !ct::is_callout(s) {
        return;
    }
    for d in ct::leader_decor(page, s) {
        item(painter, vp, &d);
    }
}

fn item(painter: &Painter, vp: &Viewport2D, d: &Decor) {
    match d {
        Decor::Fill { points, color: c } => {
            let c = color(*c);
            if c.a() == 0 || points.len() < 3 {
                return;
            }
            let pts: Vec<Pos2> = points.iter().map(|p| vp.world_to_screen(pos(*p))).collect();
            painter.add(PathShape::convex_polygon(pts, c, Stroke::NONE));
        }
        Decor::Line { points, color: c, width, dashed } => {
            let stroke = Stroke::new(line_px(*width, vp.zoom), color(*c));
            if stroke.width <= 0.0 || stroke.color.a() == 0 {
                return;
            }
            let world: Vec<Pos2> = points.iter().map(|p| pos(*p)).collect();
            let pieces = if *dashed { tess::dashes(&world, false, &DASH) } else { vec![world] };
            for piece in pieces.into_iter().filter(|p| p.len() >= 2) {
                painter.add(PathShape::line(piece.iter().map(|p| vp.world_to_screen(*p)).collect(), stroke));
            }
        }
        Decor::Text { center, room, angle, text, size, color: c, bold } => {
            let Some(font) = font_for(*size, *bold, vp.zoom) else { return };
            let wrap = (*room as f32 * vp.zoom).max(1.0);
            let galley = layout(painter, text, font, color(*c), false, wrap);
            let at = vp.world_to_screen(pos(*center)) - galley.rect.center().to_vec2();
            let a = (*angle as f32).to_radians();
            let shape = TextShape::new(at, galley, color(*c));
            painter.add(if a.abs() > 1e-4 { shape.with_angle_and_anchor(a, Align2::CENTER_CENTER) } else { shape });
        }
    }
}

/// A centred, wrapped single-style galley for headings and callout text.
fn layout(painter: &Painter, text: &str, font: egui::FontId, colour: egui::Color32, italic: bool, wrap: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat { font_id: font, color: colour, italics: italic, ..Default::default() },
    );
    job.wrap.max_width = wrap.max(1.0);
    job.halign = egui::Align::Center;
    painter.layout_job(job)
}
