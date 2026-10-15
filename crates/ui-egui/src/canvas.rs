//! The central area: a page with the selected note's title and a text editor for its body.
//! Zoom scales the font. A real craft replaces this module with its own canvas.

use egui::{Rect, Stroke, Ui, vec2};
use serde_json::json;

use crate::CraftApp;
use crate::theme::{Tokens, regular, semibold};

/// Body text size at 100%.
pub const BASE_FONT: f32 = 14.0;
/// Page width at 100%; the page grows with zoom and is centred in the canvas.
pub const PAGE_W: f32 = 680.0;

/// Draw the editor for the current note.
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let zoom = app.session.view.zoom;
    let note = app.session.sel.note;
    let Some(n) = app.session.doc.notes.get(note) else { return };
    let title = if n.title.trim().is_empty() { format!("Note {}", note + 1) } else { n.title.clone() };
    let mut body = n.body.clone();
    let page_w = (PAGE_W * zoom).min(ui.available_width() - 40.0).max(200.0);
    egui::ScrollArea::vertical().id_salt("canvas_scroll").show(ui, |ui| {
        ui.add_space(24.0);
        let x = ui.max_rect().center().x - page_w / 2.0;
        let page_top = ui.cursor().min.y;
        ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_size(egui::pos2(x, page_top), vec2(page_w, f32::INFINITY))), |ui| {
            egui::Frame::NONE.fill(t.page).stroke(Stroke::new(1.0, t.border)).inner_margin(28.0 * zoom.clamp(0.6, 2.0)).show(ui, |ui| {
                ui.set_min_height(600.0 * zoom);
                ui.set_width(page_w - 56.0 * zoom.clamp(0.6, 2.0));
                ui.label(egui::RichText::new(title).font(semibold(BASE_FONT * 1.6 * zoom)).color(t.text));
                ui.add_space(10.0 * zoom);
                let id = ui.make_persistent_id(("note_body", note));
                let edit = egui::TextEdit::multiline(&mut body)
                    .id(id)
                    .font(regular(BASE_FONT * zoom))
                    .text_color(t.text)
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY)
                    .desired_rows(20)
                    .hint_text("Type your note here");
                let resp = ui.add(edit);
                // The field edits a copy; the document changes through a command so it is undoable
                // and visible to every other front.
                if resp.changed() {
                    let _ = app.run("edit.setText", json!({"text": body, "note": note}));
                }
                // Mirror the caret into the session, so clipboard commands from the ribbon act on
                // what the user sees selected.
                if let Some(state) = egui::TextEdit::load_state(ui.ctx(), id)
                    && let Some(range) = state.cursor.char_range()
                {
                    app.session.sel.start = range.primary.index.into();
                    app.session.sel.end = range.secondary.index.into();
                    app.session.clamp_selection();
                }
            });
        });
        ui.add_space(40.0);
    });
}
