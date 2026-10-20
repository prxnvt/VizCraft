//! Circuit > Symbols: the Valve dialog, the pressure valves, a pump and a cylinder of the page's
//! medium, and the hydraulic power unit.

use serde_json::{Value, json};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size};

fn pump(app: &CraftApp) -> Value {
    json!({"type": if super::pneumatic(app) { "fluidpower.Compressor" } else { "fluidpower.HydraulicPump" }})
}

fn cylinder(app: &CraftApp) -> Value {
    json!({"type": if super::pneumatic(app) { "fluidpower.PneumaticCylinder" } else { "fluidpower.HydraulicCylinder" }})
}

/// The Symbols group.
pub(super) fn group() -> Group {
    let pv = |label, t: &str| MenuEntry::new(label, "fluid.insert", json!({"type": t}));
    Group::new(
        "Symbols",
        vec![
            Big { id: "ui.valve", icon: "symbol" },
            Menu {
                label: "Pressure Valve",
                icon: "pressureValve",
                big: true,
                menu: vec![
                    pv("Relief Valve", "fluidpower.ReliefValve"),
                    pv("Pressure Reducing Valve", "fluidpower.ReducingValve"),
                    pv("Sequence Valve", "fluidpower.SequenceValve"),
                    pv("Pressure Regulator", "fluidpower.PressureRegulator"),
                ],
            },
            Stack(vec![
                Button { id: "fluid.insert", icon: "fluidPump", label: "Pump", size: Size::Small, params: pump, checked: None },
                Button { id: "fluid.insert", icon: "fluidCylinder", label: "Cylinder", size: Size::Small, params: cylinder, checked: None },
            ]),
            Big { id: "fluid.powerUnit", icon: "powerUnit" },
        ],
    )
}
