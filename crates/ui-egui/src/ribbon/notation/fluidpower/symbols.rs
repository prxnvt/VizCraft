//! Circuit > Symbols: the Valve dialog, the pressure valves and the hydraulic power unit, in one
//! stack of small buttons.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// The Symbols group: the Valve dialog and the power unit (pumps, cylinders and pressure valves
/// are in the Insert menus beside it, the pumps and cylinders by medium).
pub(super) fn group() -> Group {
    let pv = |label, t: &str| MenuEntry::new(label, "fluid.insert", json!({"type": t}));
    Group::new(
        "Symbols",
        vec![Stack(vec![
            Small { id: "ui.valve", icon: "fluidValve" },
            Menu {
                label: "Pressure Valve",
                icon: "pressureValve",
                big: false,
                menu: vec![
                    pv("Relief Valve", "fluidpower.ReliefValve"),
                    pv("Pressure Reducing Valve", "fluidpower.ReducingValve"),
                    pv("Sequence Valve", "fluidpower.SequenceValve"),
                    pv("Pressure Regulator", "fluidpower.PressureRegulator"),
                ],
            },
            Small { id: "fluid.powerUnit", icon: "powerUnit" },
        ])],
    )
}
