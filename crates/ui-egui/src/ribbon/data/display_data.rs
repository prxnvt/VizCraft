//! Data > Display Data: the Data Graphics gallery (the selection's graphic checked), Edit Data
//! Graphic, Insert Legend and Remove.

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The gallery: one row per graphic of the document, checked when the primary selection shows
/// it, then No Data Graphic. Each row runs `datagraphic.apply` on the selection.
fn gallery(app: &mut CraftApp, ui: &mut egui::Ui) {
    let on = !app.session.sel.items.is_empty();
    crate::widgets::menu_button_if(ui, app, "dataGraphic", "Data Graphics", "Data Graphics", true, on, |ui, app| {
        let current = vizcraft_engine::cmd::datagraphic::current(&app.session);
        let graphics: Vec<(String, String)> = app.session.doc.data_graphics.iter().map(|g| (g.id.clone(), g.title().to_string())).collect();
        if graphics.is_empty() {
            ui.label(egui::RichText::new("No data graphics yet: use Edit Data Graphic").weak());
        }
        for (id, name) in graphics {
            let checked = current.as_deref() == Some(id.as_str());
            crate::widgets::menu_item(ui, app, &name, "datagraphic.apply", json!({"graphic": id}), Some(checked));
        }
        ui.separator();
        crate::widgets::menu_item(ui, app, "No Data Graphic", "datagraphic.apply", json!({"graphic": null}), Some(current.is_none()));
    });
}

/// The Data tab's Display Data group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Display Data",
        vec![
            Custom(gallery),
            Stack(vec![
                Button {
                    id: "ui.dialog",
                    icon: "settings",
                    label: "Edit Data Graphic",
                    size: Size::Small,
                    params: |_| json!({"name": "dataGraphic"}),
                    checked: None,
                },
                Small { id: "datagraphic.legend", icon: "legend" },
                Button {
                    id: "datagraphic.apply",
                    icon: "delete",
                    label: "Remove Data Graphic",
                    size: Size::Small,
                    params: |_| json!({"graphic": null}),
                    checked: None,
                },
            ]),
        ],
    )
}
