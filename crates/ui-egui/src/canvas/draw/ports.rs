//! Port marks on typed shapes: the ports of each shape's element type at its connection points
//! (`vizcraft_stencils::port_marks`), a filled square for `in`, a hollow one for `out` and a
//! half-filled one for `inOut`, in the shape's line colour. Labels show from 100 percent zoom,
//! where they are readable; SVG export draws the same marks.

use egui::{Align2, Color32, CornerRadius, FontId, Painter, Rect, Stroke, StrokeKind, pos2};
use vizcraft_diagram::{Document, Page};
use vizcraft_stencils::ports::PortDirection;

use super::{color, line_px, pos};
use crate::canvas::Viewport2D;

/// Draw the port marks of every visible typed shape on `page` that meets `visible`.
pub fn draw(painter: &Painter, vp: &Viewport2D, doc: &Document, page: &Page, visible: &vizcraft_diagram::Rect) {
    for s in page.shapes.iter().filter(|s| vizcraft_diagram::layer::visible(page, s) && s.bounds().intersects(visible)) {
        let Some(t) = s.element.as_deref().and_then(|e| doc.model.element(e)).map(|e| e.type_id.as_str()) else { continue };
        let line = color(s.style.effective_line());
        let stroke = Stroke::new(line_px(s.style.line_width.clamp(0.5, 4.0), vp.zoom).max(1.0), line);
        for m in vizcraft_stencils::port_marks(s, t) {
            let corner = |x: f64, y: f64| vp.world_to_screen(pos(vizcraft_diagram::Point::new(x, y)));
            let r = Rect::from_min_max(corner(m.rect.x0, m.rect.y0), corner(m.rect.x1, m.rect.y1));
            let fill = if m.direction == PortDirection::In { line } else { Color32::WHITE };
            painter.rect_filled(r, CornerRadius::ZERO, fill);
            if m.direction == PortDirection::InOut {
                painter.rect_filled(Rect::from_min_max(r.min, pos2(r.center().x, r.max.y)), CornerRadius::ZERO, line);
            }
            painter.rect_stroke(r, CornerRadius::ZERO, stroke, StrokeKind::Middle);
            if vp.zoom >= 1.0 && !m.label.trim().is_empty() {
                let size = (r.height() * 1.1).clamp(8.0, 16.0);
                painter.text(pos2(r.max.x + 2.0 * vp.zoom, r.center().y), Align2::LEFT_CENTER, m.label.trim(), FontId::proportional(size), line);
            }
        }
    }
}
