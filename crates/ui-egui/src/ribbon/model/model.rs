//! Model > Model: show the Model Tree, find where an element is drawn, move it in the model, and
//! delete it from the model or from this view only.

use serde_json::{Value, json};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};
use crate::widgets::{menu_button_if, menu_item};

/// The model elements the selected shapes draw, once each, in selection order.
fn selected_elements(app: &CraftApp) -> Vec<String> {
    let Some(page) = app.session.doc.page() else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for id in &app.session.sel.items {
        if let Some(e) = page.shape(id).and_then(|s| s.element.clone()).filter(|e| !out.contains(e)) {
            out.push(e);
        }
    }
    out
}

fn tree_open(app: &CraftApp) -> bool {
    app.ui.panes.is_open("navigation")
}

/// Show in All Views: the pages that draw the selected element, a click goes to one.
fn views_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    let element = selected_elements(app).into_iter().next();
    menu_button_if(ui, app, "layout", "Show in All Views", "Show in All Views", false, element.is_some(), |ui, app| {
        let Some(e) = element else { return };
        for v in vizcraft_diagram::views::views_of(&app.session.doc, &e) {
            let name = app.session.doc.pages.get(v.page).map(|p| p.name.clone()).unwrap_or_default();
            menu_item(ui, app, &name, "page.setActive", json!({"page": v.page_id}), Some(app.session.doc.page().is_some_and(|p| p.id == v.page_id)));
        }
    });
}

/// Move in Model: none, or one of the container elements, as the owner of the selected element.
fn move_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    let element = selected_elements(app).into_iter().next();
    menu_button_if(ui, app, "container", "Move in Model", "Move in Model", false, element.is_some(), |ui, app| {
        let Some(e) = element else { return };
        let cat = vizcraft_notation::catalog();
        let owner = app.session.doc.model.element(&e).and_then(|x| x.owner.clone());
        menu_item(ui, app, "No owner", "model.move", json!({"id": e, "owner": Value::Null}), Some(owner.is_none()));
        let containers: Vec<(String, String)> = app
            .session
            .doc
            .model
            .elements
            .iter()
            .filter(|x| x.id != e && cat.element_type(&x.type_id).is_some_and(|(_, t)| t.container))
            .take(40)
            .map(|x| (x.id.clone(), if x.name.is_empty() { x.id.clone() } else { x.name.clone() }))
            .collect();
        for (id, name) in containers {
            let on = owner.as_deref() == Some(id.as_str());
            menu_item(ui, app, &name, "model.move", json!({"id": e, "owner": id}), Some(on));
        }
    });
}

/// The Model tab's Model group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Model",
        vec![
            Stack(vec![Toggle { id: "ui.pane.navigation", icon: "panelLeft", checked: tree_open }, Custom(views_menu), Custom(move_menu)]),
            Stack(vec![
                Button {
                    id: "model.delete",
                    icon: "delete",
                    label: "Delete from Model",
                    size: Size::Small,
                    params: |a| json!({"ids": selected_elements(a)}),
                    checked: None,
                },
                Small { id: "element.deleteFromView", icon: "delete" },
            ]),
        ],
    )
}
