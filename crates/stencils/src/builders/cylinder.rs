//! Cylinder builder: ISO 1219 linear actuators.
//!
//! Provenance: own design, drawn from the written rules of ISO 1219-1. The barrel is a rectangle
//! with the piston a double line across it and the rod a pair of lines leaving one end, or both
//! for a through rod; a single acting cylinder has one port and may hold a return spring in the
//! rod chamber; cushioning is a small box at the end it acts on.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ISO 1219-1";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "cylinder",
    label: "Cylinder",
    description: "An ISO 1219 linear actuator: single or double acting, single or through rod, cushioning at either end, an optional return spring. Ports A (cap end) and B (rod end) on the underside.",
    params: &[
        ParamSpec {
            key: "acting",
            label: "Acting",
            kind: ParamKind::Choice(&["double", "single"]),
            default: Def::Text("double"),
            description: "Pressure on both sides of the piston, or one.",
        },
        ParamSpec {
            key: "rod",
            label: "Rod",
            kind: ParamKind::Choice(&["single", "through"]),
            default: Def::Text("single"),
            description: "A rod at one end or through both.",
        },
        ParamSpec {
            key: "cushion",
            label: "Cushion",
            kind: ParamKind::Choice(&["none", "a", "b", "both"]),
            default: Def::Text("none"),
            description: "End cushioning at the cap end (a), the rod end (b) or both.",
        },
        ParamSpec {
            key: "springReturn",
            label: "Spring Return",
            kind: ParamKind::Bool,
            default: Def::Bool(false),
            description: "A spring in the rod chamber returns the piston.",
        },
        ParamSpec {
            key: "medium",
            label: "Medium",
            kind: ParamKind::Choice(&["hydraulic", "pneumatic"]),
            default: Def::Text("hydraulic"),
            description: "A pneumatic single acting cylinder vents its rod chamber.",
        },
    ],
    sources: &[("ISO 1219", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    d.standard("iso1219");
    let through = p.text("rod") == "through";
    let single = p.text("acting") == "single";
    // The barrel is eight modules long; a rod reaches three modules out of each end it leaves.
    let x0 = if through { 3.0 } else { 0.0 };
    let x1 = x0 + 8.0;
    let w = x1 + 3.0;
    d.size(w, 6.0);
    d.rect(x0, 1.0, x1, 5.0);
    let px = x0 + 3.0;
    d.line(&[(px - 0.2, 1.0), (px - 0.2, 5.0)]);
    d.line(&[(px + 0.2, 1.0), (px + 0.2, 5.0)]);
    d.line(&[(px + 0.2, 2.7), (w, 2.7), (w, 3.3), (px + 0.2, 3.3)]);
    if through {
        d.line(&[(px - 0.2, 2.7), (0.0, 2.7), (0.0, 3.3), (px - 0.2, 3.3)]);
    }
    if p.flag("springReturn") {
        let mut pts = Vec::new();
        let (a, b) = (px + 0.5, x1 - 0.3);
        for k in 0..=8 {
            let y = if k % 2 == 0 { 1.4 } else { 4.6 };
            pts.push((a + (b - a) * k as f64 / 8.0, y));
        }
        d.line(&pts);
    }
    let cushion = p.text("cushion");
    if matches!(cushion, "a" | "both") {
        d.line(&[(x0, 2.4), (x0 + 0.9, 2.4), (x0 + 0.9, 3.6), (x0, 3.6)]);
    }
    if matches!(cushion, "b" | "both") {
        d.line(&[(x1, 2.2), (x1 - 0.9, 2.2), (x1 - 0.9, 3.8), (x1, 3.8)]);
    }
    let (ax, bx) = (x0 + 1.0, x1 - 1.0);
    d.line(&[(ax, 5.0), (ax, 6.0)]);
    d.port("A", ax, 6.0, "down");
    d.label("pA", "A", ax + 0.3, 5.5, 0.7, "start");
    if single {
        if p.text("medium") == "pneumatic" {
            // The rod chamber breathes to the air.
            d.line(&[(bx, 5.0), (bx, 5.4)]);
            d.head((bx, 5.4), (bx, 6.0), 0.5, 0.3, false);
        }
    } else {
        d.line(&[(bx, 5.0), (bx, 6.0)]);
        d.port("B", bx, 6.0, "down");
        d.label("pB", "B", bx + 0.3, 5.5, 0.7, "start");
    }
}
