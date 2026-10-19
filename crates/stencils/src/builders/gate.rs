//! Logic gate builder: rectangular (IEC 60617-12) and distinctive shape (IEEE 91) gates.
//!
//! Provenance: own design, drawn from the written rules of IEC 60617-12 and IEEE 91. Rectangular:
//! a box with the function qualifier at its top (`&`, `≥1`, `=1`, `1`) and a negation circle at
//! an inverted input or output. Distinctive: a flat back and round front for AND, a curved back
//! and pointed front for OR, a second back curve for XOR, a triangle for NOT, the negation circle
//! at inverted pins; a back line extends past the body when the inputs need more room.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of IEC 60617-12 and IEEE 91";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "gate",
    label: "Logic Gate",
    description: "A logic gate: and, or, not, nand, nor or xor with 1 to 16 inputs, inverted inputs from a mask (`01` inverts the second), drawn rectangular (IEC 60617-12) or in distinctive shapes (IEEE 91). Inputs A to P on the left, output Y on the right.",
    params: &[
        ParamSpec {
            key: "function",
            label: "Function",
            kind: ParamKind::Choice(&["and", "or", "not", "nand", "nor", "xor"]),
            default: Def::Text("and"),
            description: "The logic function.",
        },
        ParamSpec {
            key: "inputs",
            label: "Inputs",
            kind: ParamKind::Int { min: 1, max: 16 },
            default: Def::Int(2),
            description: "Input count (not has one).",
        },
        ParamSpec {
            key: "invert",
            label: "Inverted Inputs",
            kind: ParamKind::Text { max: 16 },
            default: Def::Text(""),
            description: "One character per input, `1` inverts it (`01`).",
        },
        ParamSpec {
            key: "shapes",
            label: "Shapes",
            kind: ParamKind::Choice(&["rectangular", "distinctive"]),
            default: Def::Text("rectangular"),
            description: "IEC 60617-12 boxes or IEEE 91 distinctive shapes.",
        },
    ],
    sources: &[("IEC 60617-12", SOURCE), ("IEEE 91", SOURCE)],
    build,
};

const NAMES: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P"];

/// Points of the quadratic from `a` through control `c` to `b`, `n` steps, `a` excluded.
fn quad(a: (f64, f64), c: (f64, f64), b: (f64, f64), n: usize) -> Vec<(f64, f64)> {
    (1..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            (u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1)
        })
        .collect()
}

fn build(p: &Params, d: &mut Draw) {
    let f = p.text("function");
    let distinctive = p.text("shapes") == "distinctive";
    d.standard(if distinctive { "ieee91" } else { "iec60617" });
    let mut n = p.int("inputs").clamp(1, 16);
    if f == "not" && n != 1 {
        d.warn("`inputs`: a not gate has one input");
        n = 1;
    } else if f != "not" && n < 2 {
        d.warn("`inputs`: this gate needs at least two inputs");
        n = 2;
    }
    let h = (2 * n).max(4) as f64;
    d.size(7.0, h);
    let cy = h / 2.0;
    let ys: Vec<f64> = (0..n).map(|i| cy - (n - 1) as f64 + 2.0 * i as f64).collect();
    let inverted_out = matches!(f, "not" | "nand" | "nor");
    let mask: Vec<bool> = p.text("invert").chars().map(|c| c == '1').collect();
    let base = match f {
        "nand" => "and",
        "nor" => "or",
        other => other,
    };
    // Where each input lead ends (the body's back) and where the output leaves.
    let mut ends: Vec<f64> = vec![1.0; ys.len()];
    let tip;
    if !distinctive {
        d.rect(1.0, 0.5, 5.0, h - 0.5);
        let q = match base {
            "and" => "&",
            "or" => "≥1",
            "xor" => "=1",
            _ => "1",
        };
        d.label("qualifier", q, 3.0, 1.4, 1.0, "middle");
        tip = 5.0;
    } else {
        let bh = (h / 2.0 - 0.5).min(2.5);
        let (top, bot) = (cy - bh, cy + bh);
        let back = |y: f64, x0: f64| x0 + 0.6 * (1.0 - ((y - cy) / bh).powi(2)).max(0.0);
        match base {
            "not" => {
                d.shape(&[(1.0, top), (5.3, cy), (1.0, bot)]);
                tip = 5.3;
            }
            "and" => {
                let xs = 6.0 - bh;
                let mut pts = vec![(xs, bot), (1.0, bot), (1.0, top), (xs, top)];
                pts.extend((1..16).map(|i| {
                    let a = (-90.0 + 180.0 * i as f64 / 16.0).to_radians();
                    (xs + bh * a.cos(), cy + bh * a.sin())
                }));
                d.shape(&pts);
                tip = 6.0;
            }
            _ => {
                let x0 = if base == "xor" { 1.4 } else { 1.0 };
                let mut pts = vec![(x0, top)];
                pts.extend(quad((x0, top), (4.5, top), (6.0, cy), 10));
                pts.extend(quad((6.0, cy), (4.5, bot), (x0, bot), 10));
                pts.extend((1..12).map(|i| {
                    let y = bot - 2.0 * bh * i as f64 / 12.0;
                    (back(y, x0), y)
                }));
                d.shape(&pts);
                if base == "xor" {
                    let curve: Vec<(f64, f64)> = (0..=12).map(|i| top + 2.0 * bh * i as f64 / 12.0).map(|y| (back(y, 1.0), y)).collect();
                    d.line(&curve);
                }
                for (e, y) in ends.iter_mut().zip(&ys) {
                    if (top..=bot).contains(y) {
                        *e = back(*y, 1.0);
                    }
                }
                tip = 6.0;
            }
        }
        // The back line reaches inputs beyond the body.
        if let (Some(first), Some(last)) = (ys.first().copied(), ys.last().copied()) {
            if first < top {
                d.line(&[(1.0, first), (1.0, top)]);
            }
            if last > bot {
                d.line(&[(1.0, bot), (1.0, last)]);
            }
        }
    }
    for (i, (y, end)) in ys.iter().zip(&ends).enumerate() {
        let name = NAMES.get(i).copied().unwrap_or("X");
        if mask.get(i).copied().unwrap_or(false) {
            d.circle(end - 0.3, *y, 0.3);
            d.line(&[(0.0, *y), (end - 0.6, *y)]);
        } else {
            d.line(&[(0.0, *y), (*end, *y)]);
        }
        d.port(name, 0.0, *y, "left");
    }
    if inverted_out {
        d.circle(tip + 0.35, cy, 0.35);
        d.line(&[(tip + 0.7, cy), (7.0, cy)]);
    } else {
        d.line(&[(tip, cy), (7.0, cy)]);
    }
    d.port("Y", 7.0, cy, "right");
}
