//! Insert > Diagram Parts: the connector and text tools, containers (with their style gallery),
//! callouts and swimlanes as big buttons, and the Compartment menu.

use serde_json::json;
use vizcraft_diagram::container::{CALLOUT_STYLES, CONTAINER_STYLES};

use crate::ribbon::Control::*;
use crate::CraftApp;
use crate::ribbon::{Group, MenuEntry, Size};
use crate::widgets::{menu_button_if, menu_item};

/// One menu entry per preset: `id` with `{"style": name}`.
fn styles(id: &'static str, presets: &'static [vizcraft_diagram::container::Preset]) -> Vec<MenuEntry> {
    presets.iter().map(|p| MenuEntry::new(p.label, id, json!({"style": p.name}))).collect()
}

/// The shape a compartment goes on: the one selected shape, and only when it is the whole
/// selection (`compartment.add` takes one shape).
fn compartment_target(app: &CraftApp) -> Option<String> {
    let page = app.session.doc.page()?;
    let mut shapes = app.session.sel.items.iter().filter(|id| page.shape(id).is_some());
    let one = shapes.next()?;
    (shapes.next().is_none() && app.session.sel.items.len() == 1).then(|| one.clone())
}

/// The Compartment menu: a paragraph, list or slot band added below the selected shape's text.
/// Drawn by hand because the params carry the selected shape's id; disabled unless exactly one
/// shape is selected.
fn compartment_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    let target = compartment_target(app);
    menu_button_if(ui, app, "compartment", "Compartment", "Compartment", false, target.is_some(), |ui, app| {
        let Some(id) = compartment_target(app) else { return };
        for (label, kind) in [("Paragraph", "paragraph"), ("List", "list"), ("Slot", "slot")] {
            menu_item(ui, app, label, "compartment.add", json!({"id": id, "kind": kind}), None);
        }
    });
}

/// The Insert tab's Diagram Parts group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Diagram Parts",
        vec![
            Button { id: "ui.tool", icon: "connector", label: "Connector", size: Size::Big, params: |_| json!({"tool": "connector"}), checked: None },
            Button { id: "ui.tool", icon: "text", label: "Text", size: Size::Big, params: |_| json!({"tool": "text"}), checked: None },
            // Around the selection, else an empty one in the middle of the page.
            Split { id: "container.add", icon: "container", big: true, menu: styles("container.add", CONTAINER_STYLES) },
            Split { id: "callout.add", icon: "callout", big: true, menu: styles("callout.add", CALLOUT_STYLES) },
            Split {
                id: "lane.addSet",
                icon: "swimlanes",
                big: true,
                menu: vec![
                    MenuEntry::new("Horizontal Lanes", "lane.addSet", json!({"orientation": "horizontal"})),
                    MenuEntry::new("Vertical Lanes", "lane.addSet", json!({"orientation": "vertical"})),
                    MenuEntry::new("Two Lanes", "lane.addSet", json!({"lanes": 2})),
                    MenuEntry::new("Four Lanes", "lane.addSet", json!({"lanes": 4})),
                ],
            },
            Stack(vec![Custom(compartment_menu)]),
        ],
    )
}
