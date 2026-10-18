//! Container > Styles: a gallery of the container style presets, each drawn as a small
//! container in its own colours. Shared with the Callout tab's gallery.

use egui::{Painter, Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};
use serde_json::json;
use vizcraft_diagram::container::{CONTAINER_STYLES, HeadingStyle, Preset};

use super::selected_container;
use crate::CraftApp;
use crate::ribbon::{Control, Group, color32};
use crate::theme::Tokens;
use crate::widgets;

/// Size of one style cell.
const CELL: egui::Vec2 = egui::Vec2::new(40.0, 30.0);

/// A preview of preset `p` in `r`: the body, the heading band or rule, and (for a callout) a
/// leader.
pub(in crate::ribbon) fn preview(painter: &Painter, r: Rect, p: &Preset, callout: bool) {
    let line = if p.line.a == 0 { egui::Color32::from_gray(120) } else { color32(p.line) };
    let body = if callout { Rect::from_min_max(pos2(r.min.x + r.width() * 0.3, r.min.y), pos2(r.max.x, r.min.y + r.height() * 0.62)) } else { r };
    let round = (p.corner as f32 / 3.0).min(5.0);
    if callout {
        painter.line_segment([pos2(body.min.x + 3.0, body.max.y - 2.0), pos2(r.min.x + 2.0, r.max.y - 2.0)], Stroke::new(1.0, line));
        painter.circle_filled(pos2(r.min.x + 2.0, r.max.y - 2.0), 1.6, line);
    }
    let fill = color32(p.fill);
    let stroke = if p.line.a == 0 { Stroke::NONE } else { Stroke::new(1.0, line) };
    if p.kind == vizcraft_diagram::ShapeKind::Ellipse {
        painter.add(egui::Shape::ellipse_filled(body.center(), body.size() / 2.0, fill));
        painter.add(egui::Shape::ellipse_stroke(body.center(), body.size() / 2.0, stroke));
    } else {
        painter.rect(body, round, fill, stroke, egui::StrokeKind::Inside);
    }
    if !callout {
        let band = Rect::from_min_max(body.min, pos2(body.max.x, body.min.y + 8.0));
        match p.heading {
            HeadingStyle::Band => {
                painter.rect_filled(band.shrink(1.0), 0.0, color32(p.band));
            }
            HeadingStyle::Line => {
                painter.line_segment([pos2(band.min.x, band.max.y), band.max], Stroke::new(1.0, line));
            }
            HeadingStyle::Plain => {}
        }
        if p.dash != vizcraft_diagram::Dash::Solid {
            // Gaps in the outline, so the dashed preset reads as dashed at this size.
            let bg = Tokens::get(&painter.ctx().clone()).ribbon;
            for i in 0..4 {
                let x = body.min.x + 6.0 + i as f32 * 8.0;
                painter.line_segment([pos2(x, body.max.y - 0.5), pos2(x + 3.0, body.max.y - 0.5)], Stroke::new(1.5, bg));
            }
        }
    }
    // A hint of the heading text.
    let text = color32(p.font);
    let y = if callout { body.center().y } else { body.min.y + 4.0 };
    painter.line_segment([pos2(body.min.x + 5.0, y), pos2(body.min.x + body.width() * 0.55, y)], Stroke::new(1.5, text));
}

/// One gallery cell; returns whether it was clicked while `id` is enabled.
pub(in crate::ribbon) fn cell(ui: &mut Ui, app: &CraftApp, id: &str, p: &Preset, selected: bool, callout: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let on = widgets::enabled(app, id);
    let (r, resp) = ui.allocate_exact_size(CELL, Sense::click());
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, on, selected, p.label));
    if on && resp.hovered() {
        ui.painter().rect_filled(r, 3.0, t.hover);
    }
    preview(ui.painter(), r.shrink(6.0), p, callout);
    let border = if selected { Stroke::new(2.0, t.accent) } else { Stroke::new(1.0, if on && resp.hovered() { t.accent } else { t.border }) };
    ui.painter().rect_stroke(r.shrink(2.0), 3.0, border, egui::StrokeKind::Inside);
    if !on {
        ui.painter().rect_filled(r.shrink(2.0), 3.0, t.ribbon.gamma_multiply(0.6));
    }
    if selected {
        widgets::check_badge(ui, r);
    }
    let resp = widgets::tooltip(app, resp, p.label, id);
    on && resp.clicked()
}

/// Presets in rows of three; runs `id` with `{"style": name}` for the one clicked.
pub(in crate::ribbon) fn grid(app: &mut CraftApp, ui: &mut Ui, id: &'static str, presets: &[Preset], current: Option<String>, callout: bool) {
    let mut chosen = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for row in presets.chunks(3) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for p in row {
                    if cell(ui, app, id, p, current.as_deref() == Some(p.name), callout) {
                        chosen = Some(p.name);
                    }
                }
            });
        }
    });
    if let Some(name) = chosen {
        let _ = app.run(id, json!({"style": name}));
    }
}

fn gallery(app: &mut CraftApp, ui: &mut Ui) {
    let current = selected_container(app).map(|c| c.style.clone());
    grid(app, ui, "container.set", CONTAINER_STYLES, current, false);
}

/// The Container tab's Styles group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Container Styles", vec![Control::Custom(gallery)])
}
