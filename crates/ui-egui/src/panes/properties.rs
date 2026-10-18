//! The Properties pane: the document title and core properties.

use egui::Ui;
use serde_json::json;

use super::{PaneSpec, Side};
use crate::CraftApp;

/// The Properties pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("properties", "Properties", Side::Right, 260.0, properties);

fn properties(app: &mut CraftApp, ui: &mut Ui) {
    ui.label(egui::RichText::new("Document title").small().weak());
    let mut doc_title = app.session.doc.title.clone();
    if ui.add(egui::TextEdit::singleline(&mut doc_title).desired_width(f32::INFINITY)).changed() {
        let _ = app.run("edit.setTitle", json!({"title": doc_title}));
    }
    ui.add_space(10.0);
    egui::Grid::new("doc_props").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        let core = &app.session.doc.core;
        ui.label(egui::RichText::new("Revision").weak());
        ui.label(core.revision.to_string());
        ui.end_row();
        if !app.session.author.is_empty() {
            ui.label(egui::RichText::new("Author").weak());
            ui.label(&app.session.author);
            ui.end_row();
        }
        if !core.modified.is_empty() {
            ui.label(egui::RichText::new("Saved").weak());
            ui.label(&core.modified);
            ui.end_row();
            ui.label(egui::RichText::new("By").weak());
            ui.label(&core.last_modified_by);
            ui.end_row();
        }
    });
}

