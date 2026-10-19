//! Text on the page: fonts for a style at a zoom, the character format (italic, underline,
//! strikethrough, highlight, colour), and shape text laid out in its text block.
//!
//! Shape text follows the SVG export: the block is the shape's frame or its stored text block
//! (offset, size, own angle), inset by the margins and the indent, aligned left, centre, right or
//! justified and top, middle or bottom, with letter case and bullets applied to the drawn lines,
//! rotated with the shape. Right-to-left text mirrors the alignment and puts the indent on the
//! right (there is no bidirectional shaping); vertical text is the same layout turned a quarter
//! turn clockwise, so lines run top to bottom and follow each other right to left. Fonts: a family
//! that names a monospace face draws in JetBrains Mono, anything else in Inter (the only faces the
//! app ships); exports name the family itself.
//!
//! Compartments (see `vizcraft_diagram::compartment`) draw in the bands its layout gives, like
//! the SVG export: the title bold and centred, a paragraph from the top of its band, a list one
//! row per row rectangle, each in the shape's style with the compartment's overrides; the shape's
//! main text sits in the band above them.

use std::sync::Arc;

use egui::epaint::TextShape;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, Painter, Pos2, Stroke, Vec2, pos2, vec2};
use vizcraft_diagram::compartment::{self as cp, CompartmentKind};
use vizcraft_diagram::{HAlign, Rect as DRect, Shape, Style, TextDirection, VAlign};

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
    let px = font_px(size, zoom)?;
    Some(if bold { semibold(px) } else { regular(px) })
}

/// Screen size of a font size at this zoom, in half-point steps; `None` when too small to read.
fn font_px(size: f64, zoom: f32) -> Option<f32> {
    let size = if size.is_finite() { size.clamp(1.0, 400.0) } else { 12.0 };
    let px = (size as f32) * zoom;
    // Half-point steps keep the glyph atlas from filling with one size per zoom step.
    let px = (px * 2.0).round() / 2.0;
    if px < MIN_TEXT_PX { None } else { Some(px) }
}

/// Whether a font family name asks for a monospace face.
pub fn is_mono(family: &str) -> bool {
    let f = family.to_ascii_lowercase();
    ["mono", "courier", "consolas", "menlo", "code", "typewriter"].iter().any(|k| f.contains(k))
}

/// The font a style's text draws in at this zoom (family, size, weight), or `None` when too
/// small to read.
pub fn style_font(st: &Style, zoom: f32) -> Option<egui::FontId> {
    if is_mono(&st.font_family) {
        return font_px(st.font_size, zoom).map(egui::FontId::monospace);
    }
    font_for(st.font_size, st.bold, zoom)
}

/// The character format of a style's text in `font`: colour, italic, underline, strikethrough
/// and highlight.
pub fn text_format(st: &Style, font: egui::FontId) -> TextFormat {
    let colour = color(st.font_color);
    let line = Stroke::new((font.size / 14.0).max(1.0), colour);
    TextFormat {
        font_id: font,
        color: colour,
        italics: st.italic,
        underline: if st.underline { line } else { Stroke::NONE },
        strikethrough: if st.strikethrough { line } else { Stroke::NONE },
        background: if st.highlight.a > 0 { color(st.highlight) } else { Color32::TRANSPARENT },
        ..Default::default()
    }
}

/// A connector label: the style's format, centred, wrapped to `wrap` screen points.
pub(in crate::canvas) fn label_layout(painter: &Painter, text: &str, st: &Style, font: egui::FontId, wrap: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.to_string(), text_format(st, font));
    job.wrap.max_width = wrap.max(1.0);
    job.halign = Align::Center;
    painter.layout_job(job)
}

/// Where a shape's text goes on screen: the text block's centre, the total rotation, and the
/// area the text is laid out in, relative to the centre in unrotated screen points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    /// Centre of the text block on screen.
    pub centre: Pos2,
    /// Rotation in radians clockwise: the shape's, the block's own, and a quarter turn for
    /// vertical text.
    pub angle: f32,
    /// The layout area relative to `centre`, before rotation (margins and indent taken off).
    pub inner: egui::Rect,
    /// Horizontal alignment after right-to-left mirroring.
    pub h_align: HAlign,
    /// Vertical alignment.
    pub v_align: VAlign,
}

impl Block {
    /// A point given relative to the centre in unrotated block coordinates, on screen.
    pub fn to_screen(&self, rel: Vec2) -> Pos2 {
        self.centre + egui::emath::Rot2::from_angle(self.angle) * rel
    }
    /// The screen position the top-left of `galley` goes at (before rotation, relative to the
    /// centre) so it sits aligned in [`Block::inner`].
    pub fn galley_offset(&self, galley: &egui::Galley) -> Vec2 {
        let (r, g) = (self.inner, galley.rect);
        let x = match self.h_align {
            HAlign::Left | HAlign::Justify => r.left() - g.left(),
            HAlign::Center => r.center().x - g.center().x,
            HAlign::Right => r.right() - g.right(),
        };
        let y = match self.v_align {
            VAlign::Top => r.top() - g.top(),
            VAlign::Middle => r.center().y - g.center().y,
            VAlign::Bottom => r.bottom() - g.bottom(),
        };
        vec2(x, y)
    }
    /// The egui alignment rows take inside the wrap width.
    pub fn row_align(&self) -> Align {
        match self.h_align {
            HAlign::Left | HAlign::Justify => Align::LEFT,
            HAlign::Center => Align::Center,
            HAlign::Right => Align::RIGHT,
        }
    }
}

/// The text block of `s` on screen (see [`Block`]).
pub fn block(vp: &Viewport2D, s: &Shape) -> Block {
    let st = &s.style;
    let (local, block_deg) = s.text_rect();
    let centre = vp.world_to_screen(pos(s.transform().apply(local.center())));
    let z = vp.zoom;
    let (hw, hh) = ((local.width() / 2.0) as f32 * z, (local.height() / 2.0) as f32 * z);
    let m = st.text_margins;
    let indent = st.indent as f32 * z;
    let rtl = st.text_direction == TextDirection::RightToLeft;
    let (il, ir) = if rtl { (0.0, indent) } else { (indent, 0.0) };
    let (x0, x1) = (-hw + m.left as f32 * z + il, hw - m.right as f32 * z - ir);
    let (y0, y1) = (-hh + m.top as f32 * z, hh - m.bottom as f32 * z);
    // Margins larger than the block leave an empty area at the block centre, not an inside-out one.
    let fix = |a: f32, b: f32| if a <= b { (a, b) } else { ((a + b) / 2.0, (a + b) / 2.0) };
    let ((x0, x1), (y0, y1)) = (fix(x0, x1), fix(y0, y1));
    let mut angle = ((s.angle_deg + block_deg) as f32).to_radians();
    let inner = if st.text_direction == TextDirection::Vertical {
        // Laid out across the block's height, then turned a quarter turn clockwise.
        angle += std::f32::consts::FRAC_PI_2;
        egui::Rect::from_min_max(pos2(y0, -x1), pos2(y1, -x0))
    } else {
        egui::Rect::from_min_max(pos2(x0, y0), pos2(x1, y1))
    };
    let h_align = match (st.h_align, rtl) {
        (HAlign::Left, true) => HAlign::Right,
        (HAlign::Right, true) => HAlign::Left,
        (a, _) => a,
    };
    Block { centre, angle, inner, h_align, v_align: st.v_align }
}

/// The lines a shape's text draws as: letter case and bullets applied.
pub fn drawn_lines(text: &str, st: &Style) -> Vec<String> {
    text.lines().enumerate().map(|(i, l)| format!("{}{}", st.bullets.marker(i), st.text_case.apply(l))).collect()
}

/// A layout job for `text` in `st`'s format, wrapped to the block's width.
pub fn block_job(text: String, st: &Style, font: egui::FontId, b: &Block) -> LayoutJob {
    let mut job = LayoutJob::single_section(text, text_format(st, font));
    job.wrap.max_width = b.inner.width().max(1.0);
    job.halign = b.row_align();
    job.justify = st.h_align == HAlign::Justify;
    job
}

/// Paint `galley` aligned in block `b`, rotated with it.
pub fn paint_in_block(painter: &Painter, b: &Block, galley: Arc<egui::Galley>, fallback: Color32) {
    let at = b.to_screen(b.galley_offset(&galley));
    let text = TextShape::new(at, galley, fallback);
    painter.add(if b.angle.abs() > 1e-4 { text.with_angle(b.angle) } else { text });
}

/// Shape text in its text block (see the module doc). Fields are expanded by the caller.
pub(super) fn shape_text(painter: &Painter, vp: &Viewport2D, s: &Shape) {
    let st = &s.style;
    let Some(font) = style_font(st, vp.zoom) else { return };
    let b = block(vp, s);
    let galley = painter.layout_job(block_job(drawn_lines(&s.text, st).join("\n"), st, font, &b));
    paint_in_block(painter, &b, galley, color(st.font_color));
}

/// The block a band area (`local`, in the shape's unrotated frame) lays text out in on screen:
/// the area inset by the band padding left and right, turned with the shape.
pub fn band_block(vp: &Viewport2D, s: &Shape, local: DRect, h_align: HAlign, v_align: VAlign) -> Block {
    let centre = vp.world_to_screen(pos(s.transform().apply(local.center())));
    let z = vp.zoom;
    let (hw, hh) = ((local.width() / 2.0) as f32 * z, (local.height() / 2.0) as f32 * z);
    let pad = (cp::PAD_X as f32 * z).min(hw);
    let inner = egui::Rect::from_min_max(pos2(-hw + pad, -hh), pos2(hw - pad, hh));
    Block { centre, angle: (s.angle_deg as f32).to_radians(), inner, h_align, v_align }
}

/// One line or paragraph of compartment text in block `b`.
fn band_text(painter: &Painter, vp: &Viewport2D, text: &str, st: &Style, b: &Block) {
    if text.trim().is_empty() {
        return;
    }
    let Some(font) = style_font(st, vp.zoom) else { return };
    let galley = painter.layout_job(block_job(text.to_string(), st, font, b));
    paint_in_block(painter, b, galley, color(st.font_color));
}

/// The text of every compartment of `s`: titles, paragraphs and rows (see the module doc).
/// `skip` names a compartment whose text the in-place editor is showing instead.
pub(super) fn compartment_text(painter: &Painter, vp: &Viewport2D, s: &Shape, skip: Option<&str>) {
    for band in cp::layout(s) {
        let Some(c) = band.compartment.and_then(|i| s.compartments.get(i)) else { continue };
        let st = c.text_style(&s.style);
        if let (Some(title), Some(r)) = (c.title.as_deref(), band.title) {
            let bold = Style { bold: true, ..st.clone() };
            band_text(painter, vp, title, &bold, &band_block(vp, s, r, HAlign::Center, VAlign::Middle));
        }
        if skip == Some(c.id.as_str()) {
            continue;
        }
        match c.kind {
            CompartmentKind::Paragraph => {
                // Only the lines that fit the band, as in the SVG export.
                let lh = st.font_size * cp::LINE_HEIGHT;
                let fit = ((band.body.height() + 0.5) / lh).floor().max(1.0) as usize;
                let text: Vec<&str> = c.text.lines().take(fit).collect();
                band_text(painter, vp, &text.join("\n"), &st, &band_block(vp, s, band.body, c.align, VAlign::Top));
            }
            CompartmentKind::List if !band.columns.is_empty() => table_text(painter, vp, s, c, &band, &st),
            CompartmentKind::List => {
                for (row, r) in c.rows.iter().zip(&band.rows) {
                    band_text(painter, vp, row, &st, &band_block(vp, s, *r, c.align, VAlign::Middle));
                }
            }
            CompartmentKind::Slot => {}
        }
    }
}

/// A table band's header titles (bold) and each row's tab-separated cells, left-aligned in their
/// columns and cut to the column's width (`fit_cell`), as the SVG export writes them. Each cell is
/// also clipped to its column, so a real glyph wider than the estimate never spills over.
fn table_text(painter: &Painter, vp: &Viewport2D, s: &Shape, c: &cp::Compartment, band: &cp::Band, st: &Style) {
    let n = c.columns.len();
    let spans: Vec<(f64, f64)> = band.columns.windows(2).map(|w| (w.first().copied().unwrap_or(0.0), w.get(1).copied().unwrap_or(0.0))).collect();
    let line = |r: DRect, texts: Vec<&str>, style: &Style| {
        for ((x0, x1), t) in spans.iter().zip(texts) {
            let t = cp::fit_cell(t, x1 - x0 - 2.0 * cp::PAD_X, style.font_size);
            let cell = DRect::new(*x0, r.y0, *x1, r.y1);
            let tf = s.transform();
            let corners = [(cell.x0, cell.y0), (cell.x1, cell.y0), (cell.x1, cell.y1), (cell.x0, cell.y1)]
                .map(|(x, y)| vp.world_to_screen(pos(tf.apply(vizcraft_diagram::Point::new(x, y)))));
            let clip = egui::Rect::from_points(&corners).intersect(painter.clip_rect());
            band_text(&painter.with_clip_rect(clip), vp, &t, style, &band_block(vp, s, cell, HAlign::Left, VAlign::Middle));
        }
    };
    if let Some(h) = band.header {
        let bold = Style { bold: true, ..st.clone() };
        line(h, c.columns.iter().map(|c| c.title.as_str()).collect(), &bold);
    }
    for (row, r) in c.rows.iter().zip(&band.rows) {
        line(*r, cp::cells(row, n), st);
    }
}

/// The text slots of a shape drawn by a glyph builder (`Shape.glyph.labels`): each at its point
/// in the shape's frame carried by the shape's transform (quarter turns and mirror included),
/// the text itself upright, in the shape's font colour. The same rule as the SVG export's
/// `glyph_labels::anchor`.
pub(super) fn glyph_labels(painter: &Painter, vp: &Viewport2D, s: &Shape) {
    let Some(g) = &s.glyph else { return };
    let t = s.transform();
    for l in g.labels.iter().filter(|l| !l.text.trim().is_empty()) {
        let at = t.apply(vizcraft_diagram::Point::new(l.x * s.w, l.y * s.h));
        if !at.is_finite() {
            continue;
        }
        let st = Style { font_size: l.size, font_color: s.style.font_color, ..Style::DEFAULT };
        let Some(font) = style_font(&st, vp.zoom) else { continue };
        let galley = painter.layout_no_wrap(l.text.clone(), font, color(st.font_color));
        let size = galley.size();
        let p = vp.world_to_screen(pos(at));
        // The slot's y is the middle of the text; x is its start, middle or end.
        let x = match l.anchor.as_str() {
            "start" => p.x,
            "end" => p.x - size.x,
            _ => p.x - size.x / 2.0,
        };
        painter.galley(pos2(x, p.y - size.y / 2.0), galley, color(st.font_color));
    }
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::{Margins, ShapeKind, TextBlock};

    use super::*;

    #[test]
    fn fonts_vanish_when_tiny_and_snap_to_half_points() {
        assert!(font_for(12.0, false, 0.1).is_none());
        assert_eq!(font_for(12.0, false, 1.04).map(|f| f.size), Some(12.5));
        assert!(font_for(f64::NAN, true, 1.0).is_some());
        let mono = Style { font_family: "Courier New".into(), ..Style::DEFAULT };
        assert_eq!(style_font(&mono, 1.0).map(|f| f.family), Some(egui::FontFamily::Monospace));
        assert!(!is_mono("Georgia"));
    }

    fn vp() -> Viewport2D {
        // World origin at the screen origin, 100% zoom.
        Viewport2D::new(egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 200.0)), [100.0, 100.0], 1.0)
    }

    #[test]
    fn the_block_follows_margins_indent_direction_and_the_text_block() {
        let mut s = Shape::new("s", ShapeKind::Rectangle, 10.0, 20.0, 100.0, 60.0);
        s.style.text_margins = Margins { left: 5.0, top: 2.0, right: 3.0, bottom: 4.0 };
        s.style.indent = 10.0;
        let b = block(&vp(), &s);
        assert_eq!(b.centre, pos2(60.0, 50.0));
        assert_eq!(b.inner, egui::Rect::from_min_max(pos2(-35.0, -28.0), pos2(47.0, 26.0)));
        s.style.text_direction = TextDirection::RightToLeft;
        s.style.h_align = HAlign::Left;
        let b = block(&vp(), &s);
        assert_eq!((b.inner.left(), b.inner.right(), b.h_align), (-45.0, 37.0, HAlign::Right));
        s.style.text_direction = TextDirection::Vertical;
        let b = block(&vp(), &s);
        assert!((b.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(b.inner, egui::Rect::from_min_max(pos2(-28.0, -47.0), pos2(26.0, 35.0)));
        // A rotated shape with an offset text block: the centre turns with the shape.
        s.style.text_direction = TextDirection::LeftToRight;
        s.angle_deg = 90.0;
        s.text_block = Some(TextBlock { dx: 20.0, dy: 0.0, w: 40.0, h: 20.0, angle_deg: 0.0 });
        let b = block(&vp(), &s);
        assert!((b.centre - pos2(60.0, 70.0)).length() < 1e-3, "{:?}", b.centre);
    }

    #[test]
    fn a_band_block_sits_in_its_band_and_turns_with_the_shape() {
        let mut s = Shape::new("s", ShapeKind::Rectangle, 10.0, 20.0, 100.0, 60.0);
        let band = vizcraft_diagram::Rect::new(0.0, 40.0, 100.0, 60.0);
        let b = band_block(&vp(), &s, band, HAlign::Left, VAlign::Top);
        assert_eq!(b.centre, pos2(60.0, 70.0));
        assert_eq!(b.inner, egui::Rect::from_min_max(pos2(-44.0, -10.0), pos2(44.0, 10.0)));
        s.angle_deg = 90.0;
        let b = band_block(&vp(), &s, band, HAlign::Left, VAlign::Top);
        assert!((b.centre - pos2(40.0, 50.0)).length() < 1e-3, "{:?}", b.centre);
        assert!((b.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn galleys_align_in_the_block() {
        let b = Block {
            centre: pos2(0.0, 0.0),
            angle: 0.0,
            inner: egui::Rect::from_min_max(pos2(-50.0, -20.0), pos2(50.0, 20.0)),
            h_align: HAlign::Right,
            v_align: VAlign::Bottom,
        };
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let st = Style::DEFAULT;
            let galley = ui.painter().layout_job(block_job("ab".into(), &st, regular(12.0), &b));
            let off = b.galley_offset(&galley);
            let r = galley.rect.translate(off);
            assert!((r.right() - 50.0).abs() < 1e-3 && (r.bottom() - 20.0).abs() < 1e-3, "{r:?}");
        });
        // The font atlas update is not painted anywhere in a test.
        out.textures_delta.clear();
        let lines = drawn_lines(
            "one\ntwo",
            &Style { bullets: vizcraft_diagram::Bullets::Number, text_case: vizcraft_diagram::TextCase::Upper, ..Style::DEFAULT },
        );
        assert_eq!(lines, vec!["1. ONE".to_string(), "2. TWO".to_string()]);
    }
}
