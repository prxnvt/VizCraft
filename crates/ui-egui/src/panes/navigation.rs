//! The Navigation pane: a placeholder until there is a model tree to show.

use egui::Ui;

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::theme::Tokens;

/// The Navigation pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("navigation", "Navigation", Side::Left, 240.0, navigation);

fn navigation(_app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(egui::RichText::new("No items").small().color(t.text_dim));
}
