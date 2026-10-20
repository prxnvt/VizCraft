//! Footprint builder: a device as it sits in a panel, a rectangle of its real width and height
//! with its tag, for panel layouts.
//!
//! Provenance: own design, drawn from the written rules of IEC 61082-1 for arrangement drawings, which show an item as its outline to scale with its reference designation. The mounting point
//! sits on the middle of the bottom edge, where a device clips onto a rail.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of IEC 61082-1 for arrangement drawings, which show an item as its outline to scale with its reference designation";

/// Millimetres per ISO module.
const MM_PER_MODULE: f64 = 2.5;

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "footprint",
    label: "Footprint",
    description: "A device drawn as its outline in a panel layout: width and height in millimetres, rounded up to whole 2.5 mm modules, with the tag in the middle. One port, `rail`, on the middle of the bottom edge.",
    params: &[
        ParamSpec {
            key: "width",
            label: "Width",
            kind: ParamKind::Number { min: 2.5, max: 2000.0 },
            default: Def::Num(45.0),
            description: "Width in millimetres.",
        },
        ParamSpec {
            key: "height",
            label: "Height",
            kind: ParamKind::Number { min: 2.5, max: 2000.0 },
            default: Def::Num(90.0),
            description: "Height in millimetres.",
        },
        ParamSpec { key: "tag", label: "Tag", kind: ParamKind::Text { max: 16 }, default: Def::Text(""), description: "Device tag (K1)." },
    ],
    sources: &[("IEC 61082-1", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    d.standard("iec60617");
    let modules = |mm: f64| (mm / MM_PER_MODULE).ceil().clamp(2.0, 800.0);
    let (w, h) = (modules(p.num("width")), modules(p.num("height")));
    d.size(w, h);
    d.rect(0.0, 0.0, w, h);
    let mid = (w / 2.0).floor();
    d.port("rail", mid, h, "down");
    d.label("tag", p.text("tag"), w / 2.0, h / 2.0, (w / 4.0).clamp(0.8, 2.0), "middle");
}
