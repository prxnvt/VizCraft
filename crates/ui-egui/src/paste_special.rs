//! The Paste Special dialog: paste the clipboard's shapes, only its text into the selection, or
//! only its formatting onto the selection. It opens when `edit.pasteSpecial` runs without `as`
//! (Home > Clipboard > Paste > Paste Special) and ends by running it with the choice.

use serde::Serialize;
use serde_json::json;

use crate::CraftApp;

/// The choice, as `edit.pasteSpecial`'s `as`.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct State {
    /// `shapes`, `text` or `style`.
    pub how: &'static str,
}

/// The dialog, starting on whatever fits the selection: text into a selection, else shapes.
pub fn open(app: &CraftApp) -> State {
    State { how: if app.session.sel.items.is_empty() { "shapes" } else { "text" } }
}

/// The choices above the OK and Cancel buttons.
pub fn body(ui: &mut egui::Ui, app: &CraftApp, s: &mut State) {
    let has_sel = !app.session.sel.items.is_empty();
    ui.radio_value(&mut s.how, "shapes", "Shapes");
    ui.add_enabled_ui(has_sel, |ui| {
        ui.radio_value(&mut s.how, "text", "Text only, into the selection");
        ui.radio_value(&mut s.how, "style", "Formatting only, onto the selection");
    });
    if !has_sel && s.how != "shapes" {
        s.how = "shapes";
    }
}

/// After the buttons: on OK run `edit.pasteSpecial` with the choice. Returns whether to close.
pub fn finish(app: &mut CraftApp, s: &State, ok: bool, cancel: bool) -> bool {
    if ok {
        let _ = app.run("edit.pasteSpecial", json!({"as": s.how}));
    }
    ok || cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_special_without_a_choice_opens_the_dialog_and_ok_pastes() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default());
        let first = app.session.doc.page().unwrap().shapes[0].id.clone();
        app.run("edit.select", json!({"ids": [first]})).unwrap();
        app.run("edit.copy", json!({})).unwrap();
        app.run("edit.pasteSpecial", json!({})).unwrap();
        assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some("pasteSpecial"));
        let n = app.session.doc.page().unwrap().shapes.len();
        let s = State { how: "shapes" };
        assert!(finish(&mut app, &s, true, false));
        assert_eq!(app.session.doc.page().unwrap().shapes.len(), n + 1);
    }
}
