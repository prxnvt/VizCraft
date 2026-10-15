//! Side panes: Navigation (the list of notes) on the left, Properties (the selected note) on
//! the right. The layout persists in the preferences; cascade 2 makes panes dockable.

use egui::{Stroke, Ui};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::CraftApp;
use crate::theme::{Tokens, semibold};

/// Which panes sit on each side and how wide the sides are. Pane names: `navigation`,
/// `properties`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PaneLayout {
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub left_width: f32,
    pub right_width: f32,
}

impl Default for PaneLayout {
    fn default() -> Self {
        PaneLayout { left: vec!["navigation".into()], right: vec!["properties".into()], left_width: 240.0, right_width: 260.0 }
    }
}

/// Narrowest and widest a side can be.
pub const MIN_WIDTH: f32 = 140.0;
pub const MAX_WIDTH: f32 = 600.0;

impl PaneLayout {
    /// Clamp widths read from a preferences file (a corrupt one must not hide the canvas).
    pub fn sanitize(&mut self) {
        for w in [&mut self.left_width, &mut self.right_width] {
            if !w.is_finite() {
                *w = 240.0;
            }
            *w = w.clamp(MIN_WIDTH, MAX_WIDTH);
        }
    }

    /// Which side (`left`/`right`) a pane is on.
    pub fn side_of(&self, pane: &str) -> Option<&'static str> {
        if self.left.iter().any(|p| p == pane) {
            Some("left")
        } else if self.right.iter().any(|p| p == pane) {
            Some("right")
        } else {
            None
        }
    }
}

/// Whether a pane is turned on (View tab).
fn visible(app: &CraftApp, pane: &str) -> bool {
    match pane {
        "navigation" => app.session.view.nav_pane,
        "properties" => app.session.view.props_pane,
        _ => false,
    }
}

/// Draw the side panels. Call after the top and bottom bars, before the central panel.
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::NONE.fill(t.panel).inner_margin(10).stroke(Stroke::new(1.0, t.border));
    let left: Vec<String> = app.ui.panes.left.iter().filter(|p| visible(app, p)).cloned().collect();
    if !left.is_empty() {
        let r = egui::Panel::left("left_panes")
            .default_size(app.ui.panes.left_width)
            .size_range(MIN_WIDTH..=MAX_WIDTH)
            .resizable(true)
            .frame(frame)
            .show(ui, |ui| {
                for p in &left {
                    pane(app, ui, p);
                }
            });
        app.ui.panes.left_width = r.response.rect.width();
    }
    let right: Vec<String> = app.ui.panes.right.iter().filter(|p| visible(app, p)).cloned().collect();
    if !right.is_empty() {
        let r = egui::Panel::right("right_panes")
            .default_size(app.ui.panes.right_width)
            .size_range(MIN_WIDTH..=MAX_WIDTH)
            .resizable(true)
            .frame(frame)
            .show(ui, |ui| {
                for p in &right {
                    pane(app, ui, p);
                }
            });
        app.ui.panes.right_width = r.response.rect.width();
    }
}

fn pane(app: &mut CraftApp, ui: &mut Ui, name: &str) {
    match name {
        "navigation" => navigation(app, ui),
        "properties" => properties(app, ui),
        _ => {}
    }
}

/// Pane title with a close button; true when closed.
fn header(ui: &mut Ui, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(semibold(14.0)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::icon_button(ui, "close", "Close pane", 12.0, true) {
                close = true;
            }
        });
    });
    ui.add_space(4.0);
    close
}

fn navigation(app: &mut CraftApp, ui: &mut Ui) {
    if header(ui, "Navigation") {
        let _ = app.run("view.navigationPane", json!({"value": false}));
        return;
    }
    let t = Tokens::get(ui.ctx());
    let mut pick = None;
    egui::ScrollArea::vertical().id_salt("nav_notes").show(ui, |ui| {
        for (i, n) in app.session.doc.notes.iter().enumerate() {
            let title = if n.title.trim().is_empty() { format!("Note {}", i + 1) } else { n.title.clone() };
            let selected = i == app.session.sel.note;
            let resp = ui.add_sized([ui.available_width(), 24.0], egui::Button::selectable(selected, title));
            if resp.clicked() {
                pick = Some(i);
            }
            let preview: String = n.body.lines().next().unwrap_or("").chars().take(40).collect();
            if !preview.is_empty() {
                ui.label(egui::RichText::new(preview).small().color(t.text_dim));
            }
        }
    });
    if let Some(i) = pick {
        let _ = app.run("edit.select", json!({"note": i, "start": 0, "end": 0}));
    }
    ui.add_space(6.0);
    if ui.button("New note").clicked() {
        let _ = app.run("insert.note", json!({}));
    }
}

fn properties(app: &mut CraftApp, ui: &mut Ui) {
    if header(ui, "Properties") {
        let _ = app.run("view.propertiesPane", json!({"value": false}));
        return;
    }
    let note = app.session.sel.note;
    let Some(n) = app.session.doc.notes.get(note) else { return };
    let mut title = n.title.clone();
    let chars = n.body.chars().count();
    let words = n.body.split_whitespace().count();
    ui.label(egui::RichText::new("Note title").small().weak());
    // The field edits a copy; the document changes through a command, so it is undoable.
    if ui.add(egui::TextEdit::singleline(&mut title).desired_width(f32::INFINITY)).changed() {
        let _ = app.run("edit.setNoteTitle", json!({"title": title, "note": note}));
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Document title").small().weak());
    let mut doc_title = app.session.doc.title.clone();
    if ui.add(egui::TextEdit::singleline(&mut doc_title).desired_width(f32::INFINITY)).changed() {
        let _ = app.run("edit.setTitle", json!({"title": doc_title}));
    }
    ui.add_space(10.0);
    egui::Grid::new("note_props").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("Words").weak());
        ui.label(words.to_string());
        ui.end_row();
        ui.label(egui::RichText::new("Characters").weak());
        ui.label(chars.to_string());
        ui.end_row();
        ui.label(egui::RichText::new("Notes").weak());
        ui.label(app.session.doc.notes.len().to_string());
        ui.end_row();
        let core = &app.session.doc.core;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_survives_prefs_and_sanitizes_junk() {
        let mut l = PaneLayout { left_width: f32::NAN, right_width: 1.0e9, ..Default::default() };
        l.sanitize();
        assert_eq!((l.left_width, l.right_width), (240.0, MAX_WIDTH));
        let json = serde_json::to_string(&PaneLayout::default()).unwrap();
        assert!(json.contains("leftWidth"));
        let back: PaneLayout = serde_json::from_str(r#"{"left": ["properties"], "right": []}"#).unwrap();
        assert_eq!(back.side_of("properties"), Some("left"));
        assert_eq!(back.side_of("navigation"), None);
        assert_eq!(back.left_width, 240.0);
    }
}
