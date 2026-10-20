//! Block builder: a box with pins down its left and right sides, for PLC modules, integrated
//! circuits and other devices drawn as a box of terminals.
//!
//! Provenance: own design, drawn from the written rules of IEC 61082-1 and IEEE 315 for devices shown as a box with their terminals. A
//! rectangle with a lead out to each pin, the pin's name inside the box beside it, the device's
//! function written at the top.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of IEC 61082-1 and IEEE 315 for devices shown as a box with their terminals";

/// Most pins on one side.
const MAX_PINS: i64 = 32;

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "block",
    label: "Block",
    description: "A box with up to 32 pins a side, for a PLC input or output module or an integrated circuit. Pins are named from `leftNames` and `rightNames` (comma separated), else L1, L2 and R1, R2; they are the port ids, top to bottom, left side first.",
    params: &[
        ParamSpec {
            key: "left",
            label: "Left pins",
            kind: ParamKind::Int { min: 0, max: MAX_PINS },
            default: Def::Int(2),
            description: "Pins on the left.",
        },
        ParamSpec {
            key: "right",
            label: "Right pins",
            kind: ParamKind::Int { min: 0, max: MAX_PINS },
            default: Def::Int(2),
            description: "Pins on the right.",
        },
        ParamSpec {
            key: "leftNames",
            label: "Left pin names",
            kind: ParamKind::Text { max: 512 },
            default: Def::Text(""),
            description: "Names of the left pins, comma separated (I0, I1).",
        },
        ParamSpec {
            key: "rightNames",
            label: "Right pin names",
            kind: ParamKind::Text { max: 512 },
            default: Def::Text(""),
            description: "Names of the right pins, comma separated.",
        },
        ParamSpec {
            key: "title",
            label: "Title",
            kind: ParamKind::Text { max: 32 },
            default: Def::Text(""),
            description: "The function written at the top (DI 8).",
        },
        ParamSpec { key: "tag", label: "Tag", kind: ParamKind::Text { max: 16 }, default: Def::Text(""), description: "Device tag (A1, U1)." },
    ],
    sources: &[("IEC 61082-1", SOURCE), ("IEEE 315", SOURCE)],
    build,
};

/// The pin ids of one side: the given names where usable, else `<side><n>`, never repeating an
/// id already in `taken`.
fn names(given: &str, count: usize, side: &str, taken: &mut Vec<String>) -> Vec<String> {
    let mut given = given.split(',').map(str::trim);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let want: String = given.next().unwrap_or("").chars().take(16).collect();
        let mut id = if want.is_empty() || taken.contains(&want) { format!("{side}{}", i + 1) } else { want };
        let mut k = 2;
        while taken.contains(&id) && k < 200 {
            id = format!("{side}{}_{k}", i + 1);
            k += 1;
        }
        taken.push(id.clone());
        out.push(id);
    }
    out
}

fn build(p: &Params, d: &mut Draw) {
    d.standard("iec60617");
    let left = p.int("left").clamp(0, MAX_PINS) as usize;
    let right = p.int("right").clamp(0, MAX_PINS) as usize;
    let rows = left.max(right).max(1) as f64;
    let (w, h) = (8.0, 2.0 * rows + 2.0);
    d.size(w, h);
    d.rect(1.0, 0.0, 7.0, h);
    let mut taken = Vec::new();
    let lnames = names(p.text("leftNames"), left, "L", &mut taken);
    let rnames = names(p.text("rightNames"), right, "R", &mut taken);
    for (i, id) in lnames.iter().enumerate() {
        let y = 2.0 + 2.0 * i as f64;
        d.line(&[(0.0, y), (1.0, y)]);
        d.port(id, 0.0, y, "left");
        d.label(&format!("pl{i}"), id, 1.3, y, 0.7, "start");
    }
    for (i, id) in rnames.iter().enumerate() {
        let y = 2.0 + 2.0 * i as f64;
        d.line(&[(7.0, y), (8.0, y)]);
        d.port(id, 8.0, y, "right");
        d.label(&format!("pr{i}"), id, 6.7, y, 0.7, "end");
    }
    if left == 0 && right == 0 {
        // A box with no pins still needs a place to connect.
        d.port("1", 4.0, h, "down");
    }
    d.label("title", p.text("title"), 4.0, 0.9, 0.8, "middle");
    d.label("tag", p.text("tag"), 4.0, -0.7, 1.0, "middle");
}
