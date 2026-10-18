//! Insert > Text: the Text Box tool, the Symbol dialog and fields.
//!
//! The Field menu lists every field, then the data keys of the selected shape (its element's
//! properties and its own data values). While the selected shape's text was just being edited,
//! the field goes in where the caret stopped.

use serde_json::{Value, json};
use vizcraft_diagram::text_fields::{DATA_EXT_PREFIX, FIELDS};

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size, selected_shape};
use crate::{CraftApp, Tool, widgets};

/// The selected shape's data keys: its element's properties, then its own data values.
fn data_keys(app: &CraftApp) -> Vec<String> {
    let Some(s) = selected_shape(app) else { return Vec::new() };
    let mut keys: Vec<String> = s.element.as_deref().and_then(|e| app.session.doc.model.element(e)).map(|e| e.properties.keys().cloned().collect()).unwrap_or_default();
    for k in s.ext.keys().filter_map(|k| k.strip_prefix(DATA_EXT_PREFIX)) {
        if !keys.iter().any(|x| x == k) {
            keys.push(k.to_string());
        }
    }
    keys
}

/// `text.insertField` params, with `at` when the selected item's last edit left a caret.
fn field_params(app: &CraftApp, mut p: Value) -> Value {
    if let (Some((id, at)), [sel]) = (&app.canvas.last_caret, app.session.sel.items.as_slice())
        && id == sel
        && let Some(o) = p.as_object_mut()
    {
        o.insert("at".into(), json!(at));
    }
    p
}

/// The Field menu button.
fn field_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    widgets::menu_button(ui, app, "field", "Field", "Insert Field", true, |ui, app| {
        for (name, label) in FIELDS.iter().filter(|(n, _)| *n != "data") {
            let p = field_params(app, json!({"field": name}));
            widgets::menu_item(ui, app, label, "text.insertField", p, None);
        }
        ui.separator();
        let keys = data_keys(app);
        if keys.is_empty() {
            ui.add_enabled(false, egui::Button::new("No shape data"));
        }
        for k in keys {
            let p = field_params(app, json!({"field": "data", "key": k}));
            widgets::menu_item(ui, app, &format!("Data: {k}"), "text.insertField", p, None);
        }
    });
}

/// The Insert tab's Text group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Text",
        vec![
            Button {
                id: "ui.tool",
                icon: "textBox",
                label: "Text Box",
                size: Size::Big,
                params: |_| json!({"tool": "text"}),
                checked: Some(|a| a.tool == Tool::Text),
            },
            Big { id: "text.insertSymbol", icon: "symbol" },
            Custom(field_menu),
        ],
    )
}
