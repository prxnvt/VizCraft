//! Circuit > Codes: renumber the components (ISO 1219-2 codes) in the page edition's form or
//! either form, label the valves' ports, and place the component list.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// The Codes group.
pub(super) fn group() -> Group {
    Group::new(
        "Codes",
        vec![
            Split {
                id: "fluid.renumber",
                icon: "numberShapes",
                big: true,
                menu: vec![
                    MenuEntry::new("Letter Codes (1V1)", "fluid.renumber", json!({"codes": "letters"})),
                    MenuEntry::new("Numeric Codes (1.1)", "fluid.renumber", json!({"codes": "numeric"})),
                ],
            },
            Menu {
                label: "Port Labels",
                icon: "portLabels",
                big: false,
                menu: vec![
                    MenuEntry::new("ISO 9461 Letters (P, A, B, T)", "fluid.setPortSet", json!({"set": "iso9461"})),
                    MenuEntry::new("ISO 5599 Numbers (1, 2, 4, 3)", "fluid.setPortSet", json!({"set": "iso5599"})),
                ],
            },
            Menu {
                label: "Component List",
                icon: "componentList",
                big: false,
                menu: vec![MenuEntry::new("Insert Component List", "fluid.insert", json!({"type": "fluidpower.ComponentList"}))],
            },
        ],
    )
}
