//! Design > Themes: a gallery of the built-in document themes, plus no theme.

use egui::{Color32, Painter, Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};
use serde_json::json;
use vizcraft_diagram::theme::{self, DocTheme};

use crate::CraftApp;
use crate::ribbon::{Control, Group, color32};
use crate::theme::Tokens;
use crate::widgets;

/// Size of one theme or variant cell.
pub(super) const CELL: egui::Vec2 = egui::Vec2::new(44.0, 30.0);

/// One gallery cell: hover and selection chrome around `paint`, a tooltip naming it, and whether
/// it was clicked while `id` is enabled.
pub(super) fn cell(ui: &mut Ui, app: &CraftApp, id: &str, label: &str, selected: bool, paint: impl FnOnce(&Painter, Rect)) -> bool {
    let t = Tokens::get(ui.ctx());
    let on = widgets::enabled(app, id);
    let (r, resp) = ui.allocate_exact_size(CELL, Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, on, label));
    if on && resp.hovered() {
        ui.painter().rect_filled(r, 3.0, t.hover);
    }
    paint(ui.painter(), r.shrink(4.0));
    let border = if selected {
        Stroke::new(2.0, t.accent)
    } else if on && resp.hovered() {
        Stroke::new(1.0, t.accent)
    } else {
        Stroke::new(1.0, t.border)
    };
    ui.painter().rect_stroke(r.shrink(2.0), 3.0, border, egui::StrokeKind::Inside);
    if !on {
        ui.painter().rect_filled(r.shrink(2.0), 3.0, t.ribbon.gamma_multiply(0.6));
    }
    let resp = widgets::tooltip(app, resp, label, id);
    on && resp.clicked()
}

/// A preview of theme `t`: its page tint, a shape in its fill and line, and its four accents.
pub(super) fn preview(p: &Painter, r: Rect, t: &DocTheme) {
    let (accents, ink) = theme::accents(t);
    let colors = t.colors();
    p.rect_filled(r, 2.0, color32(theme::background_color(t)));
    let fill = colors.map_or(Color32::WHITE, |v| color32(v.fill));
    let line = colors.map_or(color32(ink), |v| color32(v.line));
    let shape = Rect::from_min_size(r.min + vec2(3.0, 3.0), vec2(r.width() * 0.5, r.height() - 9.0));
    p.rect(shape, 2.0, fill, Stroke::new(1.0, line), egui::StrokeKind::Inside);
    let w = (r.width() * 0.4 - 4.0) / 4.0;
    for (i, a) in accents.iter().enumerate() {
        let x = r.max.x - 3.0 - (4.0 - i as f32) * w;
        p.rect_filled(Rect::from_min_size(pos2(x, r.min.y + 3.0), vec2(w - 1.0, r.height() - 9.0)), 0.0, color32(*a));
    }
    p.rect_filled(Rect::from_min_max(pos2(r.min.x + 3.0, r.max.y - 4.0), pos2(r.max.x - 3.0, r.max.y - 2.0)), 0.0, line);
}

fn gallery(app: &mut CraftApp, ui: &mut Ui) {
    let current = app.session.doc.theme.clone();
    let mut entries: Vec<(&str, String)> = vec![("none", "No Theme".to_string())];
    entries.extend(theme::themes().iter().map(|t| (t.id, t.name.to_string())));
    let mut chosen = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for row in entries.chunks(4) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for (id, name) in row {
                    let t = if *id == "none" { DocTheme::default() } else { DocTheme::new(*id, usize::from(current.variant)) };
                    let selected = if *id == "none" { current.id.is_empty() } else { current.id == *id };
                    if cell(ui, app, "design.setTheme", name, selected, |p, r| preview(p, r, &t)) {
                        chosen = Some(*id);
                    }
                }
            });
        }
    });
    if let Some(id) = chosen {
        let _ = app.run("design.setTheme", json!({"theme": id}));
    }
}

/// The Design tab's Themes group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Themes", vec![Control::Custom(gallery)])
}
