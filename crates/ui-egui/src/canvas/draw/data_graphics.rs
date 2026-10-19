//! Data graphics on the canvas (see `vizcraft_diagram::data_graphic`), from the same
//! [`render`] the SVG export writes: the shape drawn with its colour by value applied to a copy,
//! then its items (callouts, bars, icons) right above it.

use egui::epaint::PathShape;
use egui::{Align2, Painter, Stroke, pos2};
use vizcraft_diagram::data_graphic::{Mark, recolored, render};
use vizcraft_diagram::{Document, Page, Shape};

use super::{FillCache, color, font_for, line_px, pos, rect};
use crate::canvas::Viewport2D;

/// Draw `s` (as [`super::shape`] does) in the colours its data graphic gives it, and return the
/// graphic's marks (bars, icons, text) for [`marks`] to draw later: the canvas draws them in a
/// pass after the connectors, so a bar a connector crosses stays on top.
pub fn shape(painter: &Painter, vp: &Viewport2D, doc: &Document, page: &Page, s: &Shape, cache: &mut FillCache, show_text: bool) -> Vec<Mark> {
    let Some(r) = render(doc, page, s) else {
        super::shape(painter, vp, s, cache, show_text);
        return Vec::new();
    };
    super::shape(painter, vp, &recolored(s, &r), cache, show_text);
    r.items.into_iter().flat_map(|i| i.marks).collect()
}

/// Draw the marks [`shape`] returned.
pub fn marks(painter: &Painter, vp: &Viewport2D, marks: &[Mark]) {
    for m in marks {
        mark(painter, vp, m);
    }
}

fn mark(painter: &Painter, vp: &Viewport2D, m: &Mark) {
    let screen = |r: vizcraft_diagram::Rect| {
        let w = rect(r);
        egui::Rect::from_min_max(vp.world_to_screen(w.min), vp.world_to_screen(w.max))
    };
    match m {
        Mark::Rect { rect: r, fill, line, width } => {
            let sr = screen(*r);
            if let Some(f) = fill {
                painter.rect_filled(sr, 0.0, color(*f));
            }
            if let Some(l) = line
                && *width > 0.0
            {
                painter.rect_stroke(sr, 0.0, Stroke::new(line_px(*width, vp.zoom), color(*l)), egui::StrokeKind::Middle);
            }
        }
        Mark::Circle { center, radius, fill } => {
            painter.circle_filled(vp.world_to_screen(pos(*center)), (*radius as f32) * vp.zoom, color(*fill));
        }
        Mark::Polygon { points, fill } => {
            let pts = points.iter().map(|p| vp.world_to_screen(pos(*p))).collect();
            painter.add(PathShape::convex_polygon(pts, color(*fill), Stroke::NONE));
        }
        Mark::Polyline { points, line, width } => {
            let pts: Vec<_> = points.iter().map(|p| vp.world_to_screen(pos(*p))).collect();
            let stroke = Stroke::new(line_px(*width, vp.zoom), color(*line));
            painter.add(PathShape::line(pts.clone(), stroke));
            // Round caps and joins, as the SVG draws them.
            for p in pts {
                painter.circle_filled(p, stroke.width / 2.0, stroke.color);
            }
        }
        Mark::Text { rect: r, text, size, color: c } => {
            // Too small to read at this zoom: the box alone says something is there.
            let Some(font) = font_for(*size, false, vp.zoom) else { return };
            let sr = screen(*r);
            painter.text(pos2(sr.center().x, sr.center().y), Align2::CENTER_CENTER, text, font, color(*c));
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::{Context, Pos2, Rect as ERect, vec2};
    use vizcraft_diagram::data::{DataRow, DataType};
    use vizcraft_diagram::data_graphic::{DataGraphic, DataItem, ItemKind, Op, Rule};
    use vizcraft_diagram::{Color, ShapeKind};

    use super::*;

    #[test]
    fn a_shape_with_a_graphic_paints_more_and_keeps_its_style() {
        let mut doc = Document::new();
        let mut s = Shape::new("s1", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
        s.data.rows.push(DataRow { key: "cost".into(), kind: DataType::Number, value: "5".into(), ..DataRow::default() });
        s.data_graphic = Some("g".into());
        let fill = Rule { op: Op::Any, fill: Some(Color::rgb(255, 0, 0)), ..Rule::default() };
        doc.data_graphics.push(DataGraphic {
            id: "g".into(),
            name: "G".into(),
            items: vec![
                DataItem { key: "cost".into(), ..DataItem::default() },
                DataItem { kind: ItemKind::Color, key: "cost".into(), rules: vec![fill], ..DataItem::default() },
            ],
        });
        doc.page_mut().unwrap().shapes.push(s.clone());
        let ctx = Context::default();
        let mut counts = Vec::new();
        for with in [false, true] {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let vp = Viewport2D::new(ERect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0)), [50.0, 30.0], 1.0);
                let mut cache = FillCache::default();
                let page = doc.page().unwrap();
                let mut shown = page.shapes[0].clone();
                if !with {
                    shown.data_graphic = None;
                }
                let drawn = shape(ui.painter(), &vp, &doc, page, &shown, &mut cache, true);
                marks(ui.painter(), &vp, &drawn);
            });
            out.textures_delta.clear();
            counts.push(out.shapes.len());
        }
        assert!(counts[1] > counts[0], "the callout adds marks: {counts:?}");
        assert_eq!(doc.page().unwrap().shapes[0].style, s.style);
    }
}
