//! The History pane.

use egui::Ui;
use serde_json::json;

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::theme::Tokens;

/// The History pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("history", "History", Side::Right, 260.0, history);

/// The History pane: what Undo and Redo would do, with buttons, and the document revision.
fn history(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let undo = app.session.undo_label().map(str::to_string);
    let redo = app.session.redo_label().map(str::to_string);
    ui.horizontal(|ui| {
        if ui.add_enabled(undo.is_some(), egui::Button::new("Undo")).clicked() {
            let _ = app.run("edit.undo", json!({}));
        }
        if ui.add_enabled(redo.is_some(), egui::Button::new("Redo")).clicked() {
            let _ = app.run("edit.redo", json!({}));
        }
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Undo").small().weak());
    match &undo {
        Some(l) => ui.label(l.as_str()),
        None => ui.label(egui::RichText::new("Nothing to undo").color(t.text_dim)),
    };
    ui.add_space(6.0);
    ui.label(egui::RichText::new("Redo").small().weak());
    match &redo {
        Some(l) => ui.label(l.as_str()),
        None => ui.label(egui::RichText::new("Nothing to redo").color(t.text_dim)),
    };
    ui.add_space(10.0);
    egui::Grid::new("history_props").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("Revision").weak());
        ui.label(app.session.rev().to_string());
        ui.end_row();
        ui.label(egui::RichText::new("Unsaved").weak());
        ui.label(if app.session.dirty { "yes" } else { "no" });
        ui.end_row();
    });
}
