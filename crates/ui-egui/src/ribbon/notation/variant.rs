//! Notation > Variant: the rendering variants of the active pack, the page's own checked
//! (`variant.set`). "Default" is the pack's own drawing.

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::Group;
use crate::widgets::{big_label, menu_button, menu_item};

/// The page's variant id, empty for the default drawing.
pub(in crate::ribbon) fn current(app: &CraftApp) -> String {
    app.session.doc.page().and_then(|p| p.view.as_ref()).map(|v| v.variant.clone()).unwrap_or_default()
}

fn menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "variant", &big_label("Variant"), "Variant", true, |ui, app| {
        let Some(pack) = super::active_pack(app) else { return };
        let now = current(app);
        menu_item(ui, app, "Default", "variant.set", json!({"variant": ""}), Some(now.is_empty()));
        for v in &pack.variants {
            let label = if v.label.is_empty() { v.id.as_str() } else { v.label.as_str() };
            menu_item(ui, app, label, "variant.set", json!({"variant": v.id}), Some(now == v.id));
        }
    });
}

/// The notation tab's Variant group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Variant", vec![Custom(menu)])
}
