//! Process > Subprocess: move the selected shapes onto a page of their own behind one linked
//! shape, make an empty subprocess page, or link the selected shape to an existing page.

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};
use crate::widgets::{menu_button, menu_item};

/// The Link to Existing menu: one row per page.
fn pages_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "subprocessLink", "Link to Existing", "Link the selected shape to a page", false, |ui, app| {
        let pages: Vec<(String, String)> = app.session.doc.pages.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
        for (id, name) in pages {
            menu_item(ui, app, &name, "process.linkSubprocess", json!({"page": id}), None);
        }
    });
}

/// The Process tab's Subprocess group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Subprocess",
        vec![
            Button {
                id: "process.createSubprocess",
                icon: "subprocess",
                label: "Create from Selection",
                size: Size::Big,
                params: |_| json!({}),
                checked: None,
            },
            Stack(vec![
                Button {
                    id: "process.createSubprocess",
                    icon: "subprocessNew",
                    label: "Create New",
                    size: Size::Small,
                    params: |_| json!({"ids": []}),
                    checked: None,
                },
                Custom(pages_menu),
            ]),
        ],
    )
}
