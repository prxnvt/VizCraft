//! Callout > Styles: a gallery of the callout style presets.

use egui::Ui;
use vizcraft_diagram::container::CALLOUT_STYLES;

use super::selected_callout;
use crate::CraftApp;
use crate::ribbon::{Control, Group};

fn gallery(app: &mut CraftApp, ui: &mut Ui) {
    let current = selected_callout(app).map(|c| c.style.clone());
    crate::ribbon::container::grid(app, ui, "callout.set", CALLOUT_STYLES, current, true);
}

/// The Callout tab's Styles group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Callout Styles", vec![Control::Custom(gallery)])
}
