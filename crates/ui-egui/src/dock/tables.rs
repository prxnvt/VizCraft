//! The Tables tab: lists of the model's elements or relationships and relationship matrices, read
//! with `table.list` and `table.matrix`, edited in place with `table.set` and `table.toggle` and
//! written out with `table.export`. The tab holds only the view being shown and the cell being
//! edited; every number and every edit comes from the commands.
//!
//! The result of the last read is kept with the document revision and the view it was read for, so
//! a frame costs no command run until the document or the view changes.

use std::collections::HashSet;
use std::rc::Rc;

use egui::{Sense, Ui};
use serde_json::{Value, json};

use super::DockTab;
use crate::CraftApp;
use crate::theme::Tokens;

/// Height of a table row, in points.
const ROW_H: f32 = 20.0;
/// Width of a matrix column and of the row header column, in points.
const MATRIX_COL_W: f32 = 70.0;
const MATRIX_HEAD_W: f32 = 160.0;
/// Narrowest and widest list column, in points.
const MIN_COL_W: f32 = 56.0;
const MAX_COL_W: f32 = 280.0;

/// A cell being edited: the item and column it belongs to and the text typed so far.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    id: String,
    key: String,
    text: String,
    fresh: bool,
}

/// One relationship type that declares a text property, offered as a RACI-style matrix.
#[derive(Clone, Debug, PartialEq)]
struct Raci {
    label: String,
    rows: String,
    cols: String,
    relationship: String,
    property: String,
}

/// What the last read was for and what it returned.
#[derive(Clone, Debug, PartialEq)]
struct Cache {
    rev: u64,
    key: String,
    /// Element types in use as (id, label), relationship types in use likewise.
    types: Vec<(String, String)>,
    links: Vec<(String, String)>,
    raci: Vec<Raci>,
    view: Result<Rc<Value>, String>,
}

/// The Tables tab's state: which view, its parameters, the filter, the cell being edited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TablesState {
    /// A matrix instead of a list.
    pub matrix: bool,
    /// List: the element or relationship type; empty lists every element.
    pub type_id: String,
    /// Matrix: the row element type.
    pub rows: String,
    /// Matrix: the column element type.
    pub cols: String,
    /// Matrix: the relationship type a cell shows and a click toggles.
    pub relationship: String,
    /// Matrix: the property a cell reads (RACI), empty for marks.
    pub property: String,
    /// List: only rows with a cell containing this text.
    pub filter: String,
    edit: Option<Edit>,
    cache: Option<Cache>,
}

impl TablesState {
    /// The parameters of the view, for `table.list`, `table.matrix` and `table.export`.
    fn params(&self) -> Value {
        if self.matrix {
            let mut p = json!({"view": "matrix", "rows": self.rows, "cols": self.cols});
            if !self.relationship.is_empty() {
                p["relationship"] = json!(self.relationship);
            }
            if !self.property.is_empty() {
                p["property"] = json!(self.property);
            }
            p
        } else {
            let mut p = json!({"view": "list"});
            if !self.type_id.is_empty() {
                p["type"] = json!(self.type_id);
            }
            if !self.filter.trim().is_empty() {
                p["filter"] = json!(self.filter);
            }
            p
        }
    }

    fn key(&self) -> String {
        self.params().to_string()
    }
}

/// What a frame asked for, applied after the grid is drawn.
enum Act {
    Select(String),
    Set(Edit),
    Toggle(String, String),
    Export(&'static str),
}

/// The label of a type: the catalog's, else the id after the pack prefix.
fn type_label(id: &str, relationship: bool) -> String {
    let cat = vizcraft_notation::catalog();
    let known =
        if relationship { cat.relationship_type(id).map(|(_, t)| t.label.clone()) } else { cat.element_type(id).map(|(_, t)| t.label.clone()) };
    known.filter(|l| !l.is_empty()).unwrap_or_else(|| id.rsplit('.').next().unwrap_or(id).to_string())
}

/// The types a menu offers and the RACI entries, from the model as it is now.
fn survey(app: &CraftApp) -> (Vec<(String, String)>, Vec<(String, String)>, Vec<Raci>) {
    let m = &app.session.doc.model;
    let mut types: Vec<(String, String)> = Vec::new();
    for e in &m.elements {
        if !types.iter().any(|(t, _)| *t == e.type_id) {
            types.push((e.type_id.clone(), type_label(&e.type_id, false)));
        }
    }
    let mut links: Vec<(String, String)> = Vec::new();
    let mut raci: Vec<Raci> = Vec::new();
    for r in &m.relationships {
        if links.iter().any(|(t, _)| *t == r.type_id) {
            continue;
        }
        links.push((r.type_id.clone(), type_label(&r.type_id, true)));
        let text = vizcraft_notation::catalog()
            .relationship_type(&r.type_id)
            .and_then(|(_, rt)| rt.properties.iter().find(|p| p.kind == vizcraft_notation::schema::PropKind::Text).map(|p| p.key.clone()));
        let type_of = |id: &str| m.element(id).map(|e| e.type_id.clone());
        if let (Some(property), Some(rows), Some(cols)) = (text, type_of(&r.source), type_of(&r.target)) {
            raci.push(Raci {
                label: format!("{} by {property}", type_label(&r.type_id, true)),
                rows,
                cols,
                relationship: r.type_id.clone(),
                property,
            });
        }
    }
    (types, links, raci)
}

/// Read the view again when the document or the view changed since the last read.
fn refresh(app: &mut CraftApp) {
    // The table commands are off for an empty model; the tab says so instead of asking.
    if app.session.doc.model.elements.is_empty() && app.session.doc.model.relationships.is_empty() {
        app.ui.dock.state.tables.cache = None;
        return;
    }
    let rev = app.session.rev();
    let key = app.ui.dock.state.tables.key();
    if app.ui.dock.state.tables.cache.as_ref().is_some_and(|c| c.rev == rev && c.key == key) {
        return;
    }
    let (types, links, raci) = survey(app);
    let st = &mut app.ui.dock.state.tables;
    // A matrix with no types chosen starts on the first element type, with the only relationship
    // type in use when there is one.
    if st.matrix {
        if st.rows.is_empty() {
            st.rows = types.first().map(|t| t.0.clone()).unwrap_or_default();
        }
        if st.cols.is_empty() {
            st.cols = st.rows.clone();
        }
        if st.relationship.is_empty() && links.len() == 1 {
            st.relationship = links.first().map(|t| t.0.clone()).unwrap_or_default();
        }
    }
    let (command, params) = (if st.matrix { "table.matrix" } else { "table.list" }, st.params());
    let key = st.key();
    let view = app.session.run(command, &params).map(Rc::new).map_err(|e| e.to_string());
    app.ui.dock.state.tables.cache = Some(Cache { rev, key, types, links, raci, view });
}

/// The ids of the model items the selection draws: the selected items themselves, the elements
/// of selected shapes and the relationships of selected connectors.
fn selected_ids(app: &CraftApp) -> HashSet<String> {
    let doc = &app.session.doc;
    let mut out: HashSet<String> = HashSet::new();
    for id in &app.session.sel.items {
        out.insert(id.clone());
        if let Some(e) = vizcraft_diagram::views::element_of_shape(doc, id) {
            out.insert(e.to_string());
        }
        if let Some(r) = doc.connector(id).and_then(|c| c.relationship.clone()) {
            out.insert(r);
        }
    }
    out
}

/// Go to a page that draws the item and select what draws it.
fn select_item(app: &mut CraftApp, id: &str) {
    let views = vizcraft_diagram::views::views_of(&app.session.doc, id);
    let active = app.session.doc.active_index();
    let Some(v) = views.iter().find(|v| v.page == active).or(views.first()) else {
        app.status("Not drawn on any page");
        return;
    };
    let (page, items) = (v.page_id.clone(), v.items.clone());
    if v.page != active {
        let _ = app.run("page.setActive", json!({"page": page}));
    }
    let _ = app.run("edit.select", json!({"ids": items}));
}

/// Whether a column holds something a cell edit can change.
fn editable(key: &str) -> bool {
    !matches!(key, "id" | "type" | "source" | "target")
}

/// The Tables tab.
pub(super) fn show(app: &mut CraftApp, ui: &mut Ui) {
    refresh(app);
    let t = Tokens::get(ui.ctx());
    let mut act: Option<Act> = None;
    toolbar(app, ui, &mut act);
    ui.separator();
    let Some(cache) = app.ui.dock.state.tables.cache.clone() else {
        ui.add_space(12.0);
        ui.label(egui::RichText::new("Tables").strong());
        ui.label(egui::RichText::new("The model has no elements yet. Draw notation shapes or add elements to list them here.").color(t.text_dim));
        return;
    };
    match &cache.view {
        Err(e) => {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(e).color(t.text_dim));
        }
        Ok(v) if app.ui.dock.state.tables.matrix => matrix_body(app, ui, v, &mut act),
        Ok(v) => list_body(app, ui, v, &mut act),
    }
    apply(app, act);
}

/// Do what a frame asked for.
fn apply(app: &mut CraftApp, act: Option<Act>) {
    match act {
        None => {}
        Some(Act::Select(id)) => select_item(app, &id),
        Some(Act::Set(e)) => {
            if let Err(msg) = app.run("table.set", json!({"id": e.id, "key": e.key, "value": e.text})) {
                app.status(msg);
            }
        }
        Some(Act::Toggle(row, col)) => {
            let rel = app.ui.dock.state.tables.relationship.clone();
            if rel.is_empty() {
                app.status("Choose a relationship type for the matrix first");
            } else if let Err(msg) = app.run("table.toggle", json!({"row": row, "col": col, "relationship": rel})) {
                app.status(msg);
            }
        }
        Some(Act::Export(format)) => export(app, format),
    }
}

/// Write the view as `format`: to a path from the file picker, or as a download where the web
/// build has no picker (CSV and Markdown; a workbook needs a path).
fn export(app: &mut CraftApp, format: &'static str) {
    let ext = match format {
        "markdown" => "md",
        f => f,
    };
    let name = format!("{} - Tables.{ext}", app.title_stem());
    let mut params = app.ui.dock.state.tables.params();
    params["format"] = json!(format);
    if let Some(pick) = app.services.pick_save.as_ref() {
        let Some(path) = pick(&name) else { return };
        params["path"] = json!(path);
    } else if app.services.download.is_none() || format == "xlsx" {
        app.status("This build has no file picker; run table.export with a path instead");
        return;
    }
    match app.run("table.export", params) {
        Ok(r) => {
            if let (Some(content), Some(download)) = (r.get("content").and_then(Value::as_str), app.services.download.as_ref()) {
                download(&name, content.as_bytes());
            }
            let rows = r.get("rows").and_then(Value::as_array).map_or(0, Vec::len);
            app.status(format!("Exported {rows} row(s)"));
        }
        Err(e) => app.status(e),
    }
}

/// The View and Export menus, the filter and the count.
fn toolbar(app: &mut CraftApp, ui: &mut Ui, act: &mut Option<Act>) {
    let t = Tokens::get(ui.ctx());
    let Some(cache) = app.ui.dock.state.tables.cache.clone() else { return };
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let st = &mut app.ui.dock.state.tables;
        let title = if st.matrix {
            format!("Matrix: {} by {}", type_label(&st.rows, false), type_label(&st.cols, false))
        } else if st.type_id.is_empty() {
            "All elements".to_string()
        } else {
            type_label(&st.type_id, st.type_id.contains("Flow") && cache.links.iter().any(|(l, _)| *l == st.type_id))
        };
        ui.menu_button(format!("View: {title}"), |ui| {
            if ui.button("All elements").clicked() {
                st.matrix = false;
                st.type_id.clear();
                ui.close();
            }
            for (id, label) in &cache.types {
                if ui.button(label).clicked() {
                    st.matrix = false;
                    st.type_id = id.clone();
                    ui.close();
                }
            }
            if !cache.links.is_empty() {
                ui.separator();
                for (id, label) in &cache.links {
                    if ui.button(format!("{label} relationships")).clicked() {
                        st.matrix = false;
                        st.type_id = id.clone();
                        ui.close();
                    }
                }
            }
            ui.separator();
            ui.menu_button("Relationship Matrix", |ui| {
                if ui.button("Show Matrix").clicked() {
                    st.matrix = true;
                    ui.close();
                }
                ui.separator();
                for (label, which) in [("Rows", 0), ("Columns", 1), ("Relationship", 2)] {
                    ui.menu_button(label, |ui| {
                        let list = if which == 2 { &cache.links } else { &cache.types };
                        for (id, name) in list {
                            if ui.button(name).clicked() {
                                st.matrix = true;
                                match which {
                                    0 => st.rows = id.clone(),
                                    1 => st.cols = id.clone(),
                                    _ => st.relationship = id.clone(),
                                }
                                ui.close();
                            }
                        }
                    });
                }
                if ui.button("Cells show marks").clicked() {
                    st.matrix = true;
                    st.property.clear();
                    ui.close();
                }
            });
            for r in &cache.raci {
                if ui.button(format!("RACI: {}", r.label)).clicked() {
                    (st.matrix, st.rows, st.cols) = (true, r.rows.clone(), r.cols.clone());
                    (st.relationship, st.property) = (r.relationship.clone(), r.property.clone());
                    ui.close();
                }
            }
        });
        ui.menu_button("Export", |ui| {
            for (label, format) in [("CSV", "csv"), ("XLSX", "xlsx"), ("Markdown", "markdown")] {
                if ui.button(label).clicked() {
                    *act = Some(Act::Export(format));
                    ui.close();
                }
            }
        });
        let st = &mut app.ui.dock.state.tables;
        if !st.matrix {
            ui.add(egui::TextEdit::singleline(&mut st.filter).hint_text("Filter").desired_width(120.0));
        }
        let count = count_of(&cache);
        ui.label(egui::RichText::new(if st.matrix { format!("{count} link(s)") } else { format!("{count} row(s)") }).color(t.text_dim));
    });
}

/// Rows of a list, or marked cells of a matrix, in the last read.
fn count_of(cache: &Cache) -> usize {
    cache.view.as_ref().ok().and_then(|v| v.get("count")).and_then(Value::as_u64).map_or(0, |n| n as usize)
}

/// The text of a JSON string, or empty.
fn text_of(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// The width of each list column: the longer of the header and the first rows' cells.
fn widths(columns: &[Value], rows: &[Value]) -> Vec<f32> {
    columns
        .iter()
        .enumerate()
        .map(|(j, c)| {
            let longest = rows
                .iter()
                .take(200)
                .filter_map(|r| r.get(j))
                .map(|x| text_of(x).chars().count())
                .chain([text_of(c).chars().count()])
                .max()
                .unwrap_or(0);
            (longest as f32 * 7.0 + 18.0).clamp(MIN_COL_W, MAX_COL_W)
        })
        .collect()
}

/// A list: a header strip, then the rows that are in view.
fn list_body(app: &mut CraftApp, ui: &mut Ui, v: &Rc<Value>, act: &mut Option<Act>) {
    let t = Tokens::get(ui.ctx());
    let empty = Vec::new();
    let columns = v.get("columns").and_then(Value::as_array).unwrap_or(&empty);
    let rows = v.get("rows").and_then(Value::as_array).unwrap_or(&empty);
    let ids = v.get("ids").and_then(Value::as_array).unwrap_or(&empty);
    if rows.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("No rows").color(t.text_dim));
        return;
    }
    let w = widths(columns, rows);
    let selected = selected_ids(app);
    let mut edit = app.ui.dock.state.tables.edit.take();
    egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            for (c, w) in columns.iter().zip(&w) {
                ui.add_sized([*w, ROW_H], egui::Label::new(egui::RichText::new(text_of(c)).small().color(t.text_dim)).truncate());
            }
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, ROW_H, rows.len(), |ui, range| {
            for i in range {
                let (Some(row), Some(id)) = (rows.get(i).and_then(Value::as_array), ids.get(i).and_then(Value::as_str)) else { continue };
                let on = selected.contains(id);
                ui.horizontal(|ui| {
                    for (j, cell) in row.iter().enumerate() {
                        let key = columns.get(j).map_or("", text_of);
                        let width = w.get(j).copied().unwrap_or(MIN_COL_W);
                        cell_ui(ui, &t, &mut edit, act, id, key, text_of(cell), width, on);
                    }
                });
            }
        });
    });
    app.ui.dock.state.tables.edit = edit;
}

/// One list cell: text that selects its row on a click and edits on a double click, or the text
/// field while it is being edited.
#[allow(clippy::too_many_arguments)]
fn cell_ui(ui: &mut Ui, t: &Tokens, edit: &mut Option<Edit>, act: &mut Option<Act>, id: &str, key: &str, text: &str, width: f32, on: bool) {
    let editing = edit.as_ref().is_some_and(|e| e.id == id && e.key == key);
    if editing {
        let Some(e) = edit.as_mut() else { return };
        let resp = ui.add_sized([width, ROW_H], egui::TextEdit::singleline(&mut e.text));
        if e.fresh {
            resp.request_focus();
            e.fresh = false;
        }
        if resp.lost_focus() {
            let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if let Some(done) = edit.take()
                && !cancelled
                && done.text != text
            {
                *act = Some(Act::Set(done));
            }
        }
        return;
    }
    let mut rich = egui::RichText::new(text);
    if on {
        rich = rich.strong().color(t.accent);
    }
    let resp = ui.add_sized([width, ROW_H], egui::Label::new(rich).truncate().sense(Sense::click()));
    if resp.double_clicked() && editable(key) {
        *edit = Some(Edit { id: id.to_string(), key: key.to_string(), text: text.to_string(), fresh: true });
    } else if resp.clicked() {
        *act = Some(Act::Select(id.to_string()));
    }
}

/// A matrix: column headers, then one row of cells per row element.
fn matrix_body(app: &mut CraftApp, ui: &mut Ui, v: &Rc<Value>, act: &mut Option<Act>) {
    let t = Tokens::get(ui.ctx());
    let empty = Vec::new();
    let rows = v.get("rows").and_then(Value::as_array).unwrap_or(&empty);
    let cols = v.get("cols").and_then(Value::as_array).unwrap_or(&empty);
    if rows.is_empty() || cols.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("No elements of these types").color(t.text_dim));
        return;
    }
    let mut marked: std::collections::HashMap<(usize, usize), String> = std::collections::HashMap::new();
    for c in v.get("cells").and_then(Value::as_array).unwrap_or(&empty) {
        let at = |k: &str| c.get(k).and_then(Value::as_u64).map(|n| n as usize);
        if let (Some(r), Some(k)) = (at("row"), at("col")) {
            marked.insert((r, k), text_of(c.get("text").unwrap_or(&Value::Null)).to_string());
        }
    }
    let selected = selected_ids(app);
    egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(MATRIX_HEAD_W + ui.spacing().item_spacing.x);
            for c in cols {
                let label = text_of(c.get("label").unwrap_or(&Value::Null));
                ui.add_sized([MATRIX_COL_W, ROW_H], egui::Label::new(egui::RichText::new(label).small().color(t.text_dim)).truncate())
                    .on_hover_text(label);
            }
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, ROW_H, rows.len(), |ui, range| {
            for i in range {
                let Some(row) = rows.get(i) else { continue };
                let (row_id, label) = (text_of(row.get("id").unwrap_or(&Value::Null)), text_of(row.get("label").unwrap_or(&Value::Null)));
                ui.horizontal(|ui| {
                    let mut name = egui::RichText::new(label);
                    if selected.contains(row_id) {
                        name = name.strong().color(t.accent);
                    }
                    if ui.add_sized([MATRIX_HEAD_W, ROW_H], egui::Label::new(name).truncate().sense(Sense::click())).clicked() {
                        *act = Some(Act::Select(row_id.to_string()));
                    }
                    for (j, c) in cols.iter().enumerate() {
                        let col_id = text_of(c.get("id").unwrap_or(&Value::Null));
                        let cell = marked.get(&(i, j));
                        let text = cell.map_or("", String::as_str);
                        let button = egui::Button::new(if text.is_empty() && cell.is_some() { "." } else { text }).selected(cell.is_some());
                        let tip = format!("{label} to {}", text_of(c.get("label").unwrap_or(&Value::Null)));
                        let resp = ui.add_sized([MATRIX_COL_W, ROW_H], button);
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &tip));
                        if resp.on_hover_text(&tip).clicked() {
                            *act = Some(Act::Toggle(row_id.to_string(), col_id.to_string()));
                        }
                    }
                });
            }
        });
    });
}

/// What `ui.inspect` reports under `tables`: `{view, type, rows, cols, count}`, where `count` is
/// the rows of a list or the marked cells of a matrix in the last read.
pub fn inspect(app: &CraftApp) -> Value {
    let st = &app.ui.dock.state.tables;
    let count = st.cache.as_ref().map_or(0, count_of);
    json!({"view": if st.matrix { "matrix" } else { "list" }, "type": st.type_id, "rows": st.rows, "cols": st.cols, "count": count})
}

/// `ui.tables {view?, type?, rows?, cols?, relationship?, property?, filter?}`: set the view,
/// open the dock on the Tables tab and answer as `ui.inspect` reports it. A view the commands
/// refuse (an unknown type) is an error and changes nothing.
pub fn command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    let get = |k: &str| match p.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("`{k}` must be a string")),
    };
    let mut next = app.ui.dock.state.tables.clone();
    match get("view")?.as_deref() {
        None => {}
        Some("list") => next.matrix = false,
        Some("matrix") => next.matrix = true,
        Some(o) => return Err(format!("`view` must be list or matrix, not {o:?}")),
    }
    if let Some(v) = get("type")? {
        next.type_id = v;
    }
    if let Some(v) = get("rows")? {
        next.rows = v;
        next.matrix = p.get("view").is_none() || next.matrix;
    }
    if let Some(v) = get("cols")? {
        next.cols = v;
    }
    if let Some(v) = get("relationship")? {
        next.relationship = v;
    }
    if let Some(v) = get("property")? {
        next.property = v;
    }
    if let Some(v) = get("filter")? {
        next.filter = v;
    }
    next.edit = None;
    let (command, params) = (if next.matrix { "table.matrix" } else { "table.list" }, next.params());
    // The commands are off for an empty model, so there is nothing to check a view against.
    if !(app.session.doc.model.elements.is_empty() && app.session.doc.model.relationships.is_empty()) {
        app.session.run(command, &params).map_err(|e| e.to_string())?;
    }
    app.ui.dock.state.tables = next;
    super::open_on(app, DockTab::Tables);
    refresh(app);
    Ok(inspect(app))
}
