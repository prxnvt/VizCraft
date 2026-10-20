//! Electrical > Devices: Link to Coil (the selected contacts take the selected coil's tag),
//! Terminal Strip (ten terminals at the centre of the view), Cable (between the first two
//! selected devices), Assign I/O (the selected device on the selected module's next free
//! channel), Place Footprints (every device not yet on this panel page) and Number Wires (from
//! sheet and rung, `ladder.numberWires`). Each is one engine
//! command; what it needs comes from the selection, and the engine says why when it is missing.

use serde_json::{Value, json};
use vizcraft_engine::doc::Document;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The selected elements with their types, in selection order.
fn selected(app: &CraftApp) -> Vec<(String, String)> {
    let doc: &Document = &app.session.doc;
    let mut out: Vec<(String, String)> = Vec::new();
    for id in &app.session.sel.items {
        let el = vizcraft_diagram::views::element_of_shape(doc, id).map(str::to_string).or_else(|| doc.model.element(id).map(|e| e.id.clone()));
        if let Some(e) = el.and_then(|e| doc.model.element(&e))
            && !out.iter().any(|(x, _)| *x == e.id)
        {
            out.push((e.id.clone(), e.type_id.clone()));
        }
    }
    out
}

fn link_params(app: &CraftApp) -> Value {
    let sel = selected(app);
    let coil = sel.iter().find(|(_, t)| t == "electrical.Coil" || t == "electrical.Overload").map(|(id, _)| id.clone());
    let contacts: Vec<String> = sel.iter().filter(|(_, t)| t == "electrical.Contact").map(|(id, _)| id.clone()).collect();
    json!({"coil": coil, "contacts": contacts})
}

fn strip_params(app: &CraftApp) -> Value {
    let at = crate::canvas::insert_point(app);
    json!({"x": at.x, "y": at.y, "count": 10})
}

fn cable_params(app: &CraftApp) -> Value {
    let sel = selected(app);
    let ends: Vec<&String> = sel.iter().filter(|(_, t)| t != "electrical.Cable").map(|(id, _)| id).take(2).collect();
    json!({"from": ends.first(), "to": ends.get(1), "cores": 3})
}

fn io_params(app: &CraftApp) -> Value {
    let sel = selected(app);
    let module = sel.iter().find(|(_, t)| t == "electrical.IoModule").map(|(id, _)| id.clone());
    let device = sel.iter().find(|(_, t)| t != "electrical.IoModule").and_then(|(id, _)| app.session.doc.model.element(id)).map(|e| e.tag.clone());
    // The next channel no point of the module uses yet.
    let used: Vec<f64> = module
        .as_deref()
        .map(|m| {
            app.session
                .doc
                .model
                .elements
                .iter()
                .filter(|e| e.type_id == "electrical.IoPoint" && e.owner.as_deref() == Some(m))
                .filter_map(|e| e.prop("channel").and_then(|v| v.as_f64()))
                .collect()
        })
        .unwrap_or_default();
    let channel = (0..1024).find(|c| !used.contains(&f64::from(*c))).unwrap_or(0);
    json!({"module": module, "channel": channel, "tag": device})
}

fn none(_: &CraftApp) -> Value {
    json!({})
}

/// The Devices group.
pub(super) fn group() -> Group {
    Group::new(
        "Devices",
        vec![
            Stack(vec![
                Button {
                    id: "terminal.strip",
                    icon: "terminalStrip",
                    label: "Terminal Strip",
                    size: Size::Small,
                    params: strip_params,
                    checked: None,
                },
                Button { id: "electrical.link", icon: "linkCoil", label: "Link to Coil", size: Size::Small, params: link_params, checked: None },
                Button { id: "cable.create", icon: "cable", label: "Cable", size: Size::Small, params: cable_params, checked: None },
            ]),
            Stack(vec![
                Button { id: "io.assign", icon: "ioAssign", label: "Assign I/O", size: Size::Small, params: io_params, checked: None },
                Button { id: "electrical.placeFootprints", icon: "footprints", label: "Footprints", size: Size::Small, params: none, checked: None },
                Button { id: "ladder.numberWires", icon: "numberWires", label: "Number Wires", size: Size::Small, params: none, checked: None },
            ]),
        ],
    )
}
