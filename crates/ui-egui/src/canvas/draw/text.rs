//! Text on the page: fonts for a style at a zoom, centred wrapped layout, and shape text.

use std::sync::Arc;

use egui::epaint::TextShape;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Align2, Color32, Painter};
use vizcraft_diagram::Shape;

use super::{color, pos};
use crate::canvas::Viewport2D;
use crate::theme::{regular, semibold};

/// Smallest on-screen font size worth laying out; below it text is unreadable and layout cost
/// dominates when zoomed far out.
const MIN_TEXT_PX: f32 = 3.0;

/// Inner padding between a shape's frame and its text, in world units.
pub const TEXT_PAD: f64 = 6.0;

/// The font for a style at this zoom, or `None` when too small to read.
pub fn font_for(size: f64, bold: bool, zoom: f32) -> Option<egui::FontId> {
    let size = if size.is_finite() { size.clamp(1.0, 400.0) } else { 12.0 };
    let px = (size as f32) * zoom;
    // Half-point steps keep the glyph atlas from filling with one size per zoom step.
    let px = (px * 2.0).round() / 2.0;
    if px < MIN_TEXT_PX {
        return None;
    }
    Some(if bold { semibold(px) } else { regular(px) })
}

/// Lay out text centred, wrapped to `wrap` screen points.
pub(super) fn layout(painter: &Painter, text: &str, font: egui::FontId, colour: Color32, italic: bool, wrap: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat { font_id: font, color: colour, italics: italic, ..Default::default() });
    job.wrap.max_width = wrap.max(1.0);
    job.halign = Align::Center;
    painter.layout_job(job)
}

/// Shape text: centred in its text block (the frame unless the shape has one), wrapped to the
/// block's width, rotated with the shape and the block.
pub(super) fn shape_text(painter: &Painter, vp: &Viewport2D, s: &Shape) {
    let st = &s.style;
    let Some(font) = font_for(st.font_size, st.bold, vp.zoom) else { return };
    let (block, block_angle) = s.text_rect();
    let wrap = ((block.width() - 2.0 * TEXT_PAD).max(1.0) as f32) * vp.zoom;
    let galley = layout(painter, &s.text, font, color(st.font_color), st.italic, wrap);
    let centre = vp.world_to_screen(pos(s.transform().apply(block.center())));
    let at = centre - galley.rect.center().to_vec2();
    let angle = ((s.angle_deg + block_angle) as f32).to_radians();
    let text = TextShape::new(at, galley, color(st.font_color));
    painter.add(if angle.abs() > 1e-4 { text.with_angle_and_anchor(angle, Align2::CENTER_CENTER) } else { text });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_vanish_when_tiny_and_snap_to_half_points() {
        assert!(font_for(12.0, false, 0.1).is_none());
        assert_eq!(font_for(12.0, false, 1.04).map(|f| f.size), Some(12.5));
        assert!(font_for(f64::NAN, true, 1.0).is_some());
    }
}
