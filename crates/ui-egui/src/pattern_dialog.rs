//! The pattern form: a typed form built from a pattern's parameter schema (a count spinner, text
//! fields, a label list, a choice menu, a small table editor). As a dialog (`ui.patterns
//! {pattern}`, or a click in the Patterns pane) its Insert button runs `pattern.insert`; the
//! Patterns pane draws the same form for the selected instance with an Apply button that runs
//! `pattern.reparameterize`. The form only holds values; the engine checks them.

use std::collections::BTreeMap;

use egui::Ui;
use serde::Serialize;
use serde_json::{Value, json};
use vizcraft_engine::cmd::pattern::patterns::{self as vizcraft_patterns, Param, ParamKind, Pattern};

use crate::CraftApp;
use crate::theme::Tokens;

/// Most label or table rows the form lets a person add (the schema's caps are the engine's).
const MAX_ROWS: usize = vizcraft_patterns::schema::MAX_LABELS;

/// The values being edited for one pattern.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct State {
    /// The pattern id.
    pub pattern: String,
    /// Parameter key to its value so far, as the engine reads it.
    pub values: BTreeMap<String, Value>,
    /// The instance being changed, empty for a new insertion.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub instance: String,
    /// Why the last Insert or Apply was refused.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

impl State {
    /// A form for `pattern` at its defaults; `None` for an unknown pattern.
    pub fn new(pattern: &str) -> Option<State> {
        let p = vizcraft_patterns::find(pattern)?;
        Some(State { pattern: p.id.clone(), values: p.defaults().0, instance: String::new(), error: String::new() })
    }

    /// A form for instance `instance` of `pattern` with its current `args` (defaults fill the
    /// keys it lacks).
    pub fn for_instance(pattern: &str, instance: &str, args: &Value) -> Option<State> {
        let mut s = State::new(pattern)?;
        if let Some(o) = args.as_object() {
            for (k, v) in o {
                if let Some(slot) = s.values.get_mut(k) {
                    *slot = v.clone();
                }
            }
        }
        s.instance = instance.to_string();
        Some(s)
    }

    /// The values as the `args` param.
    pub fn args(&self) -> Value {
        Value::Object(self.values.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
}

/// The pattern's name and description, then one row per parameter.
pub fn body(ui: &mut Ui, s: &mut State) {
    let t = Tokens::get(ui.ctx());
    let Some(p) = vizcraft_patterns::find(&s.pattern) else {
        ui.label(egui::RichText::new(format!("No pattern {}", s.pattern)).color(t.text_dim));
        return;
    };
    ui.label(egui::RichText::new(&p.description).color(t.text_dim));
    ui.add_space(6.0);
    fields(ui, p, s);
    if !s.error.is_empty() {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(&s.error).color(t.red));
    }
}

/// The form rows for `p`.
pub fn fields(ui: &mut Ui, p: &Pattern, s: &mut State) {
    egui::Grid::new(("pattern_form", s.pattern.as_str(), s.instance.as_str())).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        for param in &p.params {
            ui.label(&param.label);
            let v = s.values.entry(param.key.clone()).or_insert_with(|| param.default.clone());
            field(ui, param, v);
            ui.end_row();
        }
    });
}

fn text_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn texts_of(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().map(text_of).collect()).unwrap_or_default()
}

/// One parameter's widget, editing `v` in place.
fn field(ui: &mut Ui, param: &Param, v: &mut Value) {
    let id = param.key.as_str();
    match &param.kind {
        ParamKind::Count { min, max } => {
            let mut n = v.as_i64().unwrap_or(i64::from(*min)).clamp(i64::from(*min), i64::from(*max));
            ui.add(egui::DragValue::new(&mut n).range(i64::from(*min)..=i64::from(*max)).speed(0.1));
            *v = json!(n);
        }
        ParamKind::Text | ParamKind::TagPrefix => {
            let mut t = text_of(v);
            let hint = if matches!(param.kind, ParamKind::TagPrefix) { "none" } else { "" };
            if ui.add(egui::TextEdit::singleline(&mut t).hint_text(hint).desired_width(180.0).id_salt(id)).changed() {
                *v = json!(t);
            }
        }
        ParamKind::Choice { values } => {
            let mut cur = text_of(v);
            egui::ComboBox::from_id_salt(id).selected_text(cur.clone()).show_ui(ui, |ui| {
                for x in values {
                    ui.selectable_value(&mut cur, x.clone(), x);
                }
            });
            *v = json!(cur);
        }
        ParamKind::Labels => {
            let mut rows = texts_of(v);
            let mut remove = None;
            ui.vertical(|ui| {
                for (i, r) in rows.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(r).desired_width(150.0).id_salt((id, i)));
                        if ui.small_button("Remove").on_hover_text("Remove this label").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if rows.len() < MAX_ROWS && ui.small_button("Add label").clicked() {
                    rows.push(String::new());
                }
            });
            if let Some(i) = remove.filter(|i| *i < rows.len()) {
                rows.remove(i);
            }
            *v = json!(rows);
        }
        ParamKind::Table { columns } => {
            let mut rows: Vec<Vec<String>> = v.as_array().map(|a| a.iter().map(texts_of).collect()).unwrap_or_default();
            let mut remove = None;
            ui.vertical(|ui| {
                egui::Grid::new((id, "table")).num_columns(columns.len().saturating_add(1)).spacing([4.0, 2.0]).show(ui, |ui| {
                    for c in columns {
                        ui.label(egui::RichText::new(c).small().strong());
                    }
                    ui.end_row();
                    for (i, row) in rows.iter_mut().enumerate() {
                        row.resize(columns.len(), String::new());
                        for (j, cell) in row.iter_mut().enumerate() {
                            ui.add(egui::TextEdit::singleline(cell).desired_width(80.0).id_salt((id, i, j)));
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if rows.len() < vizcraft_patterns::schema::MAX_ROWS && ui.small_button("Add row").clicked() {
                    rows.push(vec![String::new(); columns.len()]);
                }
            });
            if let Some(i) = remove.filter(|i| *i < rows.len()) {
                rows.remove(i);
            }
            *v = json!(rows);
        }
    }
}

/// After the buttons: Insert runs `pattern.insert` with the values (centred on the view), or
/// for an instance `pattern.reparameterize`. A refusal stays open with the reason. Returns
/// whether to close.
pub fn finish(app: &mut CraftApp, s: &mut State, ok: bool, cancel: bool) -> bool {
    if cancel {
        return true;
    }
    if !ok {
        return false;
    }
    match submit(app, s) {
        Ok(_) => true,
        Err(e) => {
            s.error = e;
            false
        }
    }
}

/// Run the form's command: `pattern.insert`, or `pattern.reparameterize` for an instance.
pub fn submit(app: &mut CraftApp, s: &State) -> Result<Value, String> {
    if s.instance.is_empty() {
        app.run("pattern.insert", json!({"pattern": s.pattern, "args": s.args()}))
    } else {
        app.run("pattern.reparameterize", json!({"instance": s.instance, "args": s.args()}))
    }
}

/// The dialog's title: the pattern's name.
pub fn title(s: &State) -> String {
    vizcraft_patterns::find(&s.pattern).map(|p| format!("Insert {}", p.name)).unwrap_or_else(|| "Insert Pattern".into())
}
