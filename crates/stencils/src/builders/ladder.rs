//! Ladder builder: the frame of a ladder diagram, its rung numbers down the left and a tick off
//! each rail where a rung runs.
//!
//! Provenance: own design, drawn from the written rules of NEMA ICS 19 and the JIC ladder convention: two vertical rails carry the control
//! voltage, rungs run across between them, numbered down the left rail. The rails themselves
//! are wires (named nets L1 and L2) drawn from the two ports at the top of the frame, so devices
//! wire to them like to any net; the frame draws only the numbers and ticks.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of NEMA ICS 19 and the JIC ladder convention";

/// Most rungs one frame holds.
pub const MAX_RUNGS: i64 = 200;

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "ladder",
    label: "Ladder",
    description: "The frame of a ladder diagram: rung numbers down the left (first, first + increment, ...), one rung every `spacing` modules between rails `width` modules apart. Ports L1 and L2 at the top of the left and right rails.",
    params: &[
        ParamSpec {
            key: "rungs",
            label: "Rungs",
            kind: ParamKind::Int { min: 1, max: MAX_RUNGS },
            default: Def::Int(10),
            description: "How many rungs.",
        },
        ParamSpec {
            key: "spacing",
            label: "Spacing",
            kind: ParamKind::Int { min: 2, max: 40 },
            default: Def::Int(4),
            description: "Modules from one rung to the next.",
        },
        ParamSpec {
            key: "first",
            label: "First rung",
            kind: ParamKind::Int { min: 0, max: 99_999 },
            default: Def::Int(1),
            description: "The number of the top rung.",
        },
        ParamSpec {
            key: "increment",
            label: "Increment",
            kind: ParamKind::Int { min: 1, max: 1000 },
            default: Def::Int(1),
            description: "Step between rung numbers.",
        },
        ParamSpec {
            key: "width",
            label: "Width",
            kind: ParamKind::Int { min: 8, max: 400 },
            default: Def::Int(40),
            description: "Modules between the rails.",
        },
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(&["iec60617", "jic", "ieee315"]),
            default: Def::Text("iec60617"),
            description: "The symbol set, which picks the module (2.5 mm or 0.1 inch).",
        },
    ],
    sources: &[("NEMA ICS 19", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    let std = match p.text("standard") {
        "jic" => "jic",
        "ieee315" => "ieee315",
        _ => "iec60617",
    };
    d.standard(std);
    let rungs = p.int("rungs").clamp(1, MAX_RUNGS);
    let spacing = p.int("spacing").clamp(2, 40) as f64;
    let first = p.int("first").clamp(0, 99_999);
    let inc = p.int("increment").clamp(1, 1000);
    let width = p.int("width").clamp(8, 400) as f64;
    // Three modules of margin on the left hold the rung numbers clear of the rail's junctions.
    let (left, right) = (3.0, 3.0 + width);
    let h = spacing * rungs as f64;
    d.size(right, h);
    for i in 0..rungs {
        let y = spacing * (i as f64 + 0.5);
        let n = first.saturating_add(inc.saturating_mul(i));
        d.label(&format!("r{i}"), &n.to_string(), 1.2, y, 0.9, "middle");
        // A short tick off each rail marks where the rung runs; a full guide would crowd the
        // devices and cost a command per dash.
        d.line(&[(left, y), (left + 0.5, y)]);
        d.line(&[(right - 0.5, y), (right, y)]);
    }
    // The rails leave their ports downwards, along the frame.
    d.port("L1", left, 0.0, "down");
    d.port("L2", right, 0.0, "down");
}
