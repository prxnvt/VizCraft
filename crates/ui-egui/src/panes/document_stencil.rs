//! The Document Stencil pane: the document's own masters (`Document.masters`) as tiles with a
//! preview and the name. A click inserts a copy at the middle of the view (`master.insert`); the
//! Add Selection button makes a master of the selection (`master.add`); a tile's context menu
//! removes it (`master.remove`).

use egui::{Rect, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::CustomMaster;

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::theme::{Tokens, regular};

/// The Document Stencil pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("documentStencil", "Document Stencil", Side::Left, 240.0, document_stencil);

/// Size of one master tile.
const TILE: egui::Vec2 = egui::vec2(72.0, 76.0);

/// A small drawing of a master's shapes fitted into `r`, in their document colours (open
/// outlines and freeforms included, as the canvas draws them).
fn preview(painter: &egui::Painter, m: &CustomMaster, r: Rect) {
    use crate::canvas::draw::{color, pos};
    let (w, h) = (m.w.max(1.0), m.h.max(1.0));
    let k = (f64::from(r.width()) / w).min(f64::from(r.height()) / h);
    let origin = r.center() - vec2((w * k) as f32, (h * k) as f32) / 2.0;
    let place = |p: vizcraft_diagram::Point| origin + pos(p * k).to_vec2();
    for s in &m.shapes {
        let stroke = Stroke::new(1.0, color(s.style.line));
        for pl in s.outline_world().flatten(0.5 / k.max(1e-6)) {
            let pts: Vec<egui::Pos2> = pl.points.iter().map(|p| place(*p)).collect();
            if pl.closed {
                let mut mesh = egui::epaint::Mesh::default();
                for p in &pts {
                    mesh.colored_vertex(*p, color(s.style.fill));
                }
                for [a, b, c] in crate::canvas::tess::triangulate(&pts) {
                    mesh.add_triangle(a, b, c);
                }
                painter.add(egui::Shape::mesh(mesh));
                painter.add(egui::Shape::closed_line(pts, stroke));
            } else {
                painter.add(egui::Shape::line(pts, stroke));
            }
        }
    }
}

/// One tile; returns the command a click or the context menu asks for.
fn tile(app: &CraftApp, ui: &mut Ui, m: &CustomMaster) -> Option<(&'static str, Value)> {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(TILE, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &m.name));
    if resp.hovered() {
        ui.painter().rect_filled(r, 4.0, t.hover);
    }
    let art = Rect::from_min_size(r.min + vec2(10.0, 6.0), vec2(TILE.x - 20.0, TILE.y - 30.0));
    preview(ui.painter(), m, art);
    let name: String = if m.name.chars().count() > 12 { format!("{}...", m.name.chars().take(11).collect::<String>()) } else { m.name.clone() };
    ui.painter().text(egui::pos2(r.center().x, r.max.y - 12.0), egui::Align2::CENTER_CENTER, name, regular(11.0), t.text);
    let resp = resp.on_hover_text(format!("{}: click to insert a copy", m.name));
    let mut out = None;
    if resp.clicked() {
        let at = crate::canvas::insert_point(app);
        out = Some(("master.insert", json!({"id": m.id, "x": (at.x - m.w / 2.0).round(), "y": (at.y - m.h / 2.0).round()})));
    }
    resp.context_menu(|ui| {
        if ui.button("Insert").clicked() {
            out = Some(("master.insert", json!({"id": m.id})));
            ui.close();
        }
        if ui.button("Remove from Document Stencil").clicked() {
            out = Some(("master.remove", json!({"id": m.id})));
            ui.close();
        }
    });
    out
}

fn document_stencil(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let mut run: Option<(&'static str, Value)> = None;
    let can_add = crate::widgets::enabled(app, "master.add") && !app.session.sel.items.is_empty();
    ui.horizontal(|ui| {
        if ui.add_enabled(can_add, egui::Button::new("Add Selection")).on_hover_text("Make a master of the selected shapes").clicked() {
            run = Some(("master.add", json!({})));
        }
    });
    ui.add_space(6.0);
    let masters = app.session.doc.masters.clone();
    if masters.is_empty() {
        ui.label(egui::RichText::new("No masters yet. Select shapes and choose Add Selection to keep them here.").color(t.text_dim));
    } else {
        egui::ScrollArea::vertical().id_salt("document-stencil-tiles").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                for m in &masters {
                    if let Some(r) = tile(app, ui, m) {
                        run = Some(r);
                    }
                }
            });
        });
    }
    if let Some((id, p)) = run {
        let _ = app.run(id, p);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{CraftApp, Services};

    #[test]
    fn the_pane_toggles_by_command_and_adds_the_selection() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), Services::default());
        app.run("ui.pane.documentStencil", json!({"value": true})).unwrap();
        assert!(app.ui.panes.is_open("documentStencil"));
        let first = app.session.doc.page().unwrap().shapes[0].id.clone();
        app.run("edit.select", json!({"ids": [first]})).unwrap();
        app.run("master.add", json!({"name": "Mine"})).unwrap();
        assert_eq!(app.session.doc.masters.len(), 1);
    }
}
