//! Pump and motor builder: ISO 1219 rotary energy converters.
//!
//! Provenance: own design, drawn from the written rules of ISO 1219-1. A circle with the flow
//! triangle on its rim: pointing out for a pump, in for a motor, one for one flow direction and
//! two for both, solid for hydraulic and hollow for pneumatic; a slanted arrow across for
//! variable displacement; a dashed drain line; a double line for the shaft, with a `shaft` port
//! at its end for the mechanical link to a prime mover.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ISO 1219-1";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "pump",
    label: "Pump or Motor",
    description: "An ISO 1219 pump or motor: one or two flow directions, fixed or variable displacement, an optional drain, hydraulic or pneumatic. Ports P (out) and S (suction) for a one-way pump, else A and B; L for the drain; shaft at the end of the shaft.",
    params: &[
        ParamSpec {
            key: "role",
            label: "Role",
            kind: ParamKind::Choice(&["pump", "motor"]),
            default: Def::Text("pump"),
            description: "A pump drives flow; a motor is driven by it.",
        },
        ParamSpec {
            key: "flow",
            label: "Flow",
            kind: ParamKind::Choice(&["uni", "bi"]),
            default: Def::Text("uni"),
            description: "One flow direction or both.",
        },
        ParamSpec { key: "variable", label: "Variable", kind: ParamKind::Bool, default: Def::Bool(false), description: "Variable displacement." },
        ParamSpec { key: "drain", label: "Drain", kind: ParamKind::Bool, default: Def::Bool(false), description: "A case drain port." },
        ParamSpec {
            key: "medium",
            label: "Medium",
            kind: ParamKind::Choice(&["hydraulic", "pneumatic"]),
            default: Def::Text("hydraulic"),
            description: "Solid triangles for hydraulic, hollow for pneumatic.",
        },
        ParamSpec {
            key: "tag",
            label: "Code",
            kind: ParamKind::Text { max: 32 },
            default: Def::Text(""),
            description: "The component code written beside the symbol (the element's tag).",
        },
    ],
    sources: &[("ISO 1219", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    d.standard("iso1219");
    d.size(8.0, 8.0);
    let (c, r) = (4.0, 3.0);
    d.circle(c, c, r);
    let solid = p.text("medium") != "pneumatic";
    let pump = p.text("role") != "motor";
    let bi = p.text("flow") == "bi";
    // A triangle on the rim: its tip on the rim pointing out for a pump, its base on the rim
    // pointing in for a motor.
    let triangle = |d: &mut Draw, top: bool| {
        let s = if top { -1.0 } else { 1.0 };
        let (rim, inner) = ((c, c + s * r), (c, c + s * (r - 1.1)));
        if pump {
            d.head(inner, rim, 1.1, 0.6, solid);
        } else {
            d.head(rim, inner, 1.1, 0.6, solid);
        }
    };
    triangle(d, true);
    if bi {
        triangle(d, false);
    }
    if p.flag("variable") {
        d.line(&[(1.6, 6.4), (6.4, 1.6)]);
        d.head((1.6, 6.4), (6.6, 1.4), 0.7, 0.3, true);
    }
    // The shaft leaves to the left.
    d.line(&[(c - r, 3.8), (0.0, 3.8)]);
    d.line(&[(c - r, 4.2), (0.0, 4.2)]);
    let (top, bottom) = if pump && !bi { ("P", "S") } else { ("A", "B") };
    d.line(&[(c, 0.0), (c, c - r)]);
    d.line(&[(c, 8.0), (c, c + r)]);
    d.port(top, c, 0.0, "up");
    d.port(bottom, c, 8.0, "down");
    d.label(&format!("p{top}"), top, c + 0.3, 0.5, 0.7, "start");
    d.label(&format!("p{bottom}"), bottom, c + 0.3, 7.5, 0.7, "start");
    if p.flag("drain") {
        d.dashed((c + r * 0.8, c + r * 0.6), (8.0, 6.0), 0.3, 0.2);
        d.port("L", 8.0, 6.0, "right");
        d.label("pL", "L", 7.7, 6.6, 0.7, "end");
    }
    d.port("shaft", 0.0, 4.0, "left");
    d.label("tag", p.text("tag"), 7.0, 1.0, 0.8, "start");
}
