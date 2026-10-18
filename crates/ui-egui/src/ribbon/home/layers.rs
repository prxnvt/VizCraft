//! Home > Editing > Layers: a menu that puts the selected shapes on a layer of the active page
//! (or takes them off one they are all on) and opens the Layers pane for the layer properties.

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control;
use crate::widgets::{big_label, menu_button, menu_item};

/// The Layers menu button for the Editing group.
pub(in crate::ribbon) fn control() -> Control {
    Control::Custom(layers_menu)
}

/// The menu is drawn by hand because its entries are the page's layers, which change.
fn layers_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "shape", &big_label("Layers"), "Layers", true, |ui, app| {
        let Some(page) = app.session.doc.page() else { return };
        let selected: Vec<&vizcraft_diagram::Shape> = app.session.sel.items.iter().filter_map(|id| page.shape(id)).collect();
        // Each layer with whether every selected shape is already on it.
        let entries: Vec<(String, String, bool)> = page
            .layers
            .iter()
            .map(|l| (l.id.clone(), l.name.clone(), !selected.is_empty() && selected.iter().all(|s| s.layers.contains(&l.id))))
            .collect();
        let has_sel = !selected.is_empty();
        ui.label(egui::RichText::new("Assign to Layer").small().weak());
        if entries.is_empty() {
            ui.add_enabled(false, egui::Button::new("No layers on this page"));
        }
        for (id, name, on) in entries {
            let resp = ui.add_enabled(has_sel, egui::Button::selectable(on, name));
            if resp.clicked() {
                let cmd = if on { "layer.unassign" } else { "layer.assign" };
                let _ = app.run(cmd, json!({"layer": id}));
                ui.close();
            }
        }
        ui.separator();
        menu_item(ui, app, "Layer Properties...", "ui.pane.layers", json!({"value": true}));
    });
}
