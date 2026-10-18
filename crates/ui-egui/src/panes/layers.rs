//! The Layers pane: the active page's layers with their flags, colour and shape count, and
//! buttons to add, rename and delete a layer and to put the selection on it or take it off.
//!
//! Every change runs a `layer.*` command; the pane keeps only which row is current and the
//! rename text, in egui's temporary memory.

use egui::{Color32, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::{Color, Layer};

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::ribbon::PALETTE;
use crate::theme::Tokens;

/// The Layers pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("layers", "Layers", Side::Right, 320.0, layers);

/// The flags a row toggles: column heading, `layer.set` param, tooltip.
const FLAGS: [(&str, &str, &str); 5] = [
    ("Show", "visible", "Show the shapes on this layer"),
    ("Lock", "locked", "Lock the shapes on this layer: they cannot be selected, moved or deleted"),
    ("Print", "print", "Print the shapes on this layer"),
    ("Snap", "snap", "Let other shapes snap to the shapes on this layer"),
    ("Glue", "glue", "Let connectors glue to the shapes on this layer"),
];

fn flag(l: &Layer, key: &str) -> bool {
    match key {
        "visible" => l.visible,
        "locked" => l.locked,
        "print" => l.print,
        "snap" => l.snap,
        _ => l.glue,
    }
}

fn current_id() -> egui::Id {
    egui::Id::new("layers-pane-current")
}

fn rename_id() -> egui::Id {
    egui::Id::new("layers-pane-rename")
}

/// The first `Layer N` name not used on the page.
fn fresh_name(layers: &[Layer]) -> String {
    (1..=layers.len() + 1)
        .map(|n| format!("Layer {n}"))
        .find(|n| !layers.iter().any(|l| l.name.eq_ignore_ascii_case(n)))
        .unwrap_or_else(|| format!("Layer {}", layers.len() + 2))
}

fn color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn layers(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(page) = app.session.doc.page() else { return };
    let page_name = page.name.clone();
    let rows: Vec<(Layer, usize)> = page.layers.iter().map(|l| (l.clone(), page.shapes.iter().filter(|s| s.layers.contains(&l.id)).count())).collect();
    let mut current: Option<String> = ui.ctx().data(|d| d.get_temp(current_id()));
    if current.as_ref().is_some_and(|c| !rows.iter().any(|(l, _)| l.id == *c)) {
        current = None;
    }
    let mut rename: Option<String> = ui.ctx().data(|d| d.get_temp(rename_id()));
    let mut runs: Vec<(&'static str, Value)> = Vec::new();

    let has_sel = app.session.doc.page().is_some_and(|p| app.session.sel.items.iter().any(|id| p.shape(id).is_some()));
    let cur = current.clone();
    ui.horizontal_wrapped(|ui| {
        if ui.button("Add").on_hover_text("Add a layer to this page").clicked() {
            let names: Vec<Layer> = rows.iter().map(|(l, _)| l.clone()).collect();
            runs.push(("layer.add", json!({"name": fresh_name(&names)})));
        }
        if ui.add_enabled(cur.is_some(), egui::Button::new("Rename")).clicked() {
            rename = rows.iter().find(|(l, _)| Some(&l.id) == cur.as_ref()).map(|(l, _)| l.name.clone());
        }
        if ui.add_enabled(cur.is_some(), egui::Button::new("Delete")).on_hover_text("Delete the layer; its shapes stay on the page").clicked()
            && let Some(id) = &cur
        {
            runs.push(("layer.delete", json!({"layer": id})));
        }
    });
    ui.horizontal_wrapped(|ui| {
        let can = cur.is_some() && has_sel;
        if ui.add_enabled(can, egui::Button::new("Assign Selection")).on_hover_text("Put the selected shapes on this layer").clicked()
            && let Some(id) = &cur
        {
            runs.push(("layer.assign", json!({"layer": id})));
        }
        if ui.add_enabled(can, egui::Button::new("Remove Selection")).on_hover_text("Take the selected shapes off this layer").clicked()
            && let Some(id) = &cur
        {
            runs.push(("layer.unassign", json!({"layer": id})));
        }
    });

    ui.add_space(6.0);
    ui.label(egui::RichText::new(format!("Layers of {page_name}")).small().color(t.text_dim));
    ui.add_space(4.0);
    if rows.is_empty() {
        ui.label(egui::RichText::new("This page has no layers. Shapes on no layer are always shown.").color(t.text_dim));
    } else {
        egui::ScrollArea::both().id_salt("layers-pane-rows").show(ui, |ui| {
            egui::Grid::new("layers-pane-grid").num_columns(FLAGS.len() + 2).spacing([4.0, 4.0]).striped(true).show(ui, |ui| {
                ui.label("");
                ui.label(egui::RichText::new("Name").small().color(t.text_dim));
                for (head, _, _) in FLAGS {
                    ui.label(egui::RichText::new(head).small().color(t.text_dim));
                }
                ui.end_row();
                for (l, count) in &rows {
                    let is_current = current.as_deref() == Some(l.id.as_str());
                    if let Some(c) = swatch(ui, &t, l) {
                        runs.push(("layer.set", json!({"layer": l.id, "color": c})));
                    }
                    match rename.as_mut().filter(|_| is_current) {
                        Some(text) => {
                            let resp = ui.add(egui::TextEdit::singleline(text).desired_width(120.0));
                            resp.request_focus();
                            if resp.lost_focus() {
                                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && text.trim() != l.name {
                                    runs.push(("layer.set", json!({"layer": l.id, "name": text.trim()})));
                                }
                                rename = None;
                            }
                        }
                        None => {
                            ui.horizontal(|ui| {
                                let resp = ui.selectable_label(is_current, &l.name).on_hover_text("Click to make current; double-click to rename");
                                if resp.clicked() {
                                    current = Some(l.id.clone());
                                }
                                if resp.double_clicked() {
                                    current = Some(l.id.clone());
                                    rename = Some(l.name.clone());
                                }
                                let shapes = if *count == 1 { "1 shape".to_string() } else { format!("{count} shapes") };
                                ui.label(egui::RichText::new(count.to_string()).small().color(t.text_dim)).on_hover_text(shapes);
                            });
                        }
                    }
                    for (head, key, tip) in FLAGS {
                        let mut on = flag(l, key);
                        let resp = ui.checkbox(&mut on, "").on_hover_text(tip);
                        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, format!("{head} {}", l.name)));
                        if resp.changed() {
                            runs.push(("layer.set", json!({"layer": l.id, key: on})));
                        }
                    }
                    ui.end_row();
                }
            });
        });
    }
    for (id, params) in runs {
        if let Ok(r) = app.run(id, params)
            && id == "layer.add"
            && let Some(new_id) = r.pointer("/layer/id").and_then(Value::as_str)
        {
            current = Some(new_id.to_string());
        }
    }
    ui.ctx().data_mut(|d| {
        match current {
            Some(c) => {
                d.insert_temp(current_id(), c);
            }
            None => d.remove::<String>(current_id()),
        }
        match rename {
            Some(r) => {
                d.insert_temp(rename_id(), r);
            }
            None => d.remove::<String>(rename_id()),
        }
    });
}

/// The layer's colour swatch; a click opens the palette. Returns the chosen colour as
/// `#rrggbb`, or `""` for no colour.
fn swatch(ui: &mut Ui, t: &Tokens, l: &Layer) -> Option<String> {
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    let resp = resp.on_hover_text("Layer colour: tints the shapes on this layer");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Colour {}", l.name)));
    let p = ui.painter();
    let inner = r.shrink(2.0);
    match l.color {
        Some(c) => p.rect_filled(inner, 2.0, color32(c)),
        None => p.line_segment([inner.left_bottom(), inner.right_top()], Stroke::new(1.0, t.text_dim)),
    };
    p.rect_stroke(inner, 2.0, Stroke::new(1.0, if resp.hovered() { t.accent } else { t.border_strong }), egui::StrokeKind::Outside);
    let mut out = None;
    egui::Popup::menu(&resp).show(|ui| {
        if ui.button("No Colour").clicked() {
            out = Some(String::new());
            ui.close();
        }
        for row in PALETTE {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                for rgb in row {
                    let c = Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
                    let (sr, sresp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                    ui.painter().rect_filled(sr, 2.0, c);
                    if sresp.hovered() {
                        ui.painter().rect_stroke(sr, 2.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Outside);
                    }
                    if sresp.clicked() {
                        out = Some(format!("#{rgb:06x}"));
                        ui.close();
                    }
                }
            });
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_names_skip_taken_ones() {
        assert_eq!(fresh_name(&[]), "Layer 1");
        let taken = vec![Layer { id: "a".into(), name: "layer 1".into(), ..Layer::default() }];
        assert_eq!(fresh_name(&taken), "Layer 2");
    }
}
