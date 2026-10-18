//! The Generate tab: an empty slot until generators land.

use egui::Ui;

use crate::CraftApp;
use crate::theme::Tokens;

/// The slot's body: says what will fill it.
pub(super) fn show(_app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    ui.label(egui::RichText::new("Generate").strong());
    ui.label(egui::RichText::new("The preview diff and run log of generators arrive with Cascade 5B.").color(t.text_dim));
}
