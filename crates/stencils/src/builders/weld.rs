//! Weld symbol builder: the arrow, reference line and elementary symbols of a welding callout.
//!
//! Provenance: own design, drawn from the written rules of ISO 2553 and AWS A2.4. An arrow line
//! from the joint to a horizontal reference line; ISO 2553 adds a dashed identification line
//! beside it and puts the arrow side symbol on the continuous line and the other side symbol on
//! the dashed one, while AWS A2.4 puts the arrow side below the line and the other side above.
//! A circle at the bend means all around, a flag a field weld, a fork at the far end the tail.
//! The elementary symbols are drawn as their written descriptions say (a right triangle for a
//! fillet, a V for a single V butt, two parallel lines for a square butt and so on).

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ISO 2553 and AWS A2.4";

const SYMBOLS: &[&str] =
    &["none", "fillet", "squareButt", "vButt", "bevelButt", "uButt", "jButt", "flareV", "flareBevel", "plug", "spot", "seam", "backing", "surfacing"];

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "weld",
    label: "Weld Symbol",
    description: "A welding symbol: arrow, reference line (with the ISO 2553 dashed identification line, or AWS A2.4 above and below), an elementary symbol for the arrow side and the other side, size and length, all around, field weld and a tail note. The arrow tip is the port.",
    params: &[
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(&["iso2553", "aws"]),
            default: Def::Text("iso2553"),
            description: "ISO 2553 (dashed identification line) or AWS A2.4.",
        },
        ParamSpec {
            key: "arrowSide",
            label: "Arrow Side",
            kind: ParamKind::Choice(SYMBOLS),
            default: Def::Text("fillet"),
            description: "The weld on the arrow side of the joint.",
        },
        ParamSpec {
            key: "otherSide",
            label: "Other Side",
            kind: ParamKind::Choice(SYMBOLS),
            default: Def::Text("none"),
            description: "The weld on the other side.",
        },
        ParamSpec {
            key: "size",
            label: "Size",
            kind: ParamKind::Text { max: 12 },
            default: Def::Text(""),
            description: "Weld size, left of the symbol (a5).",
        },
        ParamSpec {
            key: "length",
            label: "Length",
            kind: ParamKind::Text { max: 16 },
            default: Def::Text(""),
            description: "Length and pitch, right of the symbol.",
        },
        ParamSpec {
            key: "allAround",
            label: "All Around",
            kind: ParamKind::Bool,
            default: Def::Bool(false),
            description: "Weld all around the joint.",
        },
        ParamSpec { key: "field", label: "Field Weld", kind: ParamKind::Bool, default: Def::Bool(false), description: "Welded on site." },
        ParamSpec {
            key: "tail",
            label: "Tail",
            kind: ParamKind::Text { max: 32 },
            default: Def::Text(""),
            description: "Process or note in the tail.",
        },
    ],
    sources: &[("ISO 2553", SOURCE), ("AWS A2.4", SOURCE)],
    build,
};

/// The symbol centre across, the reference line height, and the text height.
const CX: f64 = 8.0;
const LINE_Y: f64 = 4.0;
const TEXT: f64 = 1.2;

/// Points of an arc in symbol coordinates (`u` across, `v` away from the line, angles counter
/// clockwise from `u`).
fn arc(cu: f64, cv: f64, r: f64, a0: f64, a1: f64) -> Vec<(f64, f64)> {
    (0..=12)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f64 / 12.0).to_radians();
            (cu + r * a.cos(), cv + r * a.sin())
        })
        .collect()
}

/// Draw elementary symbol `kind` with its base on the line at height `base`, growing up
/// (`dir` -1) or down (`dir` 1).
fn symbol(d: &mut Draw, kind: &str, base: f64, dir: f64) {
    let map = |pts: &[(f64, f64)]| -> Vec<(f64, f64)> { pts.iter().map(|(u, v)| (CX + u, base + dir * v)).collect() };
    let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
    match kind {
        "fillet" => lines.push(vec![(-0.8, 0.0), (-0.8, 1.6), (0.8, 0.0)]),
        "squareButt" => {
            lines.push(vec![(-0.4, 0.0), (-0.4, 1.4)]);
            lines.push(vec![(0.4, 0.0), (0.4, 1.4)]);
        }
        "vButt" => lines.push(vec![(-0.9, 1.5), (0.0, 0.0), (0.9, 1.5)]),
        "bevelButt" => {
            lines.push(vec![(-0.5, 0.0), (-0.5, 1.5)]);
            lines.push(vec![(-0.4, 0.0), (0.7, 1.5)]);
        }
        "uButt" => {
            lines.push(vec![(0.0, 0.0), (0.0, 0.5)]);
            let mut cup = vec![(-0.6, 1.6)];
            cup.extend(arc(0.0, 1.1, 0.6, 180.0, 360.0));
            cup.push((0.6, 1.6));
            lines.push(cup);
        }
        "jButt" => {
            lines.push(vec![(-0.6, 0.0), (-0.6, 1.6)]);
            let mut j = arc(-0.6, 1.1, 0.6, 270.0, 360.0);
            j.push((0.0, 1.6));
            lines.push(j);
        }
        "flareV" => {
            lines.push(arc(-1.2, 0.9, 0.8, -60.0, 60.0));
            lines.push(arc(1.2, 0.9, 0.8, 120.0, 240.0));
        }
        "flareBevel" => {
            lines.push(vec![(-0.5, 0.0), (-0.5, 1.6)]);
            lines.push(arc(0.9, 0.9, 0.8, 120.0, 240.0));
        }
        "plug" => lines.push(vec![(-0.7, 0.0), (-0.7, 0.8), (0.7, 0.8), (0.7, 0.0)]),
        "spot" => lines.push(arc(0.0, 0.6, 0.6, 0.0, 360.0)),
        "seam" => {
            lines.push(arc(0.0, 0.6, 0.6, 0.0, 360.0));
            lines.push(vec![(-1.0, 0.45), (1.0, 0.45)]);
            lines.push(vec![(-1.0, 0.75), (1.0, 0.75)]);
        }
        "backing" => lines.push(arc(0.0, 0.0, 0.7, 0.0, 180.0)),
        "surfacing" => {
            let mut s = arc(-0.4, 0.0, 0.4, 180.0, 0.0);
            s.extend(arc(0.4, 0.0, 0.4, 180.0, 0.0));
            lines.push(s);
        }
        _ => {}
    }
    for l in &lines {
        d.line(&map(l));
    }
}

fn build(p: &Params, d: &mut Draw) {
    let aws = p.text("standard") == "aws";
    d.standard(if aws { "aws" } else { "iso2553" });
    d.size(16.0, 8.0);
    // The arrow from the joint to the start of the reference line.
    d.line(&[(0.0, 8.0), (3.0, LINE_Y)]);
    d.head((3.0, LINE_Y), (0.0, 8.0), 0.9, 0.3, true);
    let tail = p.text("tail");
    d.line(&[(3.0, LINE_Y), (15.0, LINE_Y)]);
    if !aws {
        d.dashed((4.0, LINE_Y + 0.6), (14.0, LINE_Y + 0.6), 0.5, 0.3);
    }
    let (arrow_base, arrow_dir, other_base, other_dir) = if aws { (LINE_Y, 1.0, LINE_Y, -1.0) } else { (LINE_Y, -1.0, LINE_Y + 0.6, 1.0) };
    symbol(d, p.text("arrowSide"), arrow_base, arrow_dir);
    symbol(d, p.text("otherSide"), other_base, other_dir);
    let ty = arrow_base + arrow_dir * 0.8;
    d.label("size", p.text("size"), CX - 1.4, ty, TEXT, "end");
    d.label("length", p.text("length"), CX + 1.4, ty, TEXT, "start");
    if p.flag("allAround") {
        d.ring(3.0, LINE_Y, 0.5);
    }
    if p.flag("field") {
        d.line(&[(3.0, LINE_Y), (3.0, 1.6)]);
        d.shape(&[(3.0, 1.6), (4.4, 2.0), (3.0, 2.4)]);
    }
    if !tail.trim().is_empty() {
        d.line(&[(16.0, LINE_Y - 1.0), (15.0, LINE_Y), (16.0, LINE_Y + 1.0)]);
        d.label("tail", tail, 16.3, LINE_Y, TEXT, "start");
    }
    d.port("arrow", 0.0, 8.0, "down");
}
