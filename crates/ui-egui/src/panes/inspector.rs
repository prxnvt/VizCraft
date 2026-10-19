//! The Inspector pane: the selected element or shape, typed by the element schema.
//!
//! Sections, each collapsible: Identity (name and a tag that is checked against the identity
//! scope as you type), Properties (one editor per schema property, then the shape's data rows and
//! the page's keys), Relationships, Views, Shape (kind, text, position and size) and Document
//! (title and core properties, what the Properties pane held). A second tab, Format, runs the
//! fill, line and text commands. Two modes persisted in `UiState.inspector`: Standard hides ids,
//! `ext` keys and read-only fields, Expert shows them.
//!
//! Every change runs a command (`element.rename`, `element.retag`, `element.setProperty`,
//! `data.*`, `shape.*`, `arrange.setBounds`, `text.setFormat`, `edit.setTitle`); the pane keeps
//! only edit buffers in egui's temporary memory. It reads what it shows through
//! `vizcraft_engine::cmd::inspector::describe`, the same assembly `inspector.get` reports, and
//! `vizcraft_engine::cmd::data::rows_of` for shape data.

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vizcraft_diagram::data::{self, DataType};
use vizcraft_engine::cmd::data::{RowView, rows_of};
use vizcraft_engine::cmd::inspector::{check, describe};

mod relationship;

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::theme::Tokens;

/// The Inspector pane (the key is the old Shape Data pane's, so saved layouts and `ui.pane.shapeData` keep working).
pub(super) const SPEC: PaneSpec = PaneSpec::new("shapeData", "Inspector", Side::Right, 300.0, inspector);

/// How much the Inspector shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InspectorMode {
    /// Hides ids, `ext` keys and read-only model fields.
    #[default]
    Standard,
    /// Shows them.
    Expert,
}

/// Which body the Inspector shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InspectorTab {
    /// Identity, properties, relationships, views, shape and document.
    #[default]
    Properties,
    /// Fill, line and text controls.
    Format,
}

/// The Inspector's persisted state (`UiState.inspector`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InspectorState {
    /// Standard or Expert.
    pub mode: InspectorMode,
    /// The body showing.
    pub tab: InspectorTab,
    /// Open and scroll to the Document section on the next frame (set by `ui.pane.properties`).
    #[serde(skip)]
    pub show_document: bool,
}

/// `ui.inspector {mode?, tab?}`: set the mode and tab; the reply is the state afterwards. A bad
/// value is an error and changes nothing.
pub(super) fn ui_command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    let mode = match p.get("mode").and_then(Value::as_str) {
        None => None,
        Some("standard") => Some(InspectorMode::Standard),
        Some("expert") => Some(InspectorMode::Expert),
        Some(x) => return Err(format!("unknown mode {x:?} (standard or expert)")),
    };
    let tab = match p.get("tab").and_then(Value::as_str) {
        None => None,
        Some("properties") => Some(InspectorTab::Properties),
        Some("format") => Some(InspectorTab::Format),
        Some(x) => return Err(format!("unknown tab {x:?} (properties or format)")),
    };
    if let Some(m) = mode {
        app.ui.inspector.mode = m;
    }
    if let Some(t) = tab {
        app.ui.inspector.tab = t;
    }
    serde_json::to_value(&app.ui.inspector).map_err(|e| e.to_string())
}

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

type Runs = Vec<(&'static str, Value)>;

/// A collapsible section; `open` forces it open for this frame.
fn section(ui: &mut Ui, salt: &str, title: impl Into<egui::WidgetText>, open: bool, body: impl FnOnce(&mut Ui)) {
    let mut h = egui::CollapsingHeader::new(title).id_salt(("inspector-section", salt)).default_open(true);
    if open {
        h = h.open(Some(true));
    }
    h.show(ui, body);
}

fn inspector(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let mut runs: Runs = Vec::new();
    let st = app.ui.inspector.clone();

    ui.horizontal(|ui| {
        for (tab, label) in [(InspectorTab::Properties, "Properties"), (InspectorTab::Format, "Format")] {
            if ui.selectable_label(st.tab == tab, label).clicked() {
                runs.push(("ui.inspector", json!({"tab": if tab == InspectorTab::Format { "format" } else { "properties" }})));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let expert = st.mode == InspectorMode::Expert;
            let resp = ui.selectable_label(expert, "Expert").on_hover_text("Also show ids, ext keys and read-only fields");
            if resp.clicked() {
                runs.push(("ui.inspector", json!({"mode": if expert { "standard" } else { "expert" }})));
            }
        });
    });
    ui.separator();

    let (ids, rows) = selection_rows(app);
    let extra = Extra::gather(app, ui.ctx(), &ids);
    match st.tab {
        InspectorTab::Properties => properties_tab(app, ui, &t, &st, &ids, &rows, &extra, &mut runs),
        InspectorTab::Format => format_tab(app, ui, &t, &ids, &mut runs),
    }
    app.ui.inspector.show_document = false;
    for (id, params) in runs {
        let _ = app.run(id, params);
    }
}

#[allow(clippy::too_many_arguments)]
fn properties_tab(app: &CraftApp, ui: &mut Ui, t: &Tokens, st: &InspectorState, ids: &[String], rows: &[PaneRow], extra: &Extra, runs: &mut Runs) {
    let expert = st.mode == InspectorMode::Expert;
    let desc = match ids {
        [one] => describe(&app.session, one).ok(),
        _ => None,
    };
    if let Some((id, net)) = &extra.connector {
        section(ui, "connector-net", "Net", false, |ui| net_section(ui, t, id, net.as_ref(), runs));
    } else if ids.is_empty() {
        ui.label(RichText::new("Select a shape to inspect it.").color(t.text_dim));
    }
    if extra.key.get("id").is_some_and(|k| !k.is_null())
        && let Some(reference) = relationship::key_ref(app)
    {
        section(ui, "key", "Key", false, |ui| relationship::include_in_key(ui, t, &extra.key, &reference, runs));
    }
    if let Some(d) = &desc {
        if d.get("element").is_some_and(|e| !e.is_null()) {
            section(ui, "identity", "Identity", false, |ui| identity(app, ui, t, d, expert, runs));
        }
        // A symbol's parameters are what an engineer edits most, so they come first.
        if let Some(sym) = &extra.symbol {
            section(ui, "symbol", "Symbol", false, |ui| symbol_section(ui, t, d, sym, runs));
        }
        section(ui, "properties", "Properties", false, |ui| properties(app, ui, t, d, ids, rows, expert, runs));
        if d["element"].is_object() {
            let rels = d["relationships"].as_array().map(Vec::as_slice).unwrap_or_default();
            section(ui, "relationships", format!("Relationships ({})", rels.len()), false, |ui| relationships(app, ui, t, rels, expert, runs));
            let views = d["views"].as_array().map(Vec::as_slice).unwrap_or_default();
            section(ui, "views", "Views", false, |ui| views_section(app, ui, t, views, runs));
        }
        if d["shape"].is_object() {
            section(ui, "shape", "Shape", false, |ui| shape_section(app, ui, t, &d["shape"], expert, runs));
        }
    } else if !ids.is_empty() {
        section(ui, "properties", "Properties", false, |ui| properties(app, ui, t, &Value::Null, ids, rows, expert, runs));
    }
    if app.session.doc.page().is_some_and(|p| p.sheet.is_some()) {
        section(ui, "sheet", "Sheet", false, |ui| sheet_section(app, ui, t, ids));
    }
    section(ui, "document", "Document properties", st.show_document, |ui| document(app, ui, t, &extra.doc_props, runs));
}

/// A two-column row: dim label, then the widget.
fn field(ui: &mut Ui, t: &Tokens, label: &str, body: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(label).color(t.text_dim));
    body(ui);
    ui.end_row();
}

fn identity(app: &CraftApp, ui: &mut Ui, t: &Tokens, d: &Value, expert: bool, runs: &mut Runs) {
    let id = d["element"]["id"].as_str().unwrap_or_default().to_string();
    let name = d["element"]["name"].as_str().unwrap_or_default().to_string();
    let tag = d["element"]["tag"].as_str().unwrap_or_default().to_string();
    egui::Grid::new("inspector-identity").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        field(ui, t, "Type", |ui| {
            let label = d["type"]["label"].as_str().filter(|l| !l.is_empty()).or(d["element"]["type"].as_str()).unwrap_or_default();
            ui.label(label);
        });
        field(ui, t, "Name", |ui| {
            if let Some(n) = text_cell(ui, &format!("{id}|name"), &name, "", 150.0) {
                runs.push(("element.rename", json!({"id": id, "name": n})));
            }
        });
        field(ui, t, "Tag", |ui| {
            ui.horizontal(|ui| tag_field(app, ui, t, &id, &tag, runs));
        });
        if expert {
            field(ui, t, "Id", |ui| {
                ui.label(RichText::new(&id).monospace());
            });
            field(ui, t, "Owner", |ui| {
                ui.label(RichText::new(d["element"]["owner"].as_str().unwrap_or("(project)")).monospace());
            });
            field(ui, t, "Tag format", |ui| {
                let f = d["identity"]["format"].as_str().unwrap_or_default();
                ui.label(RichText::new(if f.is_empty() { "(none)" } else { f }).monospace());
            });
        }
    });
}

/// The tag editor: asks the engine whether the text is free on every change, colours it red and
/// names the element holding it when it is taken; Enter or leaving the field commits a free tag.
fn tag_field(app: &CraftApp, ui: &mut Ui, t: &Tokens, id: &str, tag: &str, runs: &mut Runs) {
    let key = egui::Id::new(("inspector-tag", id));
    let mut buf: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_else(|| tag.to_string());
    let verdict = if buf == tag { None } else { check(&app.session, id, &buf).ok() };
    let taken = verdict.as_ref().is_some_and(|v| v["free"] == false);
    let mut edit = egui::TextEdit::singleline(&mut buf).id(key.with("field")).desired_width(90.0);
    if taken {
        edit = edit.text_color(t.red);
    }
    let resp = ui.add(edit);
    if taken {
        // The red outline stands for the colour in every theme.
        ui.painter().rect_stroke(resp.rect, 2.0, egui::Stroke::new(1.0, t.red), egui::StrokeKind::Outside);
    }
    if resp.lost_focus() {
        let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if !escaped && !taken && !buf.is_empty() && buf != tag {
            runs.push(("element.retag", json!({"id": id, "tag": buf})));
        }
        ui.ctx().data_mut(|d| d.remove::<String>(key));
    } else if resp.has_focus() {
        ui.ctx().data_mut(|d| d.insert_temp(key, buf));
    } else {
        ui.ctx().data_mut(|d| d.remove::<String>(key));
    }
    if ui.small_button("Auto").on_hover_text("Give the element the next number of its scope").clicked() {
        runs.push(("element.retag", json!({"id": id})));
    }
    if let Some(c) = verdict.as_ref().filter(|_| taken).map(|v| &v["conflict"]) {
        let who = c["name"].as_str().filter(|n| !n.is_empty()).or(c["id"].as_str()).unwrap_or("another element");
        ui.label(RichText::new(format!("In use by {who}")).small().color(t.red));
    }
}

#[allow(clippy::too_many_arguments)]
fn properties(app: &CraftApp, ui: &mut Ui, t: &Tokens, d: &Value, ids: &[String], rows: &[PaneRow], expert: bool, runs: &mut Runs) {
    let mut show_hidden: bool = ui.ctx().data(|dd| dd.get_temp(show_hidden_id())).unwrap_or(false);
    let element = d["element"]["id"].as_str().map(str::to_string);
    if let Some(el) = &element {
        let props: Vec<&Value> = d["properties"]
            .as_array()
            .map(|a| a.iter().filter(|p| p["source"] == "schema" || (expert && p["source"] == "extra")).collect())
            .unwrap_or_default();
        if props.is_empty() {
            ui.label(RichText::new("The element type has no properties.").color(t.text_dim));
        } else {
            egui::Grid::new("inspector-props").num_columns(2).spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
                for p in props {
                    let key = p["key"].as_str().unwrap_or_default();
                    let mut label = p["label"].as_str().unwrap_or(key).to_string();
                    let empty = p["value"].is_null() || p["value"] == "";
                    let required = p["required"] == true;
                    if required {
                        label.push_str(" *");
                    }
                    let tip = format!("Key: {key}\nType: {}{}", p["kind"].as_str().unwrap_or("text"), if required { "\nRequired" } else { "" });
                    let color = if required && empty { t.red } else { t.text };
                    ui.label(RichText::new(label).color(color)).on_hover_text(tip);
                    if let Some(v) = prop_editor(ui, t, &format!("{el}|{key}"), p) {
                        runs.push(("element.setProperty", json!({"id": el, "key": key, "value": v})));
                    }
                    ui.end_row();
                }
            });
        }
        ui.separator();
    }
    if ids.is_empty() {
        return;
    }
    let doc = &app.session.doc;
    if let [one] = ids {
        let text = doc.shape(one).map(|s| s.text.lines().next().unwrap_or("").trim().to_string()).unwrap_or_default();
        let shown = if text.is_empty() { format!("Shape data of {one}") } else { format!("Shape data of {text}") };
        ui.label(RichText::new(shown).small().color(t.text_dim));
    } else {
        ui.label(RichText::new(format!("{} shapes: the keys they share", ids.len())).small().color(t.text_dim));
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button("Define...").on_hover_text("Define a data key on this page").clicked() {
            runs.push(("data.defineDialog", json!({})));
        }
        ui.checkbox(&mut show_hidden, "Show hidden");
    });
    let use_page = ids.iter().all(|id| doc.shape(id).is_some_and(|s| s.data.page_keys));
    let mut on = use_page;
    if ui.checkbox(&mut on, "Use page keys").on_hover_text("Give the shape every key this page defines, now and later").changed() {
        runs.push(("data.usePageKeys", json!({"ids": ids, "value": on})));
    }
    let visible: Vec<&PaneRow> = rows.iter().filter(|r| !r.row.model && (show_hidden || !r.row.hidden)).collect();
    if visible.is_empty() {
        ui.label(RichText::new("No shape data yet. Add a key below.").color(t.text_dim));
    } else {
        data_grid(ui, t, ids, &visible, runs);
    }
    ui.separator();
    add_form(ui, ids, runs);
    page_keys_section(app, ui, t, runs);
    ui.ctx().data_mut(|dd| dd.insert_temp(show_hidden_id(), show_hidden));
}

fn data_grid(ui: &mut Ui, t: &Tokens, ids: &[String], visible: &[&PaneRow], runs: &mut Runs) {
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
                if row.page_key { "\nDefined on this page" } else { "" }
            );
            let color = if row.required && row.value.is_empty() { t.red } else { t.text };
            ui.label(RichText::new(label).color(color)).on_hover_text(tip);
            let salt = format!("{}|{}", ids.join(","), row.key);
            if let Some(v) = value_cell(ui, t, &salt, r) {
                runs.push(("data.set", json!({"ids": ids, "key": row.key, "value": v})));
            }
            if ui.small_button("x").on_hover_text(format!("Remove {} from the selection", row.key)).clicked() {
                runs.push(("data.remove", json!({"ids": ids, "key": row.key})));
            }
            ui.end_row();
        }
    });
}

/// The editor for one element property, typed by its schema kind; returns the value to set
/// (`null` removes the property).
fn prop_editor(ui: &mut Ui, t: &Tokens, salt: &str, p: &Value) -> Option<Value> {
    let unit = p["unit"].as_str().unwrap_or_default();
    match p["kind"].as_str().unwrap_or("text") {
        "bool" => {
            let mut on = p["value"].as_bool().unwrap_or(false);
            let label = p["label"].as_str().unwrap_or_default().to_string();
            let resp = ui.checkbox(&mut on, "");
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, label.clone()));
            resp.changed().then_some(Value::Bool(on))
        }
        "enum" => {
            let current = p["value"].as_str().unwrap_or_default();
            let mut out = None;
            let shown = if current.is_empty() { "(none)" } else { current };
            egui::ComboBox::from_id_salt(("inspector-enum", salt)).selected_text(shown).width(140.0).show_ui(ui, |ui| {
                if ui.selectable_label(current.is_empty(), "(none)").clicked() {
                    out = Some(Value::Null);
                }
                for v in p["values"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(Value::as_str) {
                    if ui.selectable_label(current == v, v).clicked() {
                        out = Some(Value::String(v.to_string()));
                    }
                }
            });
            out
        }
        kind @ ("number" | "integer") => {
            let shown = p["value"].as_f64().map(data::fmt_number).unwrap_or_default();
            let mut out = None;
            ui.horizontal(|ui| {
                if let Some(text) = text_cell(ui, salt, &shown, if kind == "integer" { "whole number" } else { "number" }, 90.0) {
                    let text = text.trim();
                    out = if text.is_empty() {
                        Some(Value::Null)
                    } else {
                        text.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| if kind == "integer" { json!(f.trunc()) } else { json!(f) })
                    };
                }
                if !unit.is_empty() {
                    ui.label(RichText::new(unit).color(t.text_dim));
                }
            });
            out
        }
        _ => {
            let shown = p["value"].as_str().unwrap_or_default();
            text_cell(ui, salt, shown, "", 140.0).map(|s| if s.is_empty() { Value::Null } else { Value::String(s) })
        }
    }
}

fn relationships(app: &CraftApp, ui: &mut Ui, t: &Tokens, rels: &[Value], expert: bool, runs: &mut Runs) {
    if rels.is_empty() {
        ui.label(RichText::new("No relationships.").color(t.text_dim));
    }
    let active = app.session.doc.page().map(|p| p.id.clone()).unwrap_or_default();
    for r in rels {
        let arrow = if r["direction"] == "out" { "->" } else { "<-" };
        let other = r["otherName"].as_str().filter(|n| !n.is_empty()).or(r["other"].as_str()).unwrap_or_default();
        let mut text = format!("{} {arrow} {other}", r["label"].as_str().unwrap_or_default());
        if expert {
            text.push_str(&format!("  [{}]", r["id"].as_str().unwrap_or_default()));
        }
        let pages = r["pages"].as_array().map(Vec::as_slice).unwrap_or_default();
        let here = pages.iter().find(|p| p["page"].as_str() == Some(active.as_str()));
        let target = here.or(pages.first());
        let resp = ui.add_enabled(target.is_some(), egui::Button::new(text).frame(false));
        let resp = if target.is_none() { resp.on_disabled_hover_text("Not drawn on any page") } else { resp };
        if resp.clicked()
            && let Some(p) = target
        {
            if here.is_none() {
                runs.push(("page.setActive", json!({"page": p["page"]})));
            }
            runs.push(("edit.select", json!({"ids": p["items"]})));
        }
    }
}

fn views_section(app: &CraftApp, ui: &mut Ui, t: &Tokens, views: &[Value], runs: &mut Runs) {
    let active = app.session.doc.page().map(|p| p.id.clone()).unwrap_or_default();
    ui.label(RichText::new(format!("Appears in {} {}", views.len(), if views.len() == 1 { "view" } else { "views" })).color(t.text_dim));
    for v in views {
        let name = v["pageName"].as_str().unwrap_or_default();
        let current = v["page"].as_str() == Some(active.as_str());
        let text = if current { format!("{name} (this page)") } else { name.to_string() };
        if ui.add(egui::Button::new(text).frame(false)).on_hover_text("Go to this page and select the shape").clicked() {
            if !current {
                runs.push(("page.setActive", json!({"page": v["page"]})));
            }
            runs.push(("edit.select", json!({"ids": v["items"]})));
        }
    }
}

fn shape_section(app: &CraftApp, ui: &mut Ui, t: &Tokens, s: &Value, expert: bool, runs: &mut Runs) {
    let id = s["id"].as_str().unwrap_or_default().to_string();
    egui::Grid::new("inspector-shape").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        field(ui, t, "Kind", |ui| {
            ui.label(s["kind"].as_str().unwrap_or_default());
        });
        field(ui, t, "Stencil", |ui| {
            let st = s["stencil"].as_str().unwrap_or_default();
            ui.label(if st.is_empty() { "(built in)" } else { st });
        });
        field(ui, t, "Text", |ui| {
            let text = s["text"].as_str().unwrap_or_default();
            if let Some(new) = text_cell(ui, &format!("{id}|text"), text, "", 150.0) {
                runs.push(("shape.setText", json!({"id": id, "text": new})));
            }
        });
        for (key, label) in [("x", "X"), ("y", "Y"), ("w", "Width"), ("h", "Height")] {
            field(ui, t, label, |ui| {
                let mut v = s[key].as_f64().unwrap_or(0.0);
                let mut drag = egui::DragValue::new(&mut v).speed(1.0).max_decimals(1);
                if key == "w" || key == "h" {
                    drag = drag.range(1.0..=100_000.0);
                }
                if ui.add(drag).changed() && v.is_finite() {
                    runs.push(("arrange.setBounds", json!({"id": id, key: v})));
                }
            });
        }
        if expert {
            field(ui, t, "Angle", |ui| {
                ui.label(format!("{:.1}", s["angle"].as_f64().unwrap_or(0.0)));
            });
            field(ui, t, "Id", |ui| {
                ui.label(RichText::new(&id).monospace());
            });
            if let Some(shape) = app.session.doc.shape(&id) {
                for (k, v) in &shape.ext {
                    field(ui, t, &format!("ext {k}"), |ui| {
                        ui.label(RichText::new(v).monospace().color(t.text_dim));
                    });
                }
            }
        }
    });
}

fn document(app: &CraftApp, ui: &mut Ui, t: &Tokens, props: &Value, runs: &mut Runs) {
    ui.label(RichText::new("Document title").small().color(t.text_dim));
    // The one title: the title property when set, else the plain document title (old files).
    let title = vizcraft_diagram::text_fields::doc_value(&app.session.doc, app.session.doc.active_index(), "title");
    if let Some(new) = text_cell(ui, "document|title", &title, "", 220.0) {
        runs.push(("edit.setTitle", json!({"title": new})));
    }
    ui.add_space(4.0);
    egui::Grid::new("doc_props").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        let core = &app.session.doc.core;
        field(ui, t, "Revision", |ui| {
            ui.label(core.revision.to_string());
        });
        if !app.session.author.is_empty() {
            field(ui, t, "Author", |ui| {
                ui.label(&app.session.author);
            });
        }
        if !core.modified.is_empty() {
            field(ui, t, "Saved", |ui| {
                ui.label(&core.modified);
            });
            field(ui, t, "By", |ui| {
                ui.label(&core.last_modified_by);
            });
        }
    });
    let has_sheet = app.session.doc.pages.iter().any(|p| p.sheet.is_some());
    doc_property_form(ui, t, props, has_sheet, runs);
}

/// What the Inspector reads through read-only commands (`glyph.params`, `doc.properties`), kept
/// in egui's memory with the document revision it was read at, so a frame runs a command only
/// after the document or the selection changed.
pub struct Extra {
    /// `glyph.params` of the one selected shape, when a glyph builder draws it.
    pub symbol: Option<Value>,
    /// The one selected connector and its net as `net.of` gives it (`None` off any net).
    pub connector: Option<(String, Option<Value>)>,
    /// `doc.properties`: the title block fields every loaded pack declares.
    pub doc_props: Value,
    /// `key.get` of the active page: its key's id, rows and excluded ids (id null without a key).
    pub key: Value,
}

impl Extra {
    /// Read what the Inspector shows for `ids` (the selected shapes) and the selected connector.
    pub fn gather(app: &mut CraftApp, ctx: &egui::Context, ids: &[String]) -> Extra {
        let rev = app.session.rev();
        let sel_connector = match app.session.sel.items.as_slice() {
            [one] if app.session.doc.page().is_some_and(|p| p.connector(one).is_some()) => Some(one.clone()),
            _ => None,
        };
        // Only a shape a builder draws is asked for its parameters: `glyph.params` on any other
        // shape fails, and a failed command run leaves its error in the status bar.
        let symbol_id = match ids {
            [one] if drawn_by_builder(app, one) => Some(one.clone()),
            _ => None,
        };
        let mut read = |key: String, command: &str, params: Value| -> Value {
            let id = egui::Id::new(("inspector-read", key));
            if let Some((r, v)) = ctx.data(|d| d.get_temp::<(u64, Value)>(id))
                && r == rev
            {
                return v;
            }
            let v = app.session.run(command, &params).unwrap_or(Value::Null);
            ctx.data_mut(|d| d.insert_temp(id, (rev, v.clone())));
            v
        };
        let symbol = symbol_id.map(|one| read(format!("glyph|{one}"), "glyph.params", json!({"id": one}))).filter(|v| v.is_object());
        let doc_props = read("doc".into(), "doc.properties", json!({}));
        let key = read("key".into(), "key.get", json!({}));
        let connector = sel_connector.map(|one| {
            let net = read(format!("net|{one}"), "net.of", json!({"connector": one}));
            let net = Some(net["net"].clone()).filter(Value::is_object);
            (one, net)
        });
        Extra { symbol, connector, doc_props, key }
    }
}

/// Whether a glyph builder draws shape `id` on the active page (its own `Shape.glyph`, or its
/// element type's glyph), the same test `glyph.params` makes.
fn drawn_by_builder(app: &CraftApp, id: &str) -> bool {
    let doc = &app.session.doc;
    let Some(page) = doc.page() else { return false };
    let Some(s) = page.shape(id) else { return false };
    let element = s.element.as_deref().and_then(|e| doc.model.element(e));
    let variant = vizcraft_stencils::builders::sync::page_variant(page);
    vizcraft_stencils::builders::apply::resolve(s, element, vizcraft_notation::catalog(), variant).is_some()
}

/// What `ui.inspect` reports under `inspector`: the symbol parameters of the one selected shape
/// (`{id, builder, params: {key: {value, from}}}`), the active page's sheet (`{preset, standard,
/// size, orientation, zones: {cols, rows}, titleBlock, number, count}`) and the zone of the
/// selected shape's centre.
pub fn inspect_fields(app: &CraftApp) -> Value {
    let doc = &app.session.doc;
    let page = doc.page();
    let one = match app.session.sel.items.as_slice() {
        [one] => page.and_then(|p| p.shape(one)),
        _ => None,
    };
    let symbol = one.zip(page).and_then(|(s, p)| {
        let element = s.element.as_deref().and_then(|e| doc.model.element(e));
        let variant = vizcraft_stencils::builders::sync::page_variant(p);
        let r = vizcraft_stencils::builders::apply::resolve(s, element, vizcraft_notation::catalog(), variant)?;
        let params: serde_json::Map<String, Value> = r
            .layers
            .iter()
            .map(|(k, v, from)| {
                let value = match v {
                    vizcraft_diagram::vizcraft_model::PropValue::Bool(b) => json!(b),
                    vizcraft_diagram::vizcraft_model::PropValue::Number(n) => json!(n),
                    vizcraft_diagram::vizcraft_model::PropValue::Text(t) => json!(t),
                };
                (k.to_string(), json!({"value": value, "from": from.name()}))
            })
            .collect();
        Some(json!({"id": s.id, "builder": r.spec.name, "params": params}))
    });
    let sheet = page.and_then(|p| p.sheet).map(|sh| {
        let i = doc.active_index();
        json!({
            "preset": sh.preset_id(), "standard": sh.standard.label(), "size": sh.size.paper().name,
            "orientation": if sh.orientation == vizcraft_diagram::Orientation::Portrait { "portrait" } else { "landscape" },
            "zones": {"cols": sh.zones.cols, "rows": sh.zones.rows}, "titleBlock": sh.title_block.name(),
            "number": vizcraft_diagram::sheet::sheet_number(doc, i), "count": vizcraft_diagram::sheet::sheet_count(doc),
        })
    });
    let zone = one
        .zip(page)
        .and_then(|(s, p)| vizcraft_diagram::sheet::zone_of(p, vizcraft_diagram::geom::Point::new(s.x + s.w / 2.0, s.y + s.h / 2.0)))
        .map(|z| z.to_string());
    json!({"symbol": symbol, "sheet": sheet, "zone": zone})
}

/// The selected connector's net: its number and name edited in place through `net.set`, its
/// kind and pins, and a link to the Nets tab; a connector off any net can start one.
fn net_section(ui: &mut Ui, t: &Tokens, connector: &str, net: Option<&Value>, runs: &mut Runs) {
    let Some(net) = net else {
        ui.label(RichText::new("This connector is on no net.").color(t.text_dim));
        if ui.button("Make a wire net").on_hover_text("Make this connector the first segment of a new net").clicked() {
            runs.push(("net.assign", json!({"ids": [connector], "kind": "wire"})));
        }
        return;
    };
    let id = net["id"].as_str().unwrap_or_default().to_string();
    egui::Grid::new("inspector-net").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        field(ui, t, "Net", |ui| {
            ui.label(RichText::new(&id).monospace());
        });
        field(ui, t, "Number", |ui| {
            let number = net["number"].as_str().unwrap_or_default();
            if let Some(n) = text_cell(ui, &format!("net|{id}|number"), number, "none", 100.0) {
                runs.push(("net.set", json!({"net": id, "number": n.trim()})));
            }
        });
        field(ui, t, "Name", |ui| {
            let name = net["name"].as_str().unwrap_or_default();
            if let Some(n) = text_cell(ui, &format!("net|{id}|name"), name, "potential, e.g. L1", 100.0) {
                runs.push(("net.set", json!({"net": id, "name": n.trim()})));
            }
        });
        field(ui, t, "Kind", |ui| {
            ui.label(net["kind"].as_str().unwrap_or_default());
        });
        field(ui, t, "Pins", |ui| {
            ui.label(net["pins"].as_array().map_or(0, Vec::len).to_string());
        });
        field(ui, t, "Segments", |ui| {
            ui.label(net["segments"].as_array().map_or(0, Vec::len).to_string());
        });
    });
    if ui.button("Show in Nets").on_hover_text("Open the Nets tab of the dock on this net").clicked() {
        runs.push(("ui.nets", json!({"net": id})));
    }
}

/// The parameters of a symbol drawn by a glyph builder (`glyph.params`), one editor per
/// parameter kind. A key the element owns (its value came from the element, or the element's
/// type declares it) is written with `element.setProperty`, so the model keeps it; any other is
/// the shape's own override, written with `glyph.setParams`.
fn symbol_section(ui: &mut Ui, t: &Tokens, d: &Value, sym: &Value, runs: &mut Runs) {
    let shape = sym["id"].as_str().unwrap_or_default().to_string();
    let element = d["element"]["id"].as_str().map(str::to_string);
    let declared: Vec<&str> =
        d["properties"].as_array().map(|a| a.iter().filter(|p| p["source"] == "schema").filter_map(|p| p["key"].as_str()).collect()).unwrap_or_default();
    let builder = sym["builder"].as_str().unwrap_or_default();
    let glyph = sym["glyph"].as_str().unwrap_or("");
    let what = if glyph.is_empty() { format!("Drawn by the {builder} builder") } else { format!("Drawn by the {builder} builder ({glyph})") };
    ui.label(RichText::new(what).small().color(t.text_dim));
    let params = sym["params"].as_array().cloned().unwrap_or_default();
    egui::Grid::new("inspector-symbol").num_columns(3).spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
        for p in &params {
            let key = p["key"].as_str().unwrap_or_default();
            let spec = &p["spec"];
            let from = p["from"].as_str().unwrap_or("default");
            let label = spec["label"].as_str().filter(|l| !l.is_empty()).unwrap_or(key);
            let tip = format!("Key: {key}\nFrom: {from}\n{}", spec["description"].as_str().unwrap_or_default());
            ui.label(RichText::new(label).color(if from == "default" { t.text_dim } else { t.text })).on_hover_text(tip);
            let owned = element.as_ref().filter(|_| from == "element" || declared.contains(&key));
            if let Some(v) = param_editor(ui, t, &format!("{shape}|sym|{key}"), spec, &p["value"]) {
                match owned {
                    Some(el) => runs.push(("element.setProperty", json!({"id": el, "key": key, "value": v}))),
                    None => runs.push(("glyph.setParams", json!({"ids": [shape], "params": {key: v}}))),
                }
            }
            if from == "shape" && owned.is_none() {
                if ui.small_button("x").on_hover_text("Clear this shape's override").clicked() {
                    runs.push(("glyph.setParams", json!({"ids": [shape], "params": {key: null}})));
                }
            } else {
                ui.label("");
            }
            ui.end_row();
        }
    });
    if ui.button("Rebuild").on_hover_text("Redraw the symbol at its true size from its parameters").clicked() {
        runs.push(("glyph.rebuild", json!({"ids": [shape]})));
    }
}

/// The editor for one builder parameter, by its spec kind (`choice`, `int`, `number`, `bool`,
/// `text`); returns the value to set.
fn param_editor(ui: &mut Ui, t: &Tokens, salt: &str, spec: &Value, value: &Value) -> Option<Value> {
    match spec["kind"].as_str().unwrap_or("text") {
        "bool" => {
            let mut on = value.as_bool().unwrap_or(false);
            let label = spec["label"].as_str().unwrap_or_default().to_string();
            let resp = ui.checkbox(&mut on, "");
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, label.clone()));
            resp.changed().then_some(Value::Bool(on))
        }
        "choice" => {
            let current = value.as_str().unwrap_or_default().to_string();
            let mut out = None;
            egui::ComboBox::from_id_salt(("inspector-choice", salt)).selected_text(current.clone()).width(120.0).show_ui(ui, |ui| {
                for c in spec["choices"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(Value::as_str) {
                    if ui.selectable_label(current == c, c).clicked() && current != c {
                        out = Some(json!(c));
                    }
                }
            });
            out
        }
        kind @ ("int" | "number") => {
            let mut v = value.as_f64().unwrap_or(0.0);
            let min = spec["min"].as_f64().unwrap_or(f64::MIN);
            let max = spec["max"].as_f64().unwrap_or(f64::MAX);
            let label = spec["label"].as_str().unwrap_or_default().to_string();
            let int = kind == "int";
            let drag = egui::DragValue::new(&mut v).range(min..=max).speed(if int { 0.1 } else { 0.05 }).max_decimals(if int { 0 } else { 2 });
            let resp = ui.add(drag);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, label.clone()));
            // A drag commits when it ends, so a slow drag is one change, not one per frame.
            let done = resp.drag_stopped() || (resp.changed() && !resp.dragged());
            (done && v.is_finite() && Some(v) != value.as_f64()).then(|| if int { json!(v.round() as i64) } else { json!(v) })
        }
        _ => {
            let shown = value.as_str().unwrap_or_default();
            let _ = t;
            text_cell(ui, salt, shown, "", 120.0).map(Value::String)
        }
    }
}

/// The active page's drawing sheet: standard, size and orientation, the zone grid, the sheet
/// number and the zone the selection sits in. Changed from Design > Sheet.
fn sheet_section(app: &CraftApp, ui: &mut Ui, t: &Tokens, ids: &[String]) {
    let doc = &app.session.doc;
    let Some(page) = doc.page() else { return };
    let Some(sh) = page.sheet else { return };
    let paper = sh.size.paper();
    let orientation = if sh.orientation == vizcraft_diagram::Orientation::Portrait { "portrait" } else { "landscape" };
    egui::Grid::new("inspector-sheet").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        field(ui, t, "Standard", |ui| {
            ui.label(sh.standard.label());
        });
        field(ui, t, "Size", |ui| {
            ui.label(format!("{} {orientation}", paper.name)).on_hover_text(format!("Preset {}", sh.preset_id()));
        });
        field(ui, t, "Zones", |ui| {
            ui.label(format!("{} columns by {} rows", sh.zones.cols, sh.zones.rows));
        });
        field(ui, t, "Sheet", |ui| {
            let i = doc.active_index();
            ui.label(format!("{} of {}", vizcraft_diagram::sheet::sheet_number(doc, i), vizcraft_diagram::sheet::sheet_count(doc)));
        });
        field(ui, t, "Title block", |ui| {
            ui.label(match sh.title_block {
                vizcraft_diagram::sheet::TitleBlockKind::Iso7200 => "ISO 7200",
                vizcraft_diagram::sheet::TitleBlockKind::Asme => "ASME Y14.1",
                vizcraft_diagram::sheet::TitleBlockKind::None => "None",
            });
        });
        if let Some(s) = ids.first().and_then(|id| page.shape(id)) {
            let c = vizcraft_diagram::geom::Point::new(s.x + s.w / 2.0, s.y + s.h / 2.0);
            if let Some(z) = vizcraft_diagram::sheet::zone_of(page, c) {
                field(ui, t, "Zone", |ui| {
                    ui.label(z.to_string()).on_hover_text("The zone of the selected shape's centre");
                });
            }
        }
    });
}

/// The document property form: the title block fields `doc.properties` lists, grouped as the
/// packs group them, each edited through `doc.setProperty`. A required field left empty and a
/// value over its length are marked in red. Open by default once a page has a drawing sheet.
fn doc_property_form(ui: &mut Ui, t: &Tokens, props: &Value, has_sheet: bool, runs: &mut Runs) {
    let rows = props["properties"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("Title block fields ({})", rows.len()))
        // Keyed by whether a sheet exists, so the form opens by itself once the page gets one.
        .id_salt(("inspector-doc-fields", has_sheet))
        .default_open(has_sheet)
        .show(ui, |ui| {
            let mut group = String::new();
            for r in &rows {
                let key = r["key"].as_str().unwrap_or_default();
                if key == "title" {
                    // The title is edited above.
                    continue;
                }
                let g = r["group"].as_str().unwrap_or_default();
                if g != group {
                    group = g.to_string();
                    ui.add_space(2.0);
                    ui.label(RichText::new(if g.is_empty() { "Other" } else { g }).small().strong().color(t.text_dim));
                }
                ui.horizontal(|ui| {
                    let mut label = r["label"].as_str().filter(|l| !l.is_empty()).unwrap_or(key).to_string();
                    if r["required"] == true {
                        label.push_str(" *");
                    }
                    let issue = r["issue"].as_str();
                    let color = if issue.is_some() { t.red } else { t.text };
                    let tip = format!(
                        "Key: {key}{}{}",
                        r["maxChars"].as_u64().map(|m| format!("\nAt most {m} characters")).unwrap_or_default(),
                        issue.map(|i| format!("\n{i}")).unwrap_or_default()
                    );
                    ui.add_sized([110.0, 18.0], egui::Label::new(RichText::new(label).color(color)).truncate()).on_hover_text(tip);
                    if let Some(v) = doc_field_editor(ui, key, r) {
                        runs.push(("doc.setProperty", json!({"key": key, "value": v})));
                    }
                });
            }
        });
}

/// The editor for one document property row; returns the value to set (`null` clears).
fn doc_field_editor(ui: &mut Ui, key: &str, r: &Value) -> Option<Value> {
    let value = r["value"].as_str().unwrap_or_default().to_string();
    let values: Vec<&str> = r["values"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if !values.is_empty() {
        let mut out = None;
        let shown = if value.is_empty() { "(none)" } else { value.as_str() };
        egui::ComboBox::from_id_salt(("inspector-doc-enum", key)).selected_text(shown).width(130.0).show_ui(ui, |ui| {
            if ui.selectable_label(value.is_empty(), "(none)").clicked() {
                out = Some(Value::Null);
            }
            for v in values {
                if ui.selectable_label(value == v, v).clicked() {
                    out = Some(json!(v));
                }
            }
        });
        return out;
    }
    if r["kind"] == "boolean" {
        let mut on = value == "true";
        return ui.checkbox(&mut on, "").changed().then_some(Value::Bool(on));
    }
    let text = text_cell(ui, &format!("document|{key}"), &value, "", 130.0)?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Some(Value::Null);
    }
    Some(match r["kind"].as_str() {
        Some("number" | "whole number") => text.parse::<f64>().ok().filter(|f| f.is_finite()).map_or(json!(text), |f| json!(f)),
        _ => json!(text),
    })
}

fn hex(c: vizcraft_diagram::Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// The Format tab: fill, line and text of the selected shapes, from the first one's style.
fn format_tab(app: &CraftApp, ui: &mut Ui, t: &Tokens, ids: &[String], runs: &mut Runs) {
    let doc = &app.session.doc;
    let ids: Vec<String> = ids.to_vec();
    let Some(shape) = ids.first().and_then(|id| doc.shape(id)) else {
        ui.label(RichText::new("Select a shape to format it.").color(t.text_dim));
        return;
    };
    let st = shape.style.clone();
    let rgb = |c: vizcraft_diagram::Color| [c.r, c.g, c.b];
    section(ui, "format-shape", "Fill and line", false, |ui| {
        egui::Grid::new("inspector-format-shape").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
            field(ui, t, "Fill", |ui| {
                let mut c = rgb(st.fill);
                if ui.color_edit_button_srgb(&mut c).changed() {
                    runs.push(("shape.setStyle", json!({"ids": ids, "fill": hex(vizcraft_diagram::Color { r: c[0], g: c[1], b: c[2], a: 255 })})));
                }
            });
            field(ui, t, "Line", |ui| {
                let mut c = rgb(st.line);
                if ui.color_edit_button_srgb(&mut c).changed() {
                    runs.push(("shape.setStyle", json!({"ids": ids, "line": hex(vizcraft_diagram::Color { r: c[0], g: c[1], b: c[2], a: 255 })})));
                }
            });
            field(ui, t, "Line width", |ui| {
                let mut w = st.line_width;
                if ui.add(egui::DragValue::new(&mut w).speed(0.1).range(0.0..=100.0).max_decimals(1)).changed() && w.is_finite() {
                    runs.push(("shape.setStyle", json!({"ids": ids, "lineWidth": w})));
                }
            });
        });
    });
    section(ui, "format-text", "Text", false, |ui| {
        egui::Grid::new("inspector-format-text").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
            field(ui, t, "Color", |ui| {
                let mut c = rgb(st.font_color);
                if ui.color_edit_button_srgb(&mut c).changed() {
                    runs.push(("text.setFormat", json!({"ids": ids, "color": hex(vizcraft_diagram::Color { r: c[0], g: c[1], b: c[2], a: 255 })})));
                }
            });
            field(ui, t, "Size", |ui| {
                let mut sz = st.font_size;
                if ui.add(egui::DragValue::new(&mut sz).speed(0.5).range(1.0..=400.0).max_decimals(1)).changed() && sz.is_finite() {
                    runs.push(("text.setFormat", json!({"ids": ids, "size": sz})));
                }
            });
        });
        ui.horizontal(|ui| {
            for (key, label, on) in [("bold", "Bold", st.bold), ("italic", "Italic", st.italic), ("underline", "Underline", st.underline), ("strikethrough", "Strike", st.strikethrough)] {
                let mut v = on;
                if ui.checkbox(&mut v, label).changed() {
                    runs.push(("text.setFormat", json!({"ids": ids, key: v})));
                }
            }
        });
    });
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
        a.run("view.shapeDataPane", json!({"value": false})).unwrap();
        assert!(!a.ui.panes.is_open("shapeData"));
        a.run("view.shapeDataPane", json!({"value": true})).unwrap();
        assert!(a.ui.panes.is_open("shapeData"));
    }

    #[test]
    fn ui_inspector_sets_mode_and_tab_and_refuses_junk() {
        let mut a = app();
        let r = a.run("ui.inspector", json!({"mode": "expert", "tab": "format"})).unwrap();
        assert_eq!((r["mode"].as_str(), r["tab"].as_str()), (Some("expert"), Some("format")));
        assert_eq!(a.ui.inspector.mode, InspectorMode::Expert);
        assert!(a.run("ui.inspector", json!({"mode": "loud"})).is_err());
        assert!(a.run("ui.inspector", json!({"tab": 5})).is_ok(), "a non-string value is ignored");
        assert_eq!(a.ui.inspector.tab, InspectorTab::Format);
        let saved = serde_json::to_string(&a.prefs()).unwrap();
        assert!(saved.contains("expert"));
    }
}
