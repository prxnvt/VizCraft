//! IDEF0 box builder: an activity box with its ICOM ports on the four sides.
//!
//! Provenance: own design, drawn from the written box and arrow syntax rules of FIPS 183. A
//! rectangle with square corners; inputs enter the left side, controls the top, outputs leave the
//! right side and mechanisms join the bottom, each side's ports spread evenly along it; the box
//! number sits inside the lower right corner, and an optional call port leaves the bottom at its
//! right end, after the mechanisms.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written box and arrow syntax rules of FIPS 183";

/// Most ports on one side (FIPS 183 diagrams stay readable well below this).
const MAX_SIDE: i64 = 6;

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "idef0box",
    label: "IDEF0 Box",
    description: "An IDEF0 activity box (FIPS 183): input ports I1.. on the left, controls C1.. on the top, outputs O1.. on the right and mechanisms M1.. on the bottom, each count from 0 to 6, the box number in the lower right corner and an optional call port at the bottom right.",
    params: &[
        ParamSpec {
            key: "inputs",
            label: "Inputs",
            kind: ParamKind::Int { min: 0, max: MAX_SIDE },
            default: Def::Int(1),
            description: "Input ports on the left side.",
        },
        ParamSpec {
            key: "controls",
            label: "Controls",
            kind: ParamKind::Int { min: 0, max: MAX_SIDE },
            default: Def::Int(1),
            description: "Control ports on the top side.",
        },
        ParamSpec {
            key: "outputs",
            label: "Outputs",
            kind: ParamKind::Int { min: 0, max: MAX_SIDE },
            default: Def::Int(1),
            description: "Output ports on the right side.",
        },
        ParamSpec {
            key: "mechanisms",
            label: "Mechanisms",
            kind: ParamKind::Int { min: 0, max: MAX_SIDE },
            default: Def::Int(1),
            description: "Mechanism ports on the bottom side.",
        },
        ParamSpec {
            key: "number",
            label: "Box Number",
            kind: ParamKind::Text { max: 8 },
            default: Def::Text(""),
            description: "The box number, 0 on a context diagram, 1 to 6 below it.",
        },
        ParamSpec {
            key: "call",
            label: "Call Port",
            kind: ParamKind::Bool,
            default: Def::Bool(false),
            description: "A call arrow leaves the bottom at the right.",
        },
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(&["fips183"]),
            default: Def::Text("fips183"),
            description: "The notation the box follows.",
        },
    ],
    sources: &[("FIPS 183", SOURCE)],
    build,
};

/// Box number text height in modules.
const TEXT: f64 = 1.6;

/// Where the `k`th of `n` ports sits along a side `len` modules long: evenly spread, on a whole
/// module. Sides are at least three modules per port, so neighbours never round together.
fn along(len: f64, k: usize, n: usize) -> f64 {
    (len * (k as f64 + 1.0) / (n as f64 + 1.0)).round()
}

fn count(p: &Params, key: &str) -> usize {
    usize::try_from(p.int(key).clamp(0, MAX_SIDE)).unwrap_or(0)
}

fn build(p: &Params, d: &mut Draw) {
    d.standard("fips183");
    let (i, c, o, m) = (count(p, "inputs"), count(p, "controls"), count(p, "outputs"), count(p, "mechanisms"));
    let call = p.flag("call");
    let bottom = m + usize::from(call);
    let w = (4.0 * (c.max(bottom) as f64 + 1.0)).max(24.0);
    let h = (3.0 * (i.max(o) as f64 + 1.0)).max(14.0);
    d.size(w, h);
    d.rect(0.0, 0.0, w, h);
    let number = p.text("number");
    d.label("number", number, w - 1.0, h - 1.4, TEXT, "end");
    for k in 0..i {
        d.port(&format!("I{}", k + 1), 0.0, along(h, k, i), "left");
    }
    for k in 0..c {
        d.port(&format!("C{}", k + 1), along(w, k, c), 0.0, "up");
    }
    for k in 0..o {
        d.port(&format!("O{}", k + 1), w, along(h, k, o), "right");
    }
    for k in 0..m {
        d.port(&format!("M{}", k + 1), along(w, k, bottom), h, "down");
    }
    if call {
        d.port("call", along(w, m, bottom), h, "down");
    }
}
