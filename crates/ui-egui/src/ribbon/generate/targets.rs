//! Generate > Targets: one button per built-in target from `generate.targets`, the current one
//! checked; a click makes it the Generate tab's target through `ui.generate`.

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::Group;
use crate::widgets;

/// The targets as a column of small toggles.
fn gallery(app: &mut CraftApp, ui: &mut egui::Ui) {
    let targets = crate::dock::generate::targets(app);
    let current = crate::dock::generate::current_target(app);
    let inline = app.ui.dock.state.generate.spec.trim().is_empty();
    widgets::stack(ui, |ui| {
        for (id, label) in &targets {
            let on = inline && *id == current;
            widgets::toggle(ui, app, "generateTarget", Some(label), label, "ui.generate", json!({"target": id}), on);
        }
    });
}

/// The Generate tab's Targets group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Targets", vec![Custom(gallery)])
}
