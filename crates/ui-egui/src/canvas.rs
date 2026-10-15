//! The central canvas area. A real craft replaces this module with its own canvas (such as an
//! arrangement workspace, diagram surface, timeline, or schema grid).

use egui::Ui;

use crate::CraftApp;
use crate::theme::Tokens;

/// Draw the central canvas area.
pub fn show(_app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(t.canvas).show(ui, |ui| {
        ui.allocate_space(ui.available_size());
    });
}
