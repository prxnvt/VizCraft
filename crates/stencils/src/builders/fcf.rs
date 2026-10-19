//! Feature control frame builder: a geometric tolerance with its datums.
//!
//! Provenance: own design, drawn from the written rules of ISO 1101 and ASME Y14.5. A row of
//! cells, the frame twice the text height: the characteristic symbol, the tolerance (with a
//! diameter sign and a circled modifier letter when given), then one cell per datum letter in
//! order of precedence. The fourteen characteristic symbols are drawn as their written
//! descriptions say (a line for straightness, a circle for circularity, two parallel slants for
//! parallelism, a circle with a cross for position and so on).

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params, text_width};

const SOURCE: &str = "own design, drawn from the written rules of ISO 1101 and ASME Y14.5";

const CHARACTERISTICS: &[&str] = &[
    "straightness",
    "flatness",
    "circularity",
    "cylindricity",
    "profileLine",
    "profileSurface",
    "angularity",
    "perpendicularity",
    "parallelism",
    "position",
    "concentricity",
    "symmetry",
    "circularRunout",
    "totalRunout",
];

const MODIFIERS: &[&str] = &["none", "mmc", "lmc", "projected", "freeState", "tangentPlane", "envelope"];

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "fcf",
    label: "Feature Control Frame",
    description: "A geometric tolerance frame (ISO 1101 or ASME Y14.5): the characteristic symbol, the tolerance with an optional diameter sign and modifier, and up to three datums separated by `|`. The leader attaches at the left.",
    params: &[
        ParamSpec {
            key: "characteristic",
            label: "Characteristic",
            kind: ParamKind::Choice(CHARACTERISTICS),
            default: Def::Text("position"),
            description: "The geometric characteristic toleranced.",
        },
        ParamSpec {
            key: "diameter",
            label: "Diameter",
            kind: ParamKind::Bool,
            default: Def::Bool(false),
            description: "A cylindrical tolerance zone.",
        },
        ParamSpec {
            key: "tolerance",
            label: "Tolerance",
            kind: ParamKind::Text { max: 16 },
            default: Def::Text("0.1"),
            description: "The tolerance value.",
        },
        ParamSpec {
            key: "modifier",
            label: "Modifier",
            kind: ParamKind::Choice(MODIFIERS),
            default: Def::Text("none"),
            description: "Material condition or zone modifier after the tolerance.",
        },
        ParamSpec {
            key: "datums",
            label: "Datums",
            kind: ParamKind::Text { max: 32 },
            default: Def::Text(""),
            description: "Up to three datum references, `|` between them (A|B|C).",
        },
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(&["iso1101", "asmeY14_5"]),
            default: Def::Text("iso1101"),
            description: "Which standard the frame follows.",
        },
    ],
    sources: &[("ISO 1101", SOURCE), ("ASME Y14.5", SOURCE)],
    build,
};

/// Text height in modules: the frame is four modules high, a little over twice the text.
const TEXT: f64 = 1.8;

/// Width in whole modules of a cell holding `chars` characters, at least four.
fn cell(chars: usize) -> f64 {
    (text_width(chars, TEXT) + 1.2).ceil().max(4.0)
}

fn build(p: &Params, d: &mut Draw) {
    let asme = p.text("standard") == "asmeY14_5";
    d.standard(if asme { "asmeY14_5" } else { "iso1101" });
    let ch = p.text("characteristic");
    if asme && matches!(ch, "concentricity" | "symmetry") {
        d.warn(format!("`characteristic`: {ch} is not in ASME Y14.5-2018"));
    }
    let tol = format!("{}{}", if p.flag("diameter") { "Ø" } else { "" }, p.text("tolerance"));
    let modifier = match p.text("modifier") {
        "mmc" => "M",
        "lmc" => "L",
        "projected" => "P",
        "freeState" => "F",
        "tangentPlane" => "T",
        "envelope" => "E",
        _ => "",
    };
    let raw = p.text("datums");
    let datums: Vec<&str> = raw.split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
    if datums.len() > 3 {
        d.warn("`datums`: at most three; the rest are left out");
    }
    let datums: Vec<&str> = datums.into_iter().take(3).collect();
    let tol_w = cell(tol.chars().count()) + if modifier.is_empty() { 0.0 } else { 2.0 };
    let mut widths = vec![4.0, tol_w];
    widths.extend(datums.iter().map(|t| cell(t.chars().count())));
    let w: f64 = widths.iter().sum();
    d.size(w, 4.0);
    d.rect(0.0, 0.0, w, 4.0);
    let mut x = 0.0;
    for cw in widths.iter().take(widths.len().saturating_sub(1)) {
        x += cw;
        d.line(&[(x, 0.0), (x, 4.0)]);
    }
    symbol(d, ch);
    let tx = 4.0 + 0.6;
    d.label("tolerance", &tol, tx, 2.0, TEXT, "start");
    if !modifier.is_empty() {
        let mx = 4.0 + tol_w - 1.4;
        d.ring(mx, 2.0, 0.8);
        d.label("modifier", modifier, mx, 2.0, 1.1, "middle");
    }
    let mut x = 4.0 + tol_w;
    for (i, (t, cw)) in datums.iter().zip(widths.iter().skip(2)).enumerate() {
        d.label(&format!("datum{}", i + 1), t, x + cw / 2.0, 2.0, TEXT, "middle");
        x += cw;
    }
    d.port("leader", 0.0, 2.0, "left");
}

/// The characteristic symbol in the first cell (0 to 4 across and down).
fn symbol(d: &mut Draw, ch: &str) {
    match ch {
        "straightness" => d.line(&[(1.0, 2.0), (3.0, 2.0)]),
        "flatness" => d.line(&[(0.9, 2.6), (1.6, 1.4), (3.1, 1.4), (2.4, 2.6), (0.9, 2.6)]),
        "circularity" => d.ring(2.0, 2.0, 0.9),
        "cylindricity" => {
            d.ring(2.0, 2.0, 0.7);
            d.line(&[(0.85, 2.8), (1.85, 1.1)]);
            d.line(&[(2.15, 2.9), (3.15, 1.2)]);
        }
        "profileLine" => d.arc(2.0, 2.6, 1.0, 180.0, 360.0),
        "profileSurface" => {
            d.arc(2.0, 2.6, 1.0, 180.0, 360.0);
            d.line(&[(1.0, 2.6), (3.0, 2.6)]);
        }
        "angularity" => d.line(&[(2.9, 1.2), (1.0, 2.8), (3.0, 2.8)]),
        "perpendicularity" => {
            d.line(&[(1.0, 2.8), (3.0, 2.8)]);
            d.line(&[(2.0, 2.8), (2.0, 1.0)]);
        }
        "parallelism" => {
            d.line(&[(1.0, 2.8), (1.8, 1.2)]);
            d.line(&[(2.0, 2.8), (2.8, 1.2)]);
        }
        "concentricity" => {
            d.ring(2.0, 2.0, 0.9);
            d.ring(2.0, 2.0, 0.45);
        }
        "symmetry" => {
            d.line(&[(1.3, 1.4), (2.7, 1.4)]);
            d.line(&[(0.8, 2.0), (3.2, 2.0)]);
            d.line(&[(1.3, 2.6), (2.7, 2.6)]);
        }
        "circularRunout" => {
            d.line(&[(1.4, 2.9), (2.4, 1.6)]);
            d.head((1.4, 2.9), (2.6, 1.3), 0.6, 0.25, true);
        }
        "totalRunout" => {
            d.line(&[(1.0, 2.9), (3.0, 2.9)]);
            d.line(&[(1.2, 2.9), (1.85, 1.6)]);
            d.head((1.2, 2.9), (2.0, 1.3), 0.6, 0.25, true);
            d.line(&[(2.0, 2.9), (2.65, 1.6)]);
            d.head((2.0, 2.9), (2.8, 1.3), 0.6, 0.25, true);
        }
        // Position, the default.
        _ => {
            d.ring(2.0, 2.0, 0.7);
            d.line(&[(0.9, 2.0), (3.1, 2.0)]);
            d.line(&[(2.0, 0.9), (2.0, 3.1)]);
        }
    }
}
