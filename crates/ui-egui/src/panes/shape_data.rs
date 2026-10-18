//! The Shape Data pane: the selected shape's typed data rows with an editor per type, buttons to
//! add and remove keys, and the page's key definitions. With several shapes selected it edits the
//! keys they all have. Rows the shape's model element owns are shown read-only.
//!
//! Every change runs a `data.*` command; the pane keeps only edit buffers and the add-row form in
//! egui's temporary memory. It reads rows through `vizcraft_engine::cmd::data::rows_of`, the same
//! merge `data.get` reports.

use egui::{RichText, Ui};
use serde_json::{Value, json};
use vizcraft_diagram::data::{self, DataType};
use vizcraft_engine::cmd::data::{RowView, rows_of};

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::theme::Tokens;

/// The Shape Data pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("shapeData", "Shape Data", Side::Right, 300.0, shape_data);

/// A row as the pane edits it: the row, and whether the selected shapes disagree on its value.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneRow {
    /// The row (from the first selected shape).
    pub row: RowView,
    /// The selected shapes hold different values.
    pub mixed: bool,
}

/// The selected shapes on the active page and the rows the pane shows for them: every row of
/// one shape, or the shape data keys that all of several shapes have (model rows are per shape
/// and are left out of a multiple selection).
pub fn selection_rows(app: &CraftApp) -> (Vec<String>, Vec<PaneRow>) {
    let doc = &app.session.doc;
    let Some(page) = doc.page() else { return (Vec::new(), Vec::new()) };
    let ids: Vec<String> = app.session.sel.items.iter().filter(|id| page.shape(id).is_some()).cloned().collect();
    let Some(first) = ids.first() else { return (ids, Vec::new()) };
    let base = rows_of(doc, first).unwrap_or_default();
    if ids.len() == 1 {
        return (ids, base.into_iter().map(|row| PaneRow { row, mixed: false }).collect());
    }
    let others: Vec<Vec<RowView>> = ids.iter().skip(1).map(|id| rows_of(doc, id).unwrap_or_default()).collect();
    let rows = base
        .into_iter()
        .filter(|r| !r.model)
        .filter_map(|row| {
            let mut mixed = false;
            for o in &others {
                let m = o.iter().find(|x| !x.model && x.key == row.key)?;
                mixed |= m.value != row.value;
            }
            Some(PaneRow { row, mixed })
        })
        .collect();
    (ids, rows)
}

fn add_form_id() -> egui::Id {
    egui::Id::new("shape-data-pane-add")
}

fn show_hidden_id() -> egui::Id {
    egui::Id::new("shape-data-pane-hidden")
}

/// A one-line text editor that commits on Enter or when it loses focus (Escape cancels).
/// Returns the new text when it differs from `current`.
fn text_cell(ui: &mut Ui, salt: &str, current: &str, hint: &str, width: f32) -> Option<String> {
    let id = egui::Id::new(("shape-data-pane-edit", salt));
    let mut buf: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_else(|| current.to_string());
    let resp = ui.add(egui::TextEdit::singleline(&mut buf).id(id.with("field")).hint_text(hint).desired_width(width));
    let mut out = None;
    if resp.lost_focus() {
        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) && buf != current {
            out = Some(buf);
        }
        ui.ctx().data_mut(|d| d.remove::<String>(id));
    } else if resp.has_focus() {
        ui.ctx().data_mut(|d| d.insert_temp(id, buf));
    } else {
        ui.ctx().data_mut(|d| d.remove::<String>(id));
    }
    out
}

/// The editor for one row's value; returns the value to set.
fn value_cell(ui: &mut Ui, t: &Tokens, salt: &str, r: &PaneRow) -> Option<Value> {
    let row = &r.row;
    let hint = if r.mixed { "(mixed)" } else { "" };
    let shown = if r.mixed { "" } else { row.value.as_str() };
    if row.model {
        let text = if row.value.is_empty() { "-" } else { row.value.as_str() };
        let unit = if row.unit.is_empty() || row.value.is_empty() { String::new() } else { format!(" {}", row.unit) };
        ui.label(RichText::new(format!("{text}{unit}")).color(t.text_dim))
            .on_hover_text("Owned by the shape's model element; change it in the Properties pane");
        return None;
    }
    match row.kind {
        DataType::Bool => {
            let mut on = row.value == "true";
            let resp = ui.checkbox(&mut on, if r.mixed { "(mixed)" } else { "" });
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, row.label.clone()));
            resp.changed().then_some(Value::Bool(on))
        }
        DataType::Enum => {
            let mut out = None;
            let text = if r.mixed { "(mixed)".to_string() } else if row.value.is_empty() { "(none)".to_string() } else { row.value.clone() };
            egui::ComboBox::from_id_salt(("shape-data-pane-enum", salt)).selected_text(text).width(140.0).show_ui(ui, |ui| {
                if ui.selectable_label(row.value.is_empty() && !r.mixed, "(none)").clicked() {
                    out = Some(Value::Null);
                }
                for v in &row.values {
                    if ui.selectable_label(!r.mixed && row.value == *v, v).clicked() {
                        out = Some(Value::String(v.clone()));
                    }
                }
            });
            out
        }
        DataType::Number => {
            let mut out = None;
            ui.horizontal(|ui| {
                out = text_cell(ui, salt, shown, hint, 90.0).map(Value::String);
                if !row.unit.is_empty() {
                    ui.label(RichText::new(&row.unit).color(t.text_dim));
                }
            });
            out
        }
        DataType::Date => text_cell(ui, salt, shown, if r.mixed { hint } else { "YYYY-MM-DD" }, 140.0).map(Value::String),
        DataType::Url => text_cell(ui, salt, shown, if r.mixed { hint } else { "https://" }, 140.0).map(Value::String),
        DataType::String => text_cell(ui, salt, shown, hint, 140.0).map(Value::String),
    }
}

fn shape_data(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (ids, rows) = selection_rows(app);
    let mut runs: Vec<(&'static str, Value)> = Vec::new();
    let mut show_hidden: bool = ui.ctx().data(|d| d.get_temp(show_hidden_id())).unwrap_or(false);

    ui.horizontal_wrapped(|ui| {
        if ui.button("Define...").on_hover_text("Define a data key on this page").clicked() {
            runs.push(("data.defineDialog", json!({})));
        }
        ui.checkbox(&mut show_hidden, "Show hidden");
    });
    ui.add_space(4.0);
    if ids.is_empty() {
        ui.label(RichText::new("Select a shape to see its data.").color(t.text_dim));
    } else {
        let doc = &app.session.doc;
        let title = match ids.as_slice() {
            [one] => {
                let text = doc.shape(one).map(|s| s.text.lines().next().unwrap_or("").trim().to_string()).unwrap_or_default();
                if text.is_empty() { format!("Shape {one}") } else { format!("{text} ({one})") }
            }
            many => format!("{} shapes: the keys they share", many.len()),
        };
        ui.label(RichText::new(title).small().color(t.text_dim));
        let use_page = ids.iter().all(|id| doc.shape(id).is_some_and(|s| s.data.page_keys));
        let mut on = use_page;
        if ui.checkbox(&mut on, "Use page keys").on_hover_text("Give the shape every key this page defines, now and later").changed() {
            runs.push(("data.usePageKeys", json!({"ids": ids, "value": on})));
        }
        ui.add_space(4.0);
        let visible: Vec<&PaneRow> = rows.iter().filter(|r| show_hidden || !r.row.hidden).collect();
        if visible.is_empty() {
            ui.label(RichText::new("No shape data yet. Add a key below.").color(t.text_dim));
        } else {
            egui::Grid::new("shape-data-pane-grid").num_columns(3).spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
                for r in visible {
                    let row = &r.row;
                    let mut label = row.label.clone();
                    if row.required {
                        label.push_str(" *");
                    }
                    let tip = format!(
                        "Key: {}\nType: {}{}{}",
                        row.key,
                        row.kind.name(),
                        if row.unit.is_empty() { String::new() } else { format!(" ({})", row.unit) },
                        if row.model {
                            "\nOwned by the model element"
                        } else if row.page_key {
                            "\nDefined on this page"
                        } else {
                            ""
                        }
                    );
                    let color = if row.required && row.value.is_empty() { t.red } else { t.text };
                    ui.label(RichText::new(label).color(color)).on_hover_text(tip);
                    let salt = format!("{}|{}", ids.join(","), row.key);
                    if let Some(v) = value_cell(ui, &t, &salt, r) {
                        runs.push(("data.set", json!({"ids": ids, "key": row.key, "value": v})));
                    }
                    if row.model {
                        ui.label("");
                    } else if ui.small_button("x").on_hover_text(format!("Remove {} from the selection", row.key)).clicked() {
                        runs.push(("data.remove", json!({"ids": ids, "key": row.key})));
                    }
                    ui.end_row();
                }
            });
        }
        ui.add_space(6.0);
        ui.separator();
        add_form(ui, &ids, &mut runs);
    }

    page_keys_section(app, ui, &t, &mut runs);
    ui.ctx().data_mut(|d| d.insert_temp(show_hidden_id(), show_hidden));
    for (id, params) in runs {
        let _ = app.run(id, params);
    }
}

/// The add-row form: key, type and value, run as `data.set` on the selection.
fn add_form(ui: &mut Ui, ids: &[String], runs: &mut Vec<(&'static str, Value)>) {
    let (mut key, mut kind, mut value): (String, String, String) =
        ui.ctx().data(|d| d.get_temp(add_form_id())).unwrap_or_else(|| (String::new(), "string".into(), String::new()));
    ui.label(RichText::new("Add a key").small());
    ui.horizontal_wrapped(|ui| {
        ui.add(egui::TextEdit::singleline(&mut key).hint_text("Key").desired_width(90.0));
        egui::ComboBox::from_id_salt("shape-data-pane-add-type").selected_text(kind.clone()).width(80.0).show_ui(ui, |ui| {
            for k in DataType::ALL {
                // Enums need their values, which the Define dialog asks for.
                if k != DataType::Enum {
                    ui.selectable_value(&mut kind, k.name().to_string(), k.name());
                }
            }
        });
        ui.add(egui::TextEdit::singleline(&mut value).hint_text("Value").desired_width(90.0));
        let ok = data::is_key(key.trim());
        if ui.add_enabled(ok, egui::Button::new("Add")).clicked() {
            runs.push(("data.set", json!({"ids": ids, "key": key.trim(), "type": kind, "value": value})));
            key.clear();
            value.clear();
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(add_form_id(), (key, kind, value)));
}

/// The page's key definitions, each with a delete button.
fn page_keys_section(app: &CraftApp, ui: &mut Ui, t: &Tokens, runs: &mut Vec<(&'static str, Value)>) {
    let Some(page) = app.session.doc.page() else { return };
    let keys = data::page_keys(page);
    ui.add_space(6.0);
    egui::CollapsingHeader::new(format!("Page keys ({})", keys.len())).id_salt("shape-data-pane-page-keys").default_open(true).show(ui, |ui| {
        if keys.is_empty() {
            ui.label(RichText::new("This page defines no keys.").color(t.text_dim));
        }
        for k in &keys {
            ui.horizontal(|ui| {
                let mut text = format!("{} ({})", k.title(), k.kind.name());
                if !k.unit.is_empty() {
                    text = format!("{text}, {}", k.unit);
                }
                ui.label(text).on_hover_text(format!("Key: {}", k.key));
                if ui.small_button("x").on_hover_text("Delete the key from the page and from shapes that use page keys").clicked() {
                    runs.push(("data.undefineKey", json!({"key": k.key})));
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default())
    }

    #[test]
    fn several_shapes_share_only_common_keys() {
        let mut a = app();
        let ids: Vec<String> = a.session.doc.page().unwrap().shapes.iter().take(2).map(|s| s.id.clone()).collect();
        a.run("data.set", json!({"ids": ids, "key": "cost", "value": 1})).unwrap();
        a.run("data.set", json!({"ids": [ids[1]], "key": "cost", "value": 2})).unwrap();
        a.run("data.set", json!({"ids": [ids[0]], "key": "only", "value": "x"})).unwrap();
        a.run("edit.select", json!({"ids": ids})).unwrap();
        let (sel, rows) = selection_rows(&a);
        assert_eq!(sel, ids);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row.key, "cost");
        assert!(rows[0].mixed);
        a.run("edit.select", json!({"ids": [ids[0]]})).unwrap();
        let (_, rows) = selection_rows(&a);
        assert!(rows.iter().any(|r| r.row.key == "only"));
    }

    #[test]
    fn the_window_command_opens_and_closes_the_pane() {
        let mut a = app();
        assert!(!a.ui.panes.is_open("shapeData"), "starts closed");
        a.run("view.shapeDataPane", json!({})).unwrap();
        assert!(a.ui.panes.is_open("shapeData"));
        a.run("view.shapeDataPane", json!({"value": false})).unwrap();
        assert!(!a.ui.panes.is_open("shapeData"));
    }
}
