//! The Tag Formats dialog: the project's own tag format and numbering group for each identifier
//! scope of the active page's pack. It opens when `identity.setFormat` runs without a scope
//! (Notation > Tags > Tag Formats) and ends by running every change as one `edit.batch`, so the
//! whole edit is one undo step.

use serde::Serialize;
use serde_json::{Value, json};

use crate::CraftApp;

/// One scope as the dialog edits it.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Row {
    /// The scope id.
    pub scope: String,
    /// What it tags, for the label (the scope's local name).
    pub label: String,
    /// The pack's format, shown as the hint.
    pub pack_format: String,
    /// The format as edited.
    pub format: String,
    /// The format when the dialog opened.
    pub was_format: String,
    /// The numbering group as edited (a property key, empty for one count).
    pub number_by: String,
    /// The numbering group when the dialog opened.
    pub was_number_by: String,
}

/// The dialog's rows.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct State {
    /// One row per scope of the pack.
    pub rows: Vec<Row>,
    /// The pack the scopes come from.
    pub pack: String,
}

/// The pack whose scopes the dialog shows: the active page's viewpoint's, else `process`.
fn pack_of(app: &CraftApp) -> String {
    crate::palette::active_viewpoint(app).and_then(|v| v.split_once('.')).map_or_else(|| "process".to_string(), |(p, _)| p.to_string())
}

/// The dialog: the pack's scopes as the document reads them (`identity.formats` gives the same).
pub fn open(app: &CraftApp) -> State {
    let pack = pack_of(app);
    let scopes = vizcraft_notation::catalog().scopes();
    let reg = vizcraft_diagram::vizcraft_model::Registry::new(&app.session.doc.model, &scopes);
    let rows = scopes
        .iter()
        .zip(reg.effective())
        .filter(|(sc, _)| sc.id.split_once('.').is_some_and(|(p, _)| p == pack))
        .map(|(sc, now)| Row {
            scope: sc.id.clone(),
            // Named after the first type it tags, as people read it (Plant Unit, Line, Loop).
            label: sc
                .types
                .first()
                .and_then(|t| vizcraft_notation::catalog().element_type(t))
                .map(|(_, t)| t.label.clone())
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| sc.id.split_once('.').map_or(sc.id.as_str(), |(_, l)| l).to_string()),
            pack_format: sc.format.clone(),
            format: now.format.clone(),
            was_format: now.format.clone(),
            number_by: now.number_by.clone(),
            was_number_by: now.number_by.clone(),
        })
        .collect();
    State { rows, pack }
}

/// The grid of scopes above the OK and Cancel buttons.
pub fn body(ui: &mut egui::Ui, s: &mut State) {
    if s.rows.is_empty() {
        ui.label("This notation has no tag formats.");
        return;
    }
    ui.label("Tags are composed from these formats: {n} is the number, {key} a property, {owner:Type.tag} an owner's tag.");
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        egui::Grid::new("tag-formats").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(egui::RichText::new("Tags of").weak());
            ui.label(egui::RichText::new("Format").weak());
            ui.label(egui::RichText::new("Numbered within").weak());
            ui.end_row();
            for r in &mut s.rows {
                ui.label(&r.label);
                ui.add_sized([250.0, 20.0], egui::TextEdit::singleline(&mut r.format).hint_text(&r.pack_format));
                ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut r.number_by).hint_text("one count"));
                ui.end_row();
            }
        });
    });
}

/// The `identity.setFormat` steps for the rows that changed.
pub fn steps(s: &State) -> Vec<Value> {
    s.rows
        .iter()
        .filter(|r| r.format.trim() != r.was_format || r.number_by.trim() != r.was_number_by)
        .map(|r| {
            let format = if r.format.trim().is_empty() || r.format.trim() == r.pack_format { Value::Null } else { json!(r.format.trim()) };
            json!({"command": "identity.setFormat", "params": {"scope": r.scope, "format": format, "numberBy": r.number_by.trim()}})
        })
        .collect()
}

/// After the buttons: on OK run the changes as one batch. Returns whether to close.
pub fn finish(app: &mut CraftApp, s: &State, ok: bool, cancel: bool) -> bool {
    if ok {
        let commands = steps(s);
        if commands.is_empty() {
            return true;
        }
        return match app.run("edit.batch", json!({"label": "Tag Formats", "commands": commands})) {
            Ok(_) => true,
            Err(e) => {
                app.status(e);
                false
            }
        };
    }
    cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_format_without_a_scope_opens_the_dialog_and_ok_commits_once() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default());
        app.run("template.new", json!({"template": "process.pid"})).unwrap();
        app.run("identity.setFormat", json!({})).unwrap();
        assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some("tagFormats"));
        let mut s = open(&app);
        assert!(s.rows.iter().any(|r| r.scope == "process.line"));
        for r in &mut s.rows {
            if r.scope == "process.line" {
                r.format = "{fluidCode}-{n:4}".into();
            }
            if r.scope == "process.loop" {
                r.number_by.clear();
            }
        }
        let before = app.session.transactions().len();
        assert!(finish(&mut app, &s, true, false));
        assert_eq!(app.session.transactions().len(), before + 1, "one undo step");
        let o = &app.session.doc.model.id_formats;
        assert_eq!(o.get("process.line").map(|x| x.format.as_str()), Some("{fluidCode}-{n:4}"));
        assert_eq!(o.get("process.loop").and_then(|x| x.number_by.as_deref()), Some(""));
        app.run("edit.undo", json!({})).unwrap();
        assert!(app.session.doc.model.id_formats.is_empty());
    }
}
