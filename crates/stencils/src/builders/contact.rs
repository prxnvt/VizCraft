//! Contact builder: make, break and changeover contacts, one to six poles, plain or time
//! delayed, with an optional operator (push, pull, turn, emergency, roller, key, a process
//! quantity or proximity).
//!
//! Provenance: own design, drawn from the written rules of IEC 60617-7 and of the JIC and IEEE 315
//! ladder convention. IEC: a contact drawn upright with a blade swinging from the lower terminal,
//! a break contact touching a short tick on the upper one, ganged poles joined by a dashed
//! linkage, a delay shown by an arc on the operating stem. JIC and IEEE 315: two short parallel
//! plates across the line, a break contact crossed by a slash. Terminal numbers follow the IEC 61346 and EN 50005
//! practice: a sequence digit per pole, then 3 and 4 for a make contact, 1 and 2 for a break, 1, 2
//! and 4 for a changeover. Operators are the qualifier strokes the IEC 60617-7 text describes,
//! drawn at the end of a dashed operating stem: a bracket for push and pull, a crank for turn, a
//! mushroom head for emergency stop, a small circle for a roller, a lozenge for proximity and a
//! lettered square for a process quantity.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const STANDARDS: &[&str] = &["iec60617", "jic", "ieee315"];

/// The operators a contact may carry.
const ACTUATORS: &[&str] = &["none", "push", "pull", "turn", "emergency", "roller", "pressure", "level", "flow", "temperature", "proximity", "key"];

const SOURCE: &str = "own design, drawn from the written rules of IEC 60617-7 and of the JIC and IEEE 315 ladder convention";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "contact",
    label: "Contact",
    description: "A relay or contactor contact, or a switch: make (no), break (nc) or changeover (co), plain or on or off delayed, one to six ganged poles, with an optional operator (pushbutton, selector, limit or process switch). IEC 60617 draws it upright with a swinging blade; JIC and IEEE 315 draw it across a ladder rung as two plates. Ports per pole: 13 and 14 for a make contact, 11 and 12 for a break, 11, 12 and 14 for a changeover, the first digit counting the poles.",
    params: &[
        ParamSpec {
            key: "form",
            label: "Form",
            kind: ParamKind::Choice(&["no", "nc", "co"]),
            default: Def::Text("no"),
            description: "Make (normally open), break (normally closed) or changeover.",
        },
        ParamSpec {
            key: "timing",
            label: "Timing",
            kind: ParamKind::Choice(&["none", "onDelay", "offDelay"]),
            default: Def::Text("none"),
            description: "Delayed when the operating device is energised (onDelay) or released (offDelay).",
        },
        ParamSpec {
            key: "actuator",
            label: "Operator",
            kind: ParamKind::Choice(ACTUATORS),
            default: Def::Text("none"),
            description: "How the contact is worked by hand or by the process: none (a relay contact), push, pull, turn, emergency, roller, pressure, level, flow, temperature, proximity or key.",
        },
        ParamSpec { key: "poles", label: "Poles", kind: ParamKind::Int { min: 1, max: 6 }, default: Def::Int(1), description: "Ganged poles." },
        ParamSpec {
            key: "direction",
            label: "Direction",
            kind: ParamKind::Choice(&["auto", "vertical", "horizontal"]),
            default: Def::Text("auto"),
            description: "Which way the symbol runs between its terminals: auto (upright for IEC, across a rung for JIC and IEEE 315), vertical or horizontal.",
        },
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(STANDARDS),
            default: Def::Text("iec60617"),
            description: "Which symbol set draws it.",
        },
        ParamSpec {
            key: "tag",
            label: "Tag",
            kind: ParamKind::Text { max: 16 },
            default: Def::Text(""),
            description: "Device tag shown beside the symbol (K1).",
        },
    ],
    sources: &[("IEC 60617", SOURCE), ("JIC", SOURCE), ("IEEE 315", SOURCE)],
    build,
};

fn standard(p: &Params) -> &'static str {
    STANDARDS.iter().copied().find(|s| *s == p.text("standard")).unwrap_or("iec60617")
}

/// Draw operator `act` at `(x, y)`, where the operating stem ends: the stem arrives from the
/// right (IEC, upright contacts) or, with `vertical`, from below (contacts across a rung).
fn operator(d: &mut Draw, act: &str, x: f64, y: f64, vertical: bool) {
    // Points are written for a stem arriving from the right; `vertical` turns them a quarter so
    // the stem arrives from below.
    let at = |dx: f64, dy: f64| if vertical { (x + dy, y + dx) } else { (x + dx, y + dy) };
    match act {
        "push" => d.line(&[at(0.3, -0.5), at(0.0, -0.5), at(0.0, 0.5), at(0.3, 0.5)]),
        "pull" => d.line(&[at(-0.3, -0.5), at(0.0, -0.5), at(0.0, 0.5), at(-0.3, 0.5)]),
        "turn" => d.line(&[at(0.3, -0.5), at(0.0, -0.5), at(0.0, 0.5), at(-0.3, 0.5)]),
        "emergency" => {
            d.line(&[at(0.0, -0.5), at(0.0, 0.5)]);
            let (cx, cy) = at(0.0, 0.0);
            let (a0, a1) = if vertical { (180.0, 360.0) } else { (90.0, 270.0) };
            d.arc(cx, cy, 0.5, a0, a1);
        }
        "roller" => {
            let (cx, cy) = at(-0.3, 0.0);
            d.ring(cx, cy, 0.3);
        }
        "key" => {
            let (cx, cy) = at(-0.25, 0.0);
            d.ring(cx, cy, 0.25);
            d.line(&[at(-0.5, 0.0), at(-0.5, 0.35)]);
        }
        "proximity" => d.line(&[at(0.0, 0.0), at(-0.4, -0.4), at(-0.8, 0.0), at(-0.4, 0.4), at(0.0, 0.0)]),
        "pressure" | "level" | "flow" | "temperature" => {
            d.line(&[at(0.0, -0.4), at(-0.8, -0.4), at(-0.8, 0.4), at(0.0, 0.4), at(0.0, -0.4)]);
            let letter = match act {
                "pressure" => "p",
                "level" => "L",
                "flow" => "F",
                _ => "T",
            };
            let (cx, cy) = at(-0.4, 0.0);
            d.label("operator", letter, cx, cy, 0.55, "middle");
        }
        _ => {}
    }
}

/// Draw in the standard's own direction, then turn to the asked one.
fn build(p: &Params, d: &mut Draw) {
    draw(p, d);
    d.direct(p.text("direction"));
}

fn draw(p: &Params, d: &mut Draw) {
    let form = p.text("form");
    let nc = form == "nc";
    let co = form == "co";
    let timing = p.text("timing");
    let act = ACTUATORS.iter().copied().find(|a| *a == p.text("actuator")).unwrap_or("none");
    let poles = p.int("poles").clamp(1, 6);
    let std = standard(p);
    d.standard(std);
    let (a, b) = if nc { ("1", "2") } else { ("3", "4") };
    let timed = timing != "none";
    let worked = act != "none";
    if std == "iec60617" {
        // Upright: one column per pole, two modules apart (four for a changeover, whose make
        // terminal stands two modules right of its common one); a delay mark needs a module more
        // on the left, an operator two.
        let left = 1.0 + if timed { 1.0 } else { 0.0 } + if worked { 2.0 } else { 0.0 };
        let step = if co { 4.0 } else { 2.0 };
        let w = left + step * (poles as f64 - 1.0) + if co { 3.0 } else { 1.0 };
        d.size(w, 4.0);
        let mut mids = Vec::new();
        for k in 0..poles {
            let cx = left + step * k as f64;
            let n = k + 1;
            if co {
                // Common terminal below, break contact above it, make contact one column over.
                d.line(&[(cx, 0.0), (cx, 1.5)]);
                d.line(&[(cx, 1.5), (cx + 0.8, 1.5)]);
                d.line(&[(cx, 4.0), (cx, 2.5)]);
                d.line(&[(cx, 2.5), (cx + 0.95, 1.25)]);
                d.line(&[(cx + 2.0, 0.0), (cx + 2.0, 1.0), (cx + 1.3, 1.0)]);
                mids.push((cx + 0.47, 1.87));
                d.port(&format!("{n}1"), cx, 4.0, "down");
                d.port(&format!("{n}2"), cx, 0.0, "up");
                d.port(&format!("{n}4"), cx + 2.0, 0.0, "up");
                d.label(&format!("t{n}1"), &format!("{n}1"), cx + 0.3, 3.5, 0.8, "start");
                d.label(&format!("t{n}2"), &format!("{n}2"), cx - 0.3, 0.5, 0.8, "end");
                d.label(&format!("t{n}4"), &format!("{n}4"), cx + 2.3, 0.5, 0.8, "start");
                continue;
            }
            d.line(&[(cx, 0.0), (cx, 1.5)]);
            d.line(&[(cx, 4.0), (cx, 2.5)]);
            if nc {
                d.line(&[(cx, 1.5), (cx + 0.8, 1.5)]);
                d.line(&[(cx, 2.5), (cx + 0.95, 1.25)]);
                mids.push((cx + 0.47, 1.87));
            } else {
                d.line(&[(cx, 2.5), (cx - 0.9, 1.3)]);
                mids.push((cx - 0.45, 1.9));
            }
            d.port(&format!("{n}{a}"), cx, 0.0, "up");
            d.port(&format!("{n}{b}"), cx, 4.0, "down");
            d.label(&format!("t{n}{a}"), &format!("{n}{a}"), cx + 0.3, 0.5, 0.8, "start");
            d.label(&format!("t{n}{b}"), &format!("{n}{b}"), cx + 0.3, 3.5, 0.8, "start");
        }
        if let (Some(first), Some(last)) = (mids.first().copied(), mids.last().copied()) {
            if poles > 1 {
                d.dashed(first, last, 0.3, 0.2);
            }
            let mut stem = first;
            if timed {
                // The operating stem runs left from the first blade to the delay arc.
                let end = (first.0 - 1.2, first.1);
                d.line(&[end, first]);
                let (a0, a1) = if timing == "onDelay" { (90.0, 270.0) } else { (-90.0, 90.0) };
                d.arc(end.0, end.1, 0.4, a0, a1);
                stem = end;
            }
            if worked {
                let ax = first.0 - if timed { 2.6 } else { 1.6 };
                d.dashed((ax, stem.1), stem, 0.3, 0.2);
                operator(d, act, ax, stem.1, false);
            }
        }
        d.label("tag", p.text("tag"), w + 0.4, 2.0, 1.2, "start");
    } else {
        // Across a rung: one row per pole, two modules apart (three for a changeover); a delay
        // mark needs a module more on top, an operator two.
        let top = 1.0 + if timed { 1.0 } else { 0.0 } + if worked { 2.0 } else { 0.0 };
        let step = if co { 3.0 } else { 2.0 };
        let h = top + step * poles as f64 - if co { 0.0 } else { 1.0 };
        d.size(4.0, h);
        for k in 0..poles {
            let cy = top + step * k as f64;
            let n = k + 1;
            let plates = |d: &mut Draw, y: f64, slash: bool| {
                d.line(&[(1.6, y - 0.7), (1.6, y + 0.7)]);
                d.line(&[(2.4, y - 0.7), (2.4, y + 0.7)]);
                if slash {
                    d.line(&[(1.2, y + 0.9), (2.8, y - 0.9)]);
                }
            };
            if co {
                // The common terminal feeds a make row and, below it, a break row.
                d.line(&[(0.0, cy), (1.6, cy)]);
                d.line(&[(1.0, cy), (1.0, cy + 2.0), (1.6, cy + 2.0)]);
                d.line(&[(2.4, cy), (4.0, cy)]);
                d.line(&[(2.4, cy + 2.0), (4.0, cy + 2.0)]);
                plates(d, cy, false);
                plates(d, cy + 2.0, true);
                d.port(&format!("{n}1"), 0.0, cy, "left");
                d.port(&format!("{n}4"), 4.0, cy, "right");
                d.port(&format!("{n}2"), 4.0, cy + 2.0, "right");
                d.label(&format!("t{n}1"), &format!("{n}1"), 0.5, cy - 0.45, 0.6, "middle");
                d.label(&format!("t{n}4"), &format!("{n}4"), 3.4, cy - 0.45, 0.6, "middle");
                d.label(&format!("t{n}2"), &format!("{n}2"), 3.4, cy + 1.55, 0.6, "middle");
                continue;
            }
            d.line(&[(0.0, cy), (1.6, cy)]);
            d.line(&[(2.4, cy), (4.0, cy)]);
            plates(d, cy, nc);
            d.port(&format!("{n}{a}"), 0.0, cy, "left");
            d.port(&format!("{n}{b}"), 4.0, cy, "right");
            d.label(&format!("t{n}{a}"), &format!("{n}{a}"), 0.6, cy - 0.45, 0.6, "middle");
            d.label(&format!("t{n}{b}"), &format!("{n}{b}"), 3.4, cy - 0.45, 0.6, "middle");
        }
        if poles > 1 {
            d.dashed((2.0, top + 0.7), (2.0, h - 0.7), 0.3, 0.2);
        }
        let mut stem_top = top - 0.7;
        if timed {
            d.line(&[(2.0, top - 0.7), (2.0, top - 1.4)]);
            let (a0, a1) = if timing == "onDelay" { (180.0, 360.0) } else { (0.0, 180.0) };
            d.arc(2.0, top - 1.4, 0.4, a0, a1);
            stem_top = top - 1.4;
        }
        if worked {
            let ay = stem_top - 1.4;
            d.dashed((2.0, ay), (2.0, stem_top), 0.3, 0.2);
            operator(d, act, 2.0, ay, true);
        }
        d.label("tag", p.text("tag"), 4.4, top, 1.0, "start");
    }
}
