//! The Edit Data Graphic dialog and `ui.linkData`.
//!
//! The dialog edits one data graphic: its name and its items, each with the fields its kind uses
//! (a callout's label and number format, a bar's range and colour, the rules of an icon set or a
//! colour by value). It opens from Data > Display Data (`ui.dialog {"name": "dataGraphic"}`) on
//! the selection's graphic, else the document's first, else a new one, and ends by running
//! `datagraphic.define` (and `datagraphic.apply` on the selection), so an agent gets the same
//! result without it.
//!
//! `ui.linkData` asks for a table file and runs `datalink.link` with it, keyed on the first
//! column whose name is a data key some shape already has (else the first column), then opens
//! the External Data pane.

use egui::Ui;
use serde::Serialize;
use serde_json::{Value, json};
use vizcraft_diagram::data_graphic::{DataGraphic, DataItem, ICONS, ItemKind, Op, Position, Rule, Source, TableFormat};

use crate::CraftApp;
use crate::theme::Tokens;

/// One rule's fields, as text.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuleState {
    /// Comparison.
    pub op: Op,
    /// Compared with.
    pub value: String,
    /// Upper end of a range.
    pub to: String,
    /// Icon of an icon set.
    pub icon: String,
    /// Fill of a colour by value, `#rrggbb`.
    pub fill: String,
    /// Line colour of a colour by value.
    pub line: String,
    /// Legend text.
    pub label: String,
}

/// One item's fields, as text.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemState {
    /// What it shows.
    pub kind: ItemKind,
    /// Where the value comes from.
    pub from: Source,
    /// Data key or property.
    pub key: String,
    /// Where it sits.
    pub position: Position,
    /// Callout label.
    pub label: String,
    /// Callout number format.
    pub format: String,
    /// Bar minimum.
    pub min: String,
    /// Bar maximum.
    pub max: String,
    /// Bar colour.
    pub color: String,
    /// Icon set and colour rules.
    pub rules: Vec<RuleState>,
}

/// The dialog's fields.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// The graphic edited; `None` makes a new one.
    pub id: Option<String>,
    /// Its name.
    pub name: String,
    /// Its items.
    pub items: Vec<ItemState>,
    /// Show it on the selected shapes after OK.
    pub apply_to_selection: bool,
    /// Why the last OK was refused.
    pub error: Option<String>,
}

fn hex(c: Option<vizcraft_diagram::Color>) -> String {
    c.map(|c| c.to_hex()).unwrap_or_default()
}

impl ItemState {
    fn from_item(i: &DataItem) -> Self {
        ItemState {
            kind: i.kind,
            from: i.from,
            key: i.key.clone(),
            position: i.position,
            label: i.label.clone(),
            format: i.format.clone(),
            min: vizcraft_diagram::data::fmt_number(i.min),
            max: vizcraft_diagram::data::fmt_number(i.max),
            color: i.color.to_hex(),
            rules: i.rules.iter().map(RuleState::from_rule).collect(),
        }
    }
    fn new(key: &str) -> Self {
        ItemState::from_item(&DataItem { key: key.to_string(), ..DataItem::default() })
    }
}

impl RuleState {
    fn from_rule(r: &Rule) -> Self {
        RuleState {
            op: r.op,
            value: r.value.clone(),
            to: r.to.clone(),
            icon: r.icon.clone(),
            fill: hex(r.fill),
            line: hex(r.line),
            label: r.label.clone(),
        }
    }
}

/// The state for graphic `g`.
fn from_graphic(g: &DataGraphic, apply: bool) -> State {
    State {
        id: Some(g.id.clone()),
        name: g.name.clone(),
        items: g.items.iter().map(ItemState::from_item).collect(),
        apply_to_selection: apply,
        error: None,
    }
}

/// The dialog on the selection's graphic, else the document's first, else a new graphic with a
/// callout of the selection's first data key.
pub fn open(app: &CraftApp) -> State {
    let doc = &app.session.doc;
    let apply = !app.session.sel.items.is_empty();
    let current = vizcraft_engine::cmd::datagraphic::current(&app.session);
    let g = current.and_then(|id| doc.data_graphics.iter().find(|g| g.id == id)).or(doc.data_graphics.first());
    match g {
        Some(g) => from_graphic(g, apply),
        None => {
            let key = app
                .session
                .sel
                .primary
                .as_deref()
                .and_then(|id| doc.page()?.shape(id))
                .and_then(|s| s.data.rows.first())
                .map(|r| r.key.clone())
                .unwrap_or_default();
            State { id: None, name: "Data Graphic".into(), items: vec![ItemState::new(&key)], apply_to_selection: apply, error: None }
        }
    }
}

fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut T,
    all: &[T],
    name: fn(T) -> &'static str,
    width: f32,
) {
    egui::ComboBox::from_id_salt(salt).selected_text(name(*value)).width(width).show_ui(ui, |ui| {
        for v in all {
            ui.selectable_value(value, *v, name(*v));
        }
    });
}

fn field(ui: &mut Ui, text: &mut String, hint: &str, width: f32) {
    ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(width));
}

fn rules_ui(ui: &mut Ui, n: usize, item: &mut ItemState) {
    let mut drop = None;
    for (ri, r) in item.rules.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            combo(ui, ("dg-op", n, ri), &mut r.op, &Op::ALL, Op::name, 70.0);
            if !matches!(r.op, Op::Empty | Op::NotEmpty | Op::Any) {
                field(ui, &mut r.value, "value", 52.0);
            }
            if r.op == Op::Between {
                field(ui, &mut r.to, "to", 52.0);
            }
            if item.kind == ItemKind::Icons {
                egui::ComboBox::from_id_salt(("dg-icon", n, ri)).selected_text(r.icon.as_str()).width(86.0).show_ui(ui, |ui| {
                    for i in ICONS {
                        ui.selectable_value(&mut r.icon, (*i).to_string(), *i);
                    }
                });
            } else {
                field(ui, &mut r.fill, "#fill", 60.0);
                field(ui, &mut r.line, "#line", 60.0);
            }
            if ui.small_button("Remove").clicked() {
                drop = Some(ri);
            }
        });
    }
    if let Some(ri) = drop {
        item.rules.remove(ri);
    }
    if ui.small_button("Add Rule").clicked() {
        let icon = if item.kind == ItemKind::Icons { "check".to_string() } else { String::new() };
        let fill = if item.kind == ItemKind::Color { "#dcfce7".to_string() } else { String::new() };
        item.rules.push(RuleState { op: Op::Any, icon, fill, ..RuleState::default() });
    }
}

/// The fields above the OK and Cancel buttons.
pub fn body(ui: &mut Ui, s: &mut State) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label("Name");
        field(ui, &mut s.name, "Status", 260.0);
    });
    ui.add_space(4.0);
    let mut drop = None;
    egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
        for (n, item) in s.items.iter_mut().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    combo(ui, ("dg-kind", n), &mut item.kind, &ItemKind::ALL, ItemKind::name, 70.0);
                    combo(ui, ("dg-from", n), &mut item.from, &[Source::Data, Source::Property], Source::name, 72.0);
                    field(ui, &mut item.key, "key", 90.0);
                    if item.kind != ItemKind::Color {
                        combo(ui, ("dg-pos", n), &mut item.position, &Position::ALL, Position::name, 90.0);
                    }
                    if ui.small_button("Remove").clicked() {
                        drop = Some(n);
                    }
                });
                match item.kind {
                    ItemKind::Text => {
                        ui.horizontal(|ui| {
                            field(ui, &mut item.label, "label", 120.0);
                            field(ui, &mut item.format, "#,##0.00", 90.0);
                        });
                    }
                    ItemKind::Bar => {
                        ui.horizontal(|ui| {
                            field(ui, &mut item.min, "min", 60.0);
                            field(ui, &mut item.max, "max", 60.0);
                            field(ui, &mut item.color, "#colour", 80.0);
                        });
                    }
                    ItemKind::Icons | ItemKind::Color => rules_ui(ui, n, item),
                }
            });
        }
    });
    if let Some(n) = drop {
        s.items.remove(n);
    }
    if ui.button("Add Item").clicked() {
        let key = s.items.last().map(|i| i.key.clone()).unwrap_or_default();
        s.items.push(ItemState::new(&key));
    }
    ui.checkbox(&mut s.apply_to_selection, "Show on the selected shapes");
    if let Some(e) = &s.error {
        ui.label(egui::RichText::new(e).color(t.red));
    }
}

/// A number field as JSON: a number when it reads as one, else the text (which the command
/// refuses with a message).
fn number(text: &str) -> Value {
    let t = text.trim();
    match t.parse::<f64>() {
        Ok(n) if n.is_finite() => json!(n),
        _ if t.is_empty() => Value::Null,
        _ => json!(t),
    }
}

fn opt_text(text: &str) -> Value {
    let t = text.trim();
    if t.is_empty() { Value::Null } else { json!(t) }
}

/// The `datagraphic.define` params the fields make.
pub fn params(s: &State) -> Value {
    let items: Vec<Value> = s
        .items
        .iter()
        .map(|i| {
            let rules: Vec<Value> = i
                .rules
                .iter()
                .map(|r| {
                    json!({"op": r.op.name(), "value": opt_text(&r.value), "to": opt_text(&r.to), "label": opt_text(&r.label),
                        "icon": if i.kind == ItemKind::Icons { opt_text(&r.icon) } else { Value::Null },
                        "fill": if i.kind == ItemKind::Color { opt_text(&r.fill) } else { Value::Null },
                        "line": if i.kind == ItemKind::Color { opt_text(&r.line) } else { Value::Null }})
                })
                .collect();
            json!({"kind": i.kind.name(), "from": i.from.name(), "key": i.key.trim(), "position": i.position.name(), "label": opt_text(&i.label),
                "format": opt_text(&i.format), "min": number(&i.min), "max": number(&i.max), "color": opt_text(&i.color),
                "rules": if matches!(i.kind, ItemKind::Icons | ItemKind::Color) { json!(rules) } else { Value::Null }})
        })
        .collect();
    let mut p = json!({"name": s.name.trim(), "items": items});
    if let (Some(id), Some(o)) = (&s.id, p.as_object_mut()) {
        o.insert("id".into(), json!(id));
    }
    p
}

/// After the buttons: on OK run `datagraphic.define`, then `datagraphic.apply` on the selection
/// when asked; a refusal keeps the dialog open with the reason. Returns whether to close.
pub fn finish(app: &mut CraftApp, s: &mut State, ok: bool, cancel: bool) -> bool {
    if cancel {
        return true;
    }
    if !ok {
        return false;
    }
    let id = match app.run("datagraphic.define", params(s)) {
        Ok(r) => r.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
        Err(e) => {
            s.error = Some(e);
            return false;
        }
    };
    s.id = Some(id.clone());
    if s.apply_to_selection
        && !app.session.sel.items.is_empty()
        && let Err(e) = app.run("datagraphic.apply", json!({"graphic": id}))
    {
        s.error = Some(e);
        return false;
    }
    true
}

/// `ui.linkData {path?, keyColumn?, sheet?}`: link a table to shape data. Without a path it asks
/// for a file; without a key column it picks one (see the module docs).
pub fn ui_command(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if id != "ui.linkData" {
        return None;
    }
    let path = match p.get("path").and_then(Value::as_str) {
        Some(path) => path.to_string(),
        None => {
            if app.services.open_async.is_some() {
                return Some(Err("linking a table needs a file on disk; the web build cannot do that yet".into()));
            }
            match app.services.pick_open.as_ref().and_then(|f| f("data")) {
                Some(path) => path,
                None => return Some(Ok(json!({"linked": false}))),
            }
        }
    };
    let sheet = p.get("sheet").and_then(Value::as_str).unwrap_or_default().to_string();
    let key = match p.get("keyColumn").and_then(Value::as_str) {
        Some(k) => k.to_string(),
        None => {
            let table = match vizcraft_engine::cmd::datalink::read_table(&path, TableFormat::for_path(&path), &sheet) {
                Ok(t) => t,
                Err(e) => {
                    app.status(e.to_string());
                    return Some(Err(e.to_string()));
                }
            };
            let held = |name: &str| {
                vizcraft_engine::cmd::datalink::column_key(name)
                    .is_some_and(|k| app.session.doc.pages.iter().flat_map(|pg| &pg.shapes).any(|s| s.data.row(&k).is_some()))
            };
            match table.columns.iter().find(|c| held(&c.name)).or(table.columns.first()) {
                Some(c) => c.name.clone(),
                None => return Some(Err("the table has no columns".into())),
            }
        }
    };
    let r = app.run("datalink.link", json!({"path": path, "keyColumn": key, "sheet": if sheet.is_empty() { Value::Null } else { json!(sheet) }}));
    if r.is_ok() {
        let _ = app.run("ui.pane", json!({"name": "externalData", "value": true}));
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default())
    }

    #[test]
    fn ok_defines_and_applies_and_errors_keep_the_dialog_open() {
        let mut a = app();
        let id = a.session.doc.page().unwrap().shapes[1].id.clone();
        a.run("data.set", json!({"ids": [id], "key": "cost", "value": 12})).unwrap();
        a.run("edit.select", json!({"ids": [id]})).unwrap();
        a.run("ui.dialog", json!({"name": "dataGraphic"})).unwrap();
        assert_eq!(a.dialog.as_ref().map(|d| d.name()), Some("dataGraphic"));
        let mut s = open(&a);
        assert_eq!(s.items[0].key, "cost", "a new graphic starts from the selection's data");
        s.items.push(ItemState { kind: ItemKind::Icons, key: "cost".into(), ..ItemState::default() });
        assert!(!finish(&mut a, &mut s, true, false), "an icon set without rules is refused");
        assert!(s.error.is_some());
        s.items[1].rules.push(RuleState { op: Op::Gt, value: "10".into(), icon: "check".into(), ..RuleState::default() });
        assert!(finish(&mut a, &mut s, true, false), "{:?}", s.error);
        assert_eq!(a.session.doc.data_graphics.len(), 1);
        assert_eq!(a.session.doc.shape(&id).unwrap().data_graphic.as_deref(), Some("dg1"));
        let again = open(&a);
        assert_eq!(again.id.as_deref(), Some("dg1"), "reopening edits the selection's graphic");
        assert!(finish(&mut a, &mut s, false, true), "cancel closes");
    }

    #[test]
    fn link_data_with_a_path_links_and_opens_the_pane() {
        let dir = std::env::temp_dir().join(format!("vizcraft-ui-linkdata-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.csv");
        std::fs::write(&path, "Name,cost\nx,1\n").unwrap();
        let mut a = app();
        let id = a.session.doc.page().unwrap().shapes[1].id.clone();
        a.run("data.set", json!({"ids": [id], "key": "cost", "value": 12})).unwrap();
        let r = a.run("ui.linkData", json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!(a.session.doc.data_links[0].key_column, "cost", "the column shapes already hold is the key: {r}");
        assert!(a.ui.panes.is_open("externalData"));
        assert!(a.run("ui.linkData", json!({"path": "/nonexistent/x.csv"})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
