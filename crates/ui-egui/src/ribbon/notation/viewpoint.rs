//! Notation > Viewpoint: the viewpoints of the active pack, the page's own checked
//! (`model.setViewpoint`).

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::Group;
use crate::widgets::{big_label, menu_button, menu_item};

fn menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "layout", &big_label("Viewpoint"), "Viewpoint", true, |ui, app| {
        let Some(pack) = super::active_pack(app) else { return };
        let current = crate::palette::active_viewpoint(app).map(str::to_string);
        for v in &pack.viewpoints {
            menu_item(ui, app, &v.label, "model.setViewpoint", json!({"viewpoint": v.id}), Some(current.as_deref() == Some(v.id.as_str())));
        }
    });
}

/// The notation tab's Viewpoint group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Viewpoint", vec![Custom(menu)])
}
