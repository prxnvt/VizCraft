//! The DEXPI class each process pack type stands for: the mapping a later Proteus (DEXPI XML)
//! export will write.
//!
//! The class names were checked against the class lists of the public DEXPI P&ID Specification
//! 1.4 reference pages (Equipment, Piping and Instrumentation packages; the specification is
//! licensed CC BY 4.0). Only the mapping is used: no DEXPI text, figure or schema is copied. A
//! type whose class was not confirmed there is listed in [`UNMAPPED`] instead of guessed. The
//! reference data library URIs of the classes were not checked, so none is given yet.

/// One process type's DEXPI class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    /// The process pack type (`process.Pump`).
    pub type_id: &'static str,
    /// The DEXPI 1.4 class name (`Pump`).
    pub class: &'static str,
    /// The class's reference data URI, where it was checked (none yet).
    pub uri: Option<&'static str>,
}

const fn m(type_id: &'static str, class: &'static str) -> Mapping {
    Mapping { type_id, class, uri: None }
}

/// The checked mappings.
pub const MAPPINGS: &[Mapping] = &[
    m("process.Equipment", "Equipment"),
    m("process.Vessel", "Vessel"),
    m("process.Tank", "Tank"),
    m("process.ProcessColumn", "ProcessColumn"),
    m("process.HeatExchanger", "HeatExchanger"),
    m("process.Pump", "Pump"),
    m("process.Compressor", "Compressor"),
    m("process.Blower", "Blower"),
    m("process.Fan", "Fan"),
    m("process.Filter", "Filter"),
    m("process.Separator", "Separator"),
    m("process.Agitator", "Agitator"),
    m("process.Furnace", "Furnace"),
    m("process.Dryer", "Dryer"),
    m("process.CoolingTower", "CoolingTower"),
    m("process.Centrifuge", "Centrifuge"),
    m("process.Boiler", "Boiler"),
    m("process.Nozzle", "Nozzle"),
    m("process.PipingNetworkSystem", "PipingNetworkSystem"),
    m("process.PipingComponent", "PipingComponent"),
    m("process.GateValve", "GateValve"),
    m("process.GlobeValve", "GlobeValve"),
    m("process.BallValve", "BallValve"),
    m("process.ButterflyValve", "ButterflyValve"),
    m("process.PlugValve", "PlugValve"),
    m("process.NeedleValve", "NeedleValve"),
    m("process.CheckValve", "CheckValve"),
    m("process.AngleValve", "AngleValve"),
    m("process.SafetyValveOrFitting", "SafetyValveOrFitting"),
    m("process.PipeReducer", "PipeReducer"),
    m("process.Strainer", "Strainer"),
    m("process.Flange", "Flange"),
    m("process.LineBlind", "LineBlind"),
    m("process.Tee", "PipeTee"),
    m("process.Silencer", "Silencer"),
    m("process.PipeOffPageConnector", "PipeOffPageConnector"),
    m("process.ProcessInstrumentationFunction", "ProcessInstrumentationFunction"),
    m("process.ProcessSignalGeneratingFunction", "ProcessSignalGeneratingFunction"),
    m("process.InstrumentationLoopFunction", "InstrumentationLoopFunction"),
    m("process.SignalOffPageConnector", "SignalOffPageConnector"),
];

/// Process types with no confirmed DEXPI class: plain plant items DEXPI does not name (units,
/// loop diagram, HVAC, plumbing and isometric items) and valves whose class was not found.
pub const UNMAPPED: &[&str] = &[
    "process.Unit",
    "process.DuctRun",
    "process.DiaphragmValve",
    "process.ThreeWayValve",
    "process.Cap",
    "process.JunctionBox",
    "process.TerminalStrip",
    "process.Terminal",
    "process.Cable",
    "process.MarshallingCabinet",
    "process.IOChannel",
    "process.Barrier",
    "process.PowerSupply",
    "process.AirHandlingUnit",
    "process.Coil",
    "process.Humidifier",
    "process.Damper",
    "process.Diffuser",
    "process.TerminalUnit",
    "process.Chiller",
    "process.Fixture",
    "process.WaterHeater",
    "process.BackflowPreventer",
    "process.WaterMeter",
    "process.Elbow",
    "process.Weld",
    "process.PipeSupport",
    "process.NorthArrow",
    "process.BillOfMaterials",
];

/// The DEXPI class of `type_id`, when it is mapped.
pub fn class_of(type_id: &str) -> Option<&'static Mapping> {
    MAPPINGS.iter().find(|m| m.type_id == type_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_process_type_maps_or_is_listed_unmapped() {
        let cat = vizcraft_notation::Catalog::builtin();
        let pack = cat.pack("process").expect("the process pack loads");
        for t in &pack.element_types {
            let mapped = class_of(&t.id).is_some();
            let listed = UNMAPPED.contains(&t.id.as_str());
            assert!(mapped != listed, "{}: mapped {mapped}, listed unmapped {listed}", t.id);
        }
        for m in MAPPINGS {
            assert!(pack.element_type(m.type_id).is_some(), "{} is not a process type", m.type_id);
        }
        assert_eq!(class_of("process.Tee").map(|m| m.class), Some("PipeTee"));
    }
}
