//! Component builder: resistors, capacitors, inductors, diodes, batteries, transistors,
//! operational amplifiers, earth, and the signalling devices of a control circuit (pilot
//! lamps, meters and horns).
//!
//! Provenance: own design, drawn from the written rules of IEC 60617-4 and -5 and of IEEE 315. A resistor is a
//! rectangle (IEC) or a zigzag (IEEE 315); a capacitor two parallel plates, one curved or solid
//! when it is polarised; an inductor a row of half circles; a diode a triangle against a bar; a
//! battery a long and a short plate; a transistor a base bar with collector and emitter leads,
//! the emitter carrying an arrow; an amplifier a triangle (IEEE 315) or a box with a triangle mark
//! (IEC); earth three shortening bars, inside a circle for protective earth and over hatching for
//! a chassis. A variable part has an arrow across it. A lamp is a circle with a diagonal cross
//! (IEC) or with rays (IEEE 315 and JIC), a meter a circle with its quantity letter, a horn a
//! bell mouth; IEC draws them upright between their terminals like a coil, IEEE 315 and JIC
//! across a rung. A terminal is a small circle on the line, its two sides one connection.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const STANDARDS: &[&str] = &["iec60617", "ieee315", "jic"];

const SOURCE: &str = "own design, drawn from the written rules of IEC 60617-4 and -5 and of IEEE 315";

/// The components drawn.
const COMPONENTS: &[&str] =
    &["resistor", "capacitor", "inductor", "diode", "battery", "transistor", "opamp", "earth", "lamp", "meter", "horn", "terminal"];

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "passive",
    label: "Component",
    description: "A circuit component: resistor, capacitor, inductor, diode, battery, transistor, operational amplifier or earth, in the IEC 60617 or IEEE 315 form. Also a pilot lamp, meter or horn. Ports: 1 and 2 for a two-terminal part, A and K for a diode, + and - for a battery, B, C and E for a transistor, -, + and OUT for an amplifier, PE for earth, X1 and X2 for a lamp, 1 and 2 for a terminal (its two sides).",
    params: &[
        ParamSpec {
            key: "component",
            label: "Component",
            kind: ParamKind::Choice(COMPONENTS),
            default: Def::Text("resistor"),
            description: "Which component it is.",
        },
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
        ParamSpec { key: "polarised", label: "Polarised", kind: ParamKind::Bool, default: Def::Bool(false), description: "A polarised capacitor." },
        ParamSpec { key: "variable", label: "Variable", kind: ParamKind::Bool, default: Def::Bool(false), description: "An arrow across the part." },
        ParamSpec {
            key: "kind",
            label: "Earth kind",
            kind: ParamKind::Choice(&["protective", "functional", "chassis"]),
            default: Def::Text("protective"),
            description: "For earth: protective (PE), functional or chassis.",
        },
        ParamSpec {
            key: "function",
            label: "Function",
            kind: ParamKind::Text { max: 16 },
            default: Def::Text(""),
            description: "What a meter or relay circle shows inside: a quantity (V, A) or a device function number (51, 87T).",
        },
        ParamSpec {
            key: "colour",
            label: "Colour",
            kind: ParamKind::Text { max: 4 },
            default: Def::Text(""),
            description: "A lamp's colour code (RD, GN, YE).",
        },
        ParamSpec {
            key: "value",
            label: "Value",
            kind: ParamKind::Text { max: 24 },
            default: Def::Text(""),
            description: "Value shown below (4k7, 100n).",
        },
        ParamSpec { key: "tag", label: "Tag", kind: ParamKind::Text { max: 16 }, default: Def::Text(""), description: "Device tag (R1, C1)." },
    ],
    sources: &[("IEC 60617", SOURCE), ("IEEE 315", SOURCE)],
    build,
};

/// Draw, then turn to the asked direction (two-terminal parts only).
fn build(p: &Params, d: &mut Draw) {
    draw(p, d);
    if !matches!(p.text("component"), "transistor" | "opamp" | "earth") {
        d.direct(p.text("direction"));
    }
}

fn draw(p: &Params, d: &mut Draw) {
    let std = STANDARDS.iter().copied().find(|s| *s == p.text("standard")).unwrap_or("iec60617");
    d.standard(std);
    let iec = std == "iec60617";
    let component = COMPONENTS.iter().copied().find(|c| *c == p.text("component")).unwrap_or("resistor");
    let signal = matches!(component, "lamp" | "meter" | "horn");
    let (w, h) = match component {
        "transistor" => (4.0, 4.0),
        "opamp" => (6.0, 4.0),
        "earth" => (2.0, 3.0),
        _ if signal && iec => (4.0, 4.0),
        _ if signal || component == "terminal" => (4.0, 2.0),
        _ => (6.0, 2.0),
    };
    d.size(w, h);
    match component {
        "transistor" => transistor(d, iec),
        "opamp" => opamp(d, iec),
        "earth" => earth(d, p.text("kind")),
        _ if signal => {
            let letter = if p.text("function").trim().is_empty() { p.text("value") } else { p.text("function") };
            signalling(d, component, iec, p.text("colour"), letter)
        }
        "terminal" => {
            d.line(&[(0.0, 1.0), (1.6, 1.0)]);
            d.line(&[(2.4, 1.0), (4.0, 1.0)]);
            d.circle(2.0, 1.0, 0.4);
            d.port("1", 0.0, 1.0, "left");
            d.port("2", 4.0, 1.0, "right");
        }
        _ => two_terminal(d, component, iec, p.flag("polarised")),
    }
    if p.flag("variable") && !signal && !matches!(component, "earth" | "opamp" | "transistor" | "terminal") {
        d.line(&[(1.6, 1.9), (4.4, 0.1)]);
        d.head((1.6, 1.9), (4.4, 0.1), 0.5, 0.2, false);
    }
    d.label("tag", p.text("tag"), w / 2.0, -0.7, 1.0, "middle");
    if !matches!(component, "meter" | "terminal") {
        d.label("value", p.text("value"), w / 2.0, h + 0.7, 0.9, "middle");
    }
}

/// A lamp, meter or horn: upright between its terminals (IEC) or across a rung.
fn signalling(d: &mut Draw, component: &str, iec: bool, colour: &str, value: &str) {
    let (a, b) = if component == "lamp" { ("X1", "X2") } else { ("1", "2") };
    let (cx, cy, r) = if iec { (2.0, 2.0, 1.0) } else { (2.0, 1.0, 0.8) };
    if iec {
        d.line(&[(2.0, 0.0), (2.0, 1.0)]);
        d.line(&[(2.0, 3.0), (2.0, 4.0)]);
    } else {
        d.line(&[(0.0, 1.0), (1.2, 1.0)]);
        d.line(&[(2.8, 1.0), (4.0, 1.0)]);
    }
    match component {
        "horn" => {
            // A bell mouth opening away from the terminals' line.
            if iec {
                d.shape(&[(1.4, 1.0), (2.6, 1.0), (3.2, 3.0), (0.8, 3.0)]);
            } else {
                d.shape(&[(1.2, 0.6), (1.2, 1.4), (2.8, 1.8), (2.8, 0.2)]);
            }
        }
        _ => {
            d.circle(cx, cy, r);
            if component == "lamp" {
                let k = r * std::f64::consts::FRAC_1_SQRT_2;
                if iec {
                    d.line(&[(cx - k, cy - k), (cx + k, cy + k)]);
                    d.line(&[(cx - k, cy + k), (cx + k, cy - k)]);
                } else {
                    for (dx, dy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                        let (x0, y0) = (cx + dx * k * 1.15, cy + dy * k * 1.15);
                        d.line(&[(x0, y0), (x0 + dx * 0.3, y0 + dy * 0.3)]);
                    }
                    d.label("colour", colour, cx, cy, 0.55, "middle");
                }
                if iec {
                    d.label("colour", colour, cx + r + 0.4, cy, 0.8, "start");
                }
            } else {
                let letter = if value.trim().is_empty() { "M" } else { value };
                // Longer device numbers (50/51) shrink to stay inside the circle.
                let n = letter.chars().count().max(2) as f64;
                d.label("letter", letter, cx, cy, (r * 1.8 / n).min(r * 0.9), "middle");
            }
        }
    }
    if iec {
        d.port(a, 2.0, 0.0, "up");
        d.port(b, 2.0, 4.0, "down");
    } else {
        d.port(a, 0.0, 1.0, "left");
        d.port(b, 4.0, 1.0, "right");
    }
}

fn two_terminal(d: &mut Draw, component: &str, iec: bool, polarised: bool) {
    let (a, b) = match component {
        "diode" => ("A", "K"),
        "battery" => ("+", "-"),
        _ => ("1", "2"),
    };
    match component {
        "capacitor" | "battery" => {
            d.line(&[(0.0, 1.0), (2.7, 1.0)]);
            d.line(&[(3.3, 1.0), (6.0, 1.0)]);
            let long = 0.8;
            let short = if component == "battery" { 0.4 } else { long };
            d.line(&[(2.7, 1.0 - long), (2.7, 1.0 + long)]);
            if component == "capacitor" && polarised {
                if iec {
                    d.rect(3.3, 1.0 - long, 3.5, 1.0 + long);
                } else {
                    d.arc(3.9, 1.0, 0.9, 155.0, 205.0);
                }
                d.label("plus", "+", 2.2, 0.2, 0.7, "middle");
            } else {
                d.line(&[(3.3, 1.0 - short), (3.3, 1.0 + short)]);
            }
        }
        "inductor" => {
            d.line(&[(0.0, 1.0), (1.5, 1.0)]);
            d.line(&[(4.5, 1.0), (6.0, 1.0)]);
            for i in 0..4 {
                let cx = 1.875 + 0.75 * i as f64;
                d.arc(cx, 1.0, 0.375, 180.0, 360.0);
            }
        }
        "diode" => {
            d.line(&[(0.0, 1.0), (2.0, 1.0)]);
            d.line(&[(3.4, 1.0), (6.0, 1.0)]);
            d.shape(&[(2.0, 0.3), (2.0, 1.7), (3.4, 1.0)]);
            d.line(&[(3.4, 0.3), (3.4, 1.7)]);
        }
        _ => {
            d.line(&[(0.0, 1.0), (1.5, 1.0)]);
            d.line(&[(4.5, 1.0), (6.0, 1.0)]);
            if iec {
                d.rect(1.5, 0.55, 4.5, 1.45);
            } else {
                let mut pts = vec![(1.5, 1.0)];
                for i in 0..6 {
                    let x = 1.75 + 0.5 * i as f64;
                    pts.push((x, if i % 2 == 0 { 0.5 } else { 1.5 }));
                }
                pts.push((4.5, 1.0));
                d.line(&pts);
            }
        }
    }
    d.port(a, 0.0, 1.0, "left");
    d.port(b, 6.0, 1.0, "right");
}

fn transistor(d: &mut Draw, iec: bool) {
    d.line(&[(0.0, 2.0), (1.5, 2.0)]);
    d.line(&[(1.5, 1.0), (1.5, 3.0)]);
    d.line(&[(1.5, 1.6), (3.0, 0.8), (3.0, 0.0)]);
    d.line(&[(1.5, 2.4), (3.0, 3.2), (3.0, 4.0)]);
    d.head((1.5, 2.4), (2.6, 2.99), 0.5, 0.2, true);
    if !iec {
        d.ring(2.2, 2.0, 1.5);
    }
    d.port("B", 0.0, 2.0, "left");
    d.port("C", 3.0, 0.0, "up");
    d.port("E", 3.0, 4.0, "down");
}

fn opamp(d: &mut Draw, iec: bool) {
    d.line(&[(0.0, 1.0), (1.0, 1.0)]);
    d.line(&[(0.0, 3.0), (1.0, 3.0)]);
    if iec {
        d.rect(1.0, 0.0, 5.0, 4.0);
        d.line(&[(5.0, 2.0), (6.0, 2.0)]);
        d.line(&[(3.6, 0.4), (3.6, 1.2), (4.3, 0.8), (3.6, 0.4)]);
    } else {
        d.shape(&[(1.0, 0.0), (1.0, 4.0), (5.0, 2.0)]);
        d.line(&[(5.0, 2.0), (6.0, 2.0)]);
    }
    d.label("minus", "-", 1.5, 1.0, 0.9, "middle");
    d.label("plus", "+", 1.5, 3.0, 0.9, "middle");
    d.port("-", 0.0, 1.0, "left");
    d.port("+", 0.0, 3.0, "left");
    d.port("OUT", 6.0, 2.0, "right");
}

fn earth(d: &mut Draw, kind: &str) {
    d.line(&[(1.0, 0.0), (1.0, 1.5)]);
    d.line(&[(0.1, 1.5), (1.9, 1.5)]);
    if kind == "chassis" {
        for x in [0.3, 0.9, 1.5] {
            d.line(&[(x, 1.5), (x - 0.4, 2.2)]);
        }
    } else {
        d.line(&[(0.4, 2.0), (1.6, 2.0)]);
        d.line(&[(0.7, 2.5), (1.3, 2.5)]);
        if kind == "protective" {
            d.ring(1.0, 1.9, 0.95);
        }
    }
    d.port("PE", 1.0, 0.0, "up");
}
