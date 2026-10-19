//! The Revise dialog: a new revision of the drawing. It opens when `sheet.revise` runs without
//! a description (Design > Sheet > Revise) and ends by running it with the fields typed; the
//! engine picks the next letter of the sheet's revision scheme.

use serde::Serialize;
use serde_json::{Value, json};

use crate::CraftApp;

/// The fields of the next revision.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct State {
    /// What changed (required).
    pub description: String,
    /// Who approved it.
    pub approved: String,
    /// `YYYY-MM-DD`; empty takes today.
    pub date: String,
    /// The zone of the change (`B4`).
    pub zone: String,
    /// The latest revision so far, shown for orientation.
    pub last: String,
}

/// The dialog, with the latest revision letter filled in for reference.
pub fn open(app: &CraftApp) -> State {
    let last = app
        .session
        .doc
        .model
        .elements
        .iter()
        .rev()
        .find(|e| e.type_id == vizcraft_notation::sheet_pass::REVISION)
        .and_then(|e| e.prop("rev"))
        .map(|v| v.display())
        .unwrap_or_default();
    State { last, ..State::default() }
}

/// The fields above the OK and Cancel buttons.
pub fn body(ui: &mut egui::Ui, s: &mut State) {
    let last = if s.last.is_empty() { "No revision yet".to_string() } else { format!("Latest revision: {}", s.last) };
    ui.label(last);
    egui::Grid::new("revise").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Description");
        let r = ui.add(egui::TextEdit::singleline(&mut s.description).hint_text("What changed").desired_width(240.0));
        if !r.has_focus() && s.description.is_empty() {
            r.request_focus();
        }
        ui.end_row();
        ui.label("Approved by");
        ui.add(egui::TextEdit::singleline(&mut s.approved).desired_width(240.0));
        ui.end_row();
        ui.label("Date");
        ui.add(egui::TextEdit::singleline(&mut s.date).hint_text("today").desired_width(240.0));
        ui.end_row();
        ui.label("Zone");
        ui.add(egui::TextEdit::singleline(&mut s.zone).hint_text("e.g. B4").desired_width(240.0));
        ui.end_row();
    });
}

/// The `sheet.revise` params for the fields typed (empty ones left out).
pub fn params(s: &State) -> Value {
    let mut p = json!({"description": s.description.trim()});
    for (k, v) in [("approved", &s.approved), ("date", &s.date), ("zone", &s.zone)] {
        if !v.trim().is_empty() {
            p[k] = json!(v.trim());
        }
    }
    p
}

/// After the buttons: on OK with a description run `sheet.revise`. Returns whether to close.
pub fn finish(app: &mut CraftApp, s: &State, ok: bool, cancel: bool) -> bool {
    if ok {
        if s.description.trim().is_empty() {
            app.status("Describe the revision first".to_string());
            return false;
        }
        return app.run("sheet.revise", params(s)).is_ok();
    }
    cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revise_without_a_description_opens_the_dialog_and_ok_adds_the_revision() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default());
        app.run("sheet.set", json!({})).unwrap();
        app.run("sheet.revise", json!({})).unwrap();
        assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some("revise"));
        let mut s = open(&app);
        assert!(!finish(&mut app, &s, true, false), "a description is required");
        s.description = "First issue".into();
        s.zone = "B4".into();
        assert!(finish(&mut app, &s, true, false));
        let rev = app.session.doc.model.elements.iter().find(|e| e.type_id == vizcraft_notation::sheet_pass::REVISION).unwrap();
        assert_eq!(rev.prop("zone").map(|v| v.display()).as_deref(), Some("B4"));
    }
}
