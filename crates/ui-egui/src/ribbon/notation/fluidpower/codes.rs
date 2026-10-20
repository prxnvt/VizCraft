//! Circuit > Codes: renumber the components (ISO 1219-2 codes) in letter or numeric form, label
//! the valves' ports, and place the component list. The menus check the form and the labels the
//! page uses now.

use serde_json::{Value, json};
use vizcraft_notation::fluid::{Circuit, Role};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size};

/// The codes of the components on the active page.
fn codes(app: &CraftApp) -> Vec<String> {
    let doc = &app.session.doc;
    let c = Circuit::build(doc, vizcraft_notation::catalog(), Some(doc.active_index()));
    c.parts.iter().filter_map(|p| doc.model.element(&p.element)).map(|e| e.tag.clone()).filter(|t| !t.is_empty()).collect()
}

fn numeric(app: &CraftApp) -> bool {
    let c = codes(app);
    !c.is_empty() && c.iter().all(|t| t.contains('.'))
}

fn letters(app: &CraftApp) -> bool {
    let c = codes(app);
    !c.is_empty() && c.iter().all(|t| !t.contains('.'))
}

/// Whether the page's valves (the selected one, else all of them) are numbered by ISO 5599.
fn labels(app: &CraftApp) -> Option<bool> {
    let doc = &app.session.doc;
    let c = Circuit::build(doc, vizcraft_notation::catalog(), Some(doc.active_index()));
    let sel = app.session.sel.primary.as_deref();
    let valves: Vec<bool> = c
        .parts
        .iter()
        .filter(|p| p.role == Role::Valve && sel.is_none_or(|s| s == p.shape || c.part_of_shape(s).is_none()))
        .filter_map(|p| p.valve.as_ref().map(|v| v.numbers))
        .collect();
    match valves.first() {
        Some(&first) if valves.iter().all(|&v| v == first) => Some(first),
        _ => None,
    }
}

fn iso5599(app: &CraftApp) -> bool {
    labels(app) == Some(true)
}

fn iso9461(app: &CraftApp) -> bool {
    labels(app) == Some(false)
}

fn component_list(_app: &CraftApp) -> Value {
    json!({"type": "fluidpower.ComponentList"})
}

/// The Codes group.
pub(super) fn group() -> Group {
    Group::new(
        "Codes",
        vec![Stack(vec![
            Menu {
                label: "Renumber",
                icon: "numberShapes",
                big: false,
                menu: vec![
                    MenuEntry::new("In the Edition's Form", "fluid.renumber", json!({})),
                    MenuEntry::new("Letter Codes (1V1)", "fluid.renumber", json!({"codes": "letters"})).checked(letters),
                    MenuEntry::new("Numeric Codes (1.1)", "fluid.renumber", json!({"codes": "numeric"})).checked(numeric),
                ],
            },
            Menu {
                label: "Port Labels",
                icon: "portLabels",
                big: false,
                menu: vec![
                    MenuEntry::new("ISO 9461 Letters (P, A, B, T)", "fluid.setPortSet", json!({"set": "iso9461"})).checked(iso9461),
                    MenuEntry::new("ISO 5599 Numbers (1, 2, 4, 3)", "fluid.setPortSet", json!({"set": "iso5599"})).checked(iso5599),
                ],
            },
            Button { id: "fluid.insert", icon: "componentList", label: "Component List", size: Size::Small, params: component_list, checked: None },
        ])],
    )
}
