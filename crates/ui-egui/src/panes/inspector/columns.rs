//! The Inspector's Columns section: the columns of an ER table as a grid in drawing order (the
//! key rows, then the others) with a delete button first, then Name, Type, PK, FK, Null and Unique cells,
//! and an Add column row. Cells edit the column element (`element.rename`, `element.setProperty`),
//! delete is `model.delete`, and Add is `er.addColumn` for an ER table (any other owner takes
//! `element.create` of the table query's first type).

use egui::{RichText, Ui};
use serde_json::{Value, json};

use super::Runs;
use crate::CraftApp;
use crate::theme::Tokens;

/// The element types whose columns `er.addColumn` adds.
const ER_TABLES: [&str; 2] = ["er.Entity", "er.WeakEntity"];

/// The flag columns of the grid: `(property key, heading)`.
const FLAGS: [(&str, &str); 4] = [("pk", "PK"), ("fk", "FK"), ("nullable", "Null"), ("unique", "Unique")];

/// The owned-table queries of an element type's own compartments, in drawing order.
fn tables_of(type_id: &str) -> Vec<&'static vizcraft_notation::schema::TableQuery> {
    vizcraft_notation::catalog().compartments_in(type_id, "").iter().filter_map(|d| d.query.as_ref()).filter(|q| q.owned).collect()
}

/// One column of a table as the grid shows it.
struct Col {
    id: String,
    name: String,
    data_type: String,
    flags: [bool; 4],
}

/// The columns of element `el` in drawing order, with the first column type of its table query.
fn columns_of(app: &CraftApp, el: &str) -> (Vec<Col>, Option<String>) {
    let doc = &app.session.doc;
    let Some(owner) = doc.model.element(el) else { return (Vec::new(), None) };
    let cat = vizcraft_notation::catalog();
    let tables = tables_of(&owner.type_id);
    let first = tables.first().and_then(|q| q.types.first().cloned());
    let mut out: Vec<Col> = Vec::new();
    for q in tables {
        for id in vizcraft_notation::sheet_pass::table_row_ids(doc, cat, owner, q) {
            if out.iter().any(|c| c.id == id) {
                continue;
            }
            let Some(a) = doc.model.element(&id) else { continue };
            let flag = |k: &str| a.prop(k).and_then(vizcraft_diagram::vizcraft_model::PropValue::as_bool).unwrap_or(k == "nullable");
            out.push(Col {
                id,
                name: a.name.clone(),
                data_type: a.prop("dataType").map(vizcraft_diagram::vizcraft_model::PropValue::display).unwrap_or_default(),
                flags: [flag("pk"), flag("fk"), flag("nullable"), flag("unique")],
            });
        }
    }
    (out, first)
}

/// Whether the element type owns a table (has a Columns section).
pub(super) fn has_table(type_id: &str) -> bool {
    !tables_of(type_id).is_empty()
}

/// The Columns section for element `el` of type `type_id`.
pub(super) fn section(app: &CraftApp, ui: &mut Ui, t: &Tokens, el: &str, type_id: &str, runs: &mut Runs) {
    let (cols, child_type) = columns_of(app, el);
    if cols.is_empty() {
        ui.label(RichText::new("No columns yet.").color(t.text_dim));
    } else {
        // Seven columns are wider than the pane: the grid scrolls sideways.
        egui::ScrollArea::horizontal().id_salt(("inspector-columns-scroll", el)).show(ui, |ui| {
        egui::Grid::new(("inspector-columns", el)).num_columns(7).spacing([6.0, 3.0]).striped(true).show(ui, |ui| {
            for h in ["", "Name", "Type"].into_iter().chain(FLAGS.iter().map(|f| f.1)) {
                ui.label(RichText::new(h).small().color(t.text_dim));
            }
            ui.end_row();
            for c in &cols {
                let del = ui.add(egui::Button::new("x").small()).on_hover_text("Delete this column from the model");
                let label = format!("Delete column {}", c.name);
                del.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone()));
                if del.clicked() {
                    runs.push(("model.delete", json!({"ids": [c.id]})));
                }
                if let Some(n) = super::text_cell(ui, &format!("{}|col-name|{}", c.id, c.name), &c.name, "", 70.0)
                    && !n.trim().is_empty()
                {
                    runs.push(("element.rename", json!({"id": c.id, "name": n.trim()})));
                }
                if let Some(ty) = super::text_cell(ui, &format!("{}|col-type|{}", c.id, c.data_type), &c.data_type, "", 60.0) {
                    runs.push(("element.setProperty", json!({"id": c.id, "key": "dataType", "value": if ty.trim().is_empty() { Value::Null } else { json!(ty.trim()) }})));
                }
                for (i, (key, name)) in FLAGS.iter().enumerate() {
                    let mut on = c.flags[i];
                    let resp = ui.checkbox(&mut on, "");
                    let label = format!("{name} of {}", c.name);
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, label.clone()));
                    if resp.changed() {
                        runs.push(("element.setProperty", json!({"id": c.id, "key": key, "value": on})));
                    }
                }
                ui.end_row();
            }
        });
        });
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Add column").color(t.text_dim));
        let add_w = (ui.available_width() - 12.0).max(60.0);
        if let Some(name) = super::text_cell(ui, &format!("{el}|add-column|{}", cols.len()), "", "name", add_w)
            && !name.trim().is_empty()
        {
            if ER_TABLES.contains(&type_id) {
                runs.push(("er.addColumn", json!({"entity": el, "name": name.trim(), "dataType": "", "pk": false})));
            } else if let Some(ty) = &child_type {
                runs.push(("element.create", json!({"type": ty, "owner": el, "name": name.trim()})));
            }
        }
    });
}

/// The `columns` field of `ui.inspect`'s inspector report: the selected table's columns in
/// drawing order, or null.
pub(super) fn inspect(app: &CraftApp) -> Value {
    let Some(d) = super::members::one_element(app) else { return Value::Null };
    if !has_table(d["element"]["type"].as_str().unwrap_or_default()) {
        return Value::Null;
    }
    let el = d["element"]["id"].as_str().unwrap_or_default();
    let (cols, _) = columns_of(app, el);
    let rows: Vec<Value> = cols
        .iter()
        .map(|c| json!({"id": c.id, "name": c.name, "dataType": c.data_type, "pk": c.flags[0], "fk": c.flags[1], "nullable": c.flags[2], "unique": c.flags[3]}))
        .collect();
    json!({"element": el, "rows": rows})
}
