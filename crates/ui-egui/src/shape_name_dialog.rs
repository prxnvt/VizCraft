//! The Shape Name dialog: a name for the selected shape. It opens when `shape.rename` runs
//! without `name` (Developer > Shape Design > Shape Name) and ends by running it with the text.

use serde::Serialize;
use serde_json::json;

use crate::CraftApp;

/// The shape being named and the text so far.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct State {
    /// The shape's id.
    pub id: String,
    /// The name being typed.
    pub name: String,
}

/// The dialog for the primary selected shape, starting from its current name.
pub fn open(app: &CraftApp) -> State {
    let page = app.session.doc.page();
    let shape = app.session.sel.primary.as_ref().and_then(|id| page.and_then(|p| p.shape(id)));
    shape.map(|s| State { id: s.id.clone(), name: s.name.clone() }).unwrap_or_default()
}

/// The name field above the OK and Cancel buttons.
pub fn body(ui: &mut egui::Ui, s: &mut State) {
    ui.label(format!("Name of shape {}", s.id));
    let r = ui.add(egui::TextEdit::singleline(&mut s.name).hint_text("Shape name").desired_width(f32::INFINITY));
    if !r.has_focus() && s.name.is_empty() {
        r.request_focus();
    }
}

/// After the buttons: on OK run `shape.rename` with the text. Returns whether to close.
pub fn finish(app: &mut CraftApp, s: &State, ok: bool, cancel: bool) -> bool {
    if ok && !s.id.is_empty() {
        let _ = app.run("shape.rename", json!({"id": s.id, "name": s.name}));
    }
    ok || cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_name_without_a_name_opens_the_dialog_and_ok_renames() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default());
        let first = app.session.doc.page().unwrap().shapes[0].id.clone();
        app.run("edit.select", json!({"ids": [first.clone()]})).unwrap();
        app.run("shape.rename", json!({})).unwrap();
        assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some("shapeName"));
        let mut s = open(&app);
        assert_eq!(s.id, first);
        s.name = "Start".into();
        assert!(finish(&mut app, &s, true, false));
        assert_eq!(app.session.doc.page().unwrap().shape(&first).unwrap().name, "Start");
    }
}
