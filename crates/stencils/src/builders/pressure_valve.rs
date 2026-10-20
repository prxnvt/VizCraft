//! Pressure valve builder: ISO 1219 pressure relief, reducing and sequence valves.
//!
//! Provenance: own design, drawn from the written rules of ISO 1219-1. One square envelope with
//! the inlet P at the bottom and the outlet (T for a relief valve, A otherwise) at the top; the
//! flow arrow sits off the port line in a normally closed valve (relief, sequence) and on it in a
//! normally open one (reducing); a zigzag spring at the right side, crossed by a slanted arrow
//! when adjustable; the pilot as a dashed line to the left side, from the inlet (relief,
//! sequence) or the outlet (reducing), or from an external port X; an external drain L as a
//! dashed line from the spring side.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ISO 1219-1";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "pressureValve",
    label: "Pressure Valve",
    description: "An ISO 1219 pressure control valve: relief (P to T, normally closed), reducing (P to A, normally open, piloted from the outlet) or sequence (P to A, normally closed, with a drain); internal or external pilot (port X) and drain (port L); a spring, adjustable or set. Ports P, then T or A, then L, then X.",
    params: &[
        ParamSpec {
            key: "function",
            label: "Function",
            kind: ParamKind::Choice(&["relief", "reducing", "sequence"]),
            default: Def::Text("relief"),
            description: "Relief limits the inlet pressure, reducing holds the outlet pressure, sequence opens at a set pressure.",
        },
        ParamSpec {
            key: "pilot",
            label: "Pilot",
            kind: ParamKind::Choice(&["internal", "external"]),
            default: Def::Text("internal"),
            description: "Where the pilot pressure comes from: inside the valve, or port X.",
        },
        ParamSpec {
            key: "drain",
            label: "Drain",
            kind: ParamKind::Choice(&["internal", "external"]),
            default: Def::Text("internal"),
            description: "Where the spring chamber drains: inside, or port L (a sequence valve always has L).",
        },
        ParamSpec {
            key: "adjustable",
            label: "Adjustable",
            kind: ParamKind::Bool,
            default: Def::Bool(true),
            description: "An arrow across the spring.",
        },
        ParamSpec {
            key: "medium",
            label: "Medium",
            kind: ParamKind::Choice(&["hydraulic", "pneumatic"]),
            default: Def::Text("hydraulic"),
            description: "The fluid; the circuit checks read it.",
        },
        ParamSpec {
            key: "tag",
            label: "Code",
            kind: ParamKind::Text { max: 32 },
            default: Def::Text(""),
            description: "The component code written above the valve (the element's tag).",
        },
    ],
    sources: &[("ISO 1219", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    d.standard("iso1219");
    d.size(8.0, 6.0);
    let function = p.text("function");
    let relief = function == "relief";
    let open = function == "reducing";
    d.rect(2.0, 1.0, 6.0, 5.0);
    // The flow arrow: on the port line when normally open, beside it when closed.
    let ax = if open { 4.0 } else { 3.2 };
    if open {
        d.line(&[(ax, 5.0), (ax, 1.6)]);
    } else {
        d.line(&[(ax, 4.4), (ax, 1.6)]);
    }
    d.head((ax, 2.4), (ax, 1.6), 0.6, 0.25, true);
    let out = if relief { "T" } else { "A" };
    for (id, y0, y1, dir) in [("P", 5.0, 6.0, "down"), (out, 1.0, 0.0, "up")] {
        d.line(&[(4.0, y0), (4.0, y1)]);
        d.port(id, 4.0, y1, dir);
        d.label(&format!("p{id}"), id, 4.3, (y0 + y1) / 2.0, 0.7, "start");
    }
    // The spring at the right side.
    let mut pts = vec![(6.0, 3.0)];
    for k in 1..=6 {
        pts.push((6.0 + 0.25 * k as f64, if k % 2 == 1 { 2.6 } else { 3.4 }));
    }
    pts.push((7.6, 3.0));
    d.line(&pts);
    if p.flag("adjustable") {
        d.line(&[(6.1, 4.1), (7.5, 1.9)]);
        d.head((6.1, 4.1), (7.6, 1.75), 0.5, 0.2, true);
    }
    let external_drain = p.text("drain") == "external" || function == "sequence";
    if external_drain {
        d.dashed((6.0, 4.0), (8.0, 4.0), 0.3, 0.2);
        d.port("L", 8.0, 4.0, "right");
        d.label("pL", "L", 7.9, 4.6, 0.7, "end");
    }
    if p.text("pilot") == "external" {
        d.dashed((0.0, 3.0), (1.4, 3.0), 0.3, 0.2);
        d.head((0.8, 3.0), (2.0, 3.0), 0.6, 0.3, true);
        d.port("X", 0.0, 3.0, "left");
        d.label("pX", "X", 0.1, 2.3, 0.7, "start");
    } else {
        // From the inlet (relief, sequence) or the outlet (reducing) round to the left side.
        let y = if open { 0.5 } else { 5.5 };
        d.dashed((4.0, y), (1.0, y), 0.3, 0.2);
        d.dashed((1.0, y), (1.0, 3.0), 0.3, 0.2);
        d.head((1.0, 3.0), (2.0, 3.0), 0.6, 0.3, true);
    }
    d.label("tag", p.text("tag"), 0.0, -0.6, 0.8, "start");
}
