//! Surface texture builder: the surface texture (roughness) symbol and its requirements.
//!
//! Provenance: own design, drawn from the written rules of ISO 21920-1 and ASME Y14.36. Two legs
//! of unequal length at sixty degrees to the surface, meeting at it; a bar across closes the
//! short leg when material removal is required, a circle in the V when it is prohibited; a line
//! along the top of the long leg carries the requirements, the lay symbol sits beside the V.
//! ISO writes the parameter and value under that line and the process over it; ASME writes the
//! roughness value in the V and the process over the line.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params, text_width};

const SOURCE: &str = "own design, drawn from the written rules of ISO 21920-1 and ASME Y14.36";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "texture",
    label: "Surface Texture",
    description: "A surface texture symbol (ISO 21920-1 or ASME Y14.36): material removal any, required or prohibited, the lay direction, the roughness requirement and the production process. The vertex touches the surface and is the port.",
    params: &[
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(&["iso21920", "asmeY14_36"]),
            default: Def::Text("iso21920"),
            description: "Where the requirement text goes.",
        },
        ParamSpec {
            key: "removal",
            label: "Material Removal",
            kind: ParamKind::Choice(&["any", "required", "prohibited"]),
            default: Def::Text("any"),
            description: "Whether machining is required, prohibited or left open.",
        },
        ParamSpec {
            key: "lay",
            label: "Lay",
            kind: ParamKind::Choice(&["none", "parallel", "perpendicular", "crossed", "multi", "circular", "radial", "particulate"]),
            default: Def::Text("none"),
            description: "Direction of the surface pattern.",
        },
        ParamSpec {
            key: "value",
            label: "Value",
            kind: ParamKind::Text { max: 24 },
            default: Def::Text("Ra 3.2"),
            description: "Roughness requirement.",
        },
        ParamSpec {
            key: "process",
            label: "Process",
            kind: ParamKind::Text { max: 32 },
            default: Def::Text(""),
            description: "Production process (ground).",
        },
    ],
    sources: &[("ISO 21920-1", SOURCE), ("ASME Y14.36", SOURCE)],
    build,
};

const TEXT: f64 = 1.0;

fn build(p: &Params, d: &mut Draw) {
    let asme = p.text("standard") == "asmeY14_36";
    d.standard(if asme { "asmeY14_36" } else { "iso21920" });
    let t60 = 3f64.sqrt();
    // The vertex on the surface at (2, 6); the short leg rises two modules, the long one five.
    let vertex = (2.0, 6.0);
    let short = (2.0 - 2.0 / t60, 4.0);
    let long = (2.0 + 5.0 / t60, 1.0);
    let value = p.text("value");
    let process = p.text("process");
    let iso_text = !asme && (!value.trim().is_empty() || !process.trim().is_empty());
    let ext = if iso_text || !process.trim().is_empty() {
        let chars = value.chars().count().max(process.chars().count());
        text_width(chars, TEXT) + 0.8
    } else {
        0.0
    };
    let w = (long.0 + ext).ceil().max(5.0);
    d.size(w, 6.0);
    d.line(&[short, vertex, long]);
    if ext > 0.0 {
        d.line(&[long, (long.0 + ext, long.1)]);
    }
    match p.text("removal") {
        "required" => d.line(&[short, (2.0 + 2.0 / t60, 4.0)]),
        "prohibited" => d.ring(2.0, 4.8, 0.6),
        _ => {}
    }
    let lay = match p.text("lay") {
        "parallel" => "=",
        "crossed" => "X",
        "multi" => "M",
        "circular" => "C",
        "radial" => "R",
        "particulate" => "P",
        _ => "",
    };
    if p.text("lay") == "perpendicular" {
        d.line(&[(3.8, 5.4), (4.6, 5.4)]);
        d.line(&[(4.2, 5.4), (4.2, 4.6)]);
    } else {
        d.label("lay", lay, 4.2, 5.0, TEXT, "middle");
    }
    let tx = long.0 + 0.3;
    if asme {
        d.label("value", value, 2.7, 3.2, TEXT, "end");
    } else {
        d.label("value", value, tx, 1.8, TEXT, "start");
    }
    d.label("process", process, tx, 0.3, TEXT, "start");
    d.port("surface", vertex.0, vertex.1, "down");
}
