//! The Define Shape Data dialog: a data key for the active page (key, label, type, unit, enum
//! values, required, default). It opens from `data.defineDialog` (the Data tab's Show/Hide group
//! and the Shape Data pane) and ends by running `data.defineKey`, so an agent gets the same
//! result without it.

use egui::Ui;
use serde::Serialize;
use serde_json::{Value, json};
use vizcraft_diagram::data::DataType;

use crate::CraftApp;
use crate::theme::Tokens;

/// The dialog's fields.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// The key.
    pub key: String,
    /// Label shown instead of the key.
    pub label: String,
    /// Value type.
    #[serde(skip)]
    pub kind: DataType,
    /// Unit beside numbers.
    pub unit: String,
    /// Enum values, comma separated.
    pub values: String,
    /// The value must be set.
    pub required: bool,
    /// Starting value.
    pub default: String,
    /// Opt the selected shapes in to the page's keys.
    pub apply_to_selection: bool,
    /// Why the last OK was refused.
    pub error: Option<String>,
}

/// The dialog, empty, applying to the selection when there is one.
pub fn open(app: &CraftApp) -> State {
    State {
        key: String::new(),
        label: String::new(),
        kind: DataType::String,
        unit: String::new(),
        values: String::new(),
        required: false,
        default: String::new(),
        apply_to_selection: !app.session.sel.items.is_empty(),
        error: None,
    }
}

/// The fields above the OK and Cancel buttons.
pub fn body(ui: &mut Ui, s: &mut State) {
    let t = Tokens::get(ui.ctx());
    egui::Grid::new("define-shape-data").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Key");
        ui.add(egui::TextEdit::singleline(&mut s.key).hint_text("cost").desired_width(220.0));
        ui.end_row();
        ui.label("Label");
        ui.add(egui::TextEdit::singleline(&mut s.label).hint_text("Cost").desired_width(220.0));
        ui.end_row();
        ui.label("Type");
        egui::ComboBox::from_id_salt("define-shape-data-type").selected_text(s.kind.name()).width(120.0).show_ui(ui, |ui| {
            for k in DataType::ALL {
                ui.selectable_value(&mut s.kind, k, k.name());
            }
        });
        ui.end_row();
        if s.kind == DataType::Number {
            ui.label("Unit");
            ui.add(egui::TextEdit::singleline(&mut s.unit).hint_text("kg").desired_width(220.0));
            ui.end_row();
        }
        if s.kind == DataType::Enum {
            ui.label("Values");
            ui.add(egui::TextEdit::singleline(&mut s.values).hint_text("Low, Medium, High").desired_width(220.0));
            ui.end_row();
        }
        ui.label("Default");
        ui.add(egui::TextEdit::singleline(&mut s.default).desired_width(220.0));
        ui.end_row();
    });
    ui.checkbox(&mut s.required, "Required");
    ui.checkbox(&mut s.apply_to_selection, "Add to the selected shapes");
    if let Some(e) = &s.error {
        ui.label(egui::RichText::new(e).color(t.red));
    }
}

/// The `data.defineKey` params the fields make.
pub fn params(app: &CraftApp, s: &State) -> Value {
    let mut p = json!({"key": s.key.trim(), "type": s.kind.name(), "required": s.required});
    let set = |p: &mut Value, k: &str, v: Value| {
        if let Some(o) = p.as_object_mut() {
            o.insert(k.into(), v);
        }
    };
    if !s.label.trim().is_empty() {
        set(&mut p, "label", json!(s.label.trim()));
    }
    if s.kind == DataType::Number && !s.unit.trim().is_empty() {
        set(&mut p, "unit", json!(s.unit.trim()));
    }
    if s.kind == DataType::Enum {
        let values: Vec<&str> = s.values.split(',').map(str::trim).filter(|v| !v.is_empty()).collect();
        set(&mut p, "values", json!(values));
    }
    if !s.default.trim().is_empty() {
        set(&mut p, "default", json!(s.default.trim()));
    }
    if s.apply_to_selection {
        let page = app.session.doc.page();
        let ids: Vec<&String> = app.session.sel.items.iter().filter(|id| page.is_some_and(|p| p.shape(id).is_some())).collect();
        if !ids.is_empty() {
            set(&mut p, "ids", json!(ids));
        }
    }
    p
}

/// After the buttons: on OK run `data.defineKey`; a refusal keeps the dialog open with the
/// reason. Returns whether to close.
pub fn finish(app: &mut CraftApp, s: &mut State, ok: bool, cancel: bool) -> bool {
    if cancel {
        return true;
    }
    if !ok {
        return false;
    }
    let p = params(app, s);
    match app.run("data.defineKey", p) {
        Ok(_) => true,
        Err(e) => {
            s.error = Some(e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default())
    }

    #[test]
    fn ok_defines_the_key_and_errors_keep_the_dialog_open() {
        let mut a = app();
        a.run("data.defineDialog", json!({})).unwrap();
        assert_eq!(a.dialog.as_ref().map(|d| d.name()), Some("defineShapeData"));
        let mut s = open(&a);
        s.key = "level".into();
        s.kind = DataType::Enum;
        assert!(!finish(&mut a, &mut s, true, false), "an enum without values is refused");
        assert!(s.error.is_some());
        s.values = "Low, High".into();
        s.default = "low".into();
        assert!(finish(&mut a, &mut s, true, false));
        let keys = vizcraft_diagram::data::page_keys(a.session.doc.page().unwrap());
        assert_eq!(keys[0].values, ["Low", "High"]);
        assert_eq!(keys[0].default.as_deref(), Some("Low"));
        assert!(finish(&mut a, &mut s, false, true), "cancel closes");
    }
}
