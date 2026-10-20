//! Directional valve builder: ISO 1219 envelopes with flow paths, actuators and return.
//!
//! Provenance: own design, drawn from the written rules of ISO 1219-1 and ISO 1219-2. One square
//! envelope per switching position, side by side; inside each, a line with an arrowhead per
//! flow path and a short stub with a cross bar per blocked port; the ports leave the square of
//! the rest position only; the operators stand at the outer sides (a lever, push button, pedal,
//! roller, plunger, solenoid as a box with a diagonal, pilot as a triangle on a dashed line)
//! and the return at the side whose square is the rest position (a zigzag spring or a notched
//! detent). Port letters follow ISO 9461 (P, A, B, T, R, S) and numbers ISO 5599-3 (1, 2, 4, 3,
//! 5).
//!
//! The `paths` text names the flow in each position, positions separated by `;`, left to right:
//! `P-A` is a flow from P to A, `P|` blocks P, `X` blocks every port. Empty text draws the usual
//! paths for the port count and positions. The grammar, port sets and defaults live in
//! `vizcraft_notation::fluid::paths`, which the circuit checks read too.
//!
//! Ports come fluid ports first, then a pilot port at each pilot-operated side (X and Y, or
//! `12` and `14` by ISO 5599: the output the supply joins in that side's square), then `el` and
//! `er` on solenoids and `ml` and `mr` on rollers and plungers, so adding an operator never shifts
//! a glued fluid port.

use vizcraft_notation::fluid::paths::{self as fp, Token};

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ISO 1219-1 and ISO 1219-2";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "valve",
    label: "Directional Valve",
    description: "An ISO 1219 directional control valve: two or three positions, two to five ports, flow paths per position from a short text (`P-A B-T; X; P-B A-T`), an operator at each side, a spring or detent return. Ports leave the rest position only and are named by ISO 9461 letters or ISO 5599 numbers; pilot sides add X and Y (or 12 and 14), solenoids el and er, rollers and plungers ml and mr.",
    params: &[
        ParamSpec {
            key: "positions",
            label: "Positions",
            kind: ParamKind::Int { min: 2, max: 3 },
            default: Def::Int(fp::DEFAULT_POSITIONS),
            description: "Switching positions.",
        },
        ParamSpec {
            key: "ports",
            label: "Ports",
            kind: ParamKind::Int { min: 2, max: 5 },
            default: Def::Int(fp::DEFAULT_PORTS),
            description: "Ports (2/2 to 5/3).",
        },
        ParamSpec {
            key: "paths",
            label: "Paths",
            kind: ParamKind::Text { max: 128 },
            default: Def::Text(""),
            description: "Flow per position, `;` between positions: `P-A` flows, `P|` blocks a port, `X` blocks all. Empty: the usual paths.",
        },
        ParamSpec {
            key: "actuatorLeft",
            label: "Left Operator",
            kind: ParamKind::Choice(fp::ACTUATORS),
            default: Def::Text(fp::DEFAULT_LEFT),
            description: "Operator at the left side.",
        },
        ParamSpec {
            key: "actuatorRight",
            label: "Right Operator",
            kind: ParamKind::Choice(fp::ACTUATORS),
            default: Def::Text(fp::DEFAULT_RIGHT),
            description: "Operator at the right side.",
        },
        ParamSpec {
            key: "return",
            label: "Return",
            kind: ParamKind::Choice(&["spring", "detent", "none"]),
            default: Def::Text(fp::DEFAULT_RETURN),
            description: "What brings it back to rest.",
        },
        ParamSpec {
            key: "rest",
            label: "Rest Position",
            kind: ParamKind::Int { min: 0, max: 2 },
            default: Def::Int(fp::DEFAULT_REST),
            description: "Index of the rest position, from the left; its square carries the ports.",
        },
        ParamSpec {
            key: "portLabels",
            label: "Port Labels",
            kind: ParamKind::Choice(&["iso9461", "iso5599"]),
            default: Def::Text(fp::DEFAULT_LABELS),
            description: "Letters (P, A, B, T) or numbers (1, 2, 4, 3).",
        },
        ParamSpec {
            key: "medium",
            label: "Medium",
            kind: ParamKind::Choice(&["hydraulic", "pneumatic"]),
            default: Def::Text(fp::DEFAULT_MEDIUM),
            description: "Hydraulic draws solid pilot triangles, pneumatic hollow ones; pneumatic exhausts are R and S.",
        },
        ParamSpec {
            key: "tag",
            label: "Code",
            kind: ParamKind::Text { max: 32 },
            default: Def::Text(""),
            description: "The component code written above the valve (the element's tag).",
        },
    ],
    sources: &[("ISO 1219", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    d.standard("iso1219");
    let positions = p.int("positions").clamp(2, 3);
    let pneumatic = p.text("medium") == "pneumatic";
    let set = fp::port_set(p.int("ports"), pneumatic);
    let mut rest = p.int("rest");
    if rest >= positions {
        d.warn(format!("`rest`: a valve with {positions} positions rests at 0 to {}; the last is used", positions - 1));
        rest = positions - 1;
    }
    let w = 4.0 * positions as f64 + 4.0;
    d.size(w, 6.0);
    for i in 0..positions {
        let x0 = 2.0 + 4.0 * i as f64;
        d.rect(x0, 1.0, x0 + 4.0, 5.0);
    }
    let text = fp::effective(p.text("paths"), p.int("ports"), positions);
    let groups = fp::groups(text);
    if groups.len() != positions as usize {
        d.warn(format!("`paths`: {} groups for {positions} positions", groups.len()));
    }
    for (i, group) in groups.iter().take(positions as usize).enumerate() {
        let x0 = 2.0 + 4.0 * i as f64;
        let at = |vp: &fp::ValvePort| (x0 + vp.off, if vp.top { 1.0 } else { 5.0 });
        for token in fp::tokens(group) {
            match token {
                Token::All => {
                    for vp in &set {
                        blocked(d, at(vp), vp.top);
                    }
                }
                Token::Block(name) => match fp::find(&set, name) {
                    Some(vp) => blocked(d, at(vp), vp.top),
                    None => d.warn(format!("`paths`: no port `{}` on this valve", name.chars().take(8).collect::<String>())),
                },
                Token::Flow(a, b) => match (fp::find(&set, a), fp::find(&set, b)) {
                    (Some(u), Some(v)) => flow(d, at(u), u.top, at(v), v.top),
                    _ => d.warn(format!(
                        "`paths`: `{}-{}` does not join two ports of this valve",
                        a.chars().take(8).collect::<String>(),
                        b.chars().take(8).collect::<String>()
                    )),
                },
                Token::Bad(t) => {
                    d.warn(format!("`paths`: `{}` is not a flow (`P-A`), a block (`P|`) or `X`", t.chars().take(16).collect::<String>()))
                }
            }
        }
    }
    // The ports leave the rest square.
    let rx = 2.0 + 4.0 * rest as f64;
    let numbers = p.text("portLabels") == "iso5599";
    for vp in &set {
        let x = rx + vp.off;
        let id = vp.id(numbers);
        let (edge, end, dir) = if vp.top { (1.0, 0.0, "up") } else { (5.0, 6.0, "down") };
        d.line(&[(x, edge), (x, end)]);
        d.port(id, x, end, dir);
        d.label(&format!("p{id}"), id, x + 0.3, (edge + end) / 2.0, 0.7, "start");
    }
    let left = p.text("actuatorLeft");
    let right = p.text("actuatorRight");
    actuator(d, left, false, w, pneumatic);
    actuator(d, right, true, w, pneumatic);
    // Operator ports: pilots after the fluid ports, then electrical and mechanical ones.
    let flows = fp::flows(&set, text, positions as usize);
    let square = |i: usize| flows.get(i).map(Vec::as_slice).unwrap_or_default();
    let sides = [(left, false, square(0)), (right, true, square(positions as usize - 1))];
    for (kind, on_right, sq) in sides {
        if fp::operator_port(kind) == Some("pilot") {
            let id = fp::pilot_id(&set, sq, numbers, !on_right);
            operator_port(d, &id, on_right, w, reach(kind), true);
        }
    }
    for (kind, on_right, _) in sides {
        match fp::operator_port(kind) {
            Some("el") => operator_port(d, if on_right { "er" } else { "el" }, on_right, w, reach(kind), false),
            Some("ml") => operator_port(d, if on_right { "mr" } else { "ml" }, on_right, w, reach(kind), false),
            _ => {}
        }
    }
    let ret = p.text("return");
    if ret != "none" {
        // The return acts at the side of a free operator place, the right when both are free.
        let on_right = fp::return_on_right(left, right);
        let y = if left != "none" && right != "none" { 4.4 } else { 3.0 };
        let edge = if on_right { w - 2.0 } else { 2.0 };
        let out = if on_right { 1.0 } else { -1.0 };
        if ret == "spring" {
            let mut pts = vec![(edge, y)];
            for k in 1..=6 {
                let dy = if k % 2 == 1 { -0.4 } else { 0.4 };
                pts.push((edge + out * 0.25 * k as f64, y + dy));
            }
            pts.push((edge + out * 1.6, y));
            d.line(&pts);
        } else {
            d.line(&[(edge, y), (edge + out * 1.6, y)]);
            for k in 0..3 {
                let x = edge + out * (0.35 + 0.45 * k as f64);
                d.line(&[(x - 0.18, y + 0.45), (x, y + 0.1), (x + 0.18, y + 0.45)]);
            }
        }
    }
    d.label("tag", p.text("tag"), 0.0, -0.6, 0.8, "start");
}

/// How far from the frame's side an operator's outer end is, in modules.
fn reach(kind: &str) -> f64 {
    match kind {
        "solenoid" => 0.6,
        "roller" => 0.4,
        "plunger" => 1.0,
        _ => 0.3,
    }
}

/// A port at the outer end of an operator: a short stub from the operator, `inner` modules in
/// from the frame's side, out to that side at mid height. Pilot ports are labelled.
fn operator_port(d: &mut Draw, id: &str, right: bool, w: f64, inner: f64, labelled: bool) {
    let (x, inner, dir) = if right { (w, w - inner, "right") } else { (0.0, inner, "left") };
    d.line(&[(inner, 3.0), (x, 3.0)]);
    d.port(id, x, 3.0, dir);
    if labelled {
        let (lx, anchor) = if right { (w - 0.1, "end") } else { (0.1, "start") };
        d.label(&format!("p{id}"), id, lx, 1.6, 0.7, anchor);
    }
}

/// A blocked port: a stub into the square with a cross bar.
fn blocked(d: &mut Draw, (x, y): (f64, f64), top: bool) {
    let inward = if top { 0.8 } else { -0.8 };
    d.line(&[(x, y), (x, y + inward)]);
    d.line(&[(x - 0.4, y + inward), (x + 0.4, y + inward)]);
}

/// A flow path from `u` to `v` with its arrowhead at `v`. Ports on the same side join through
/// the middle of the square.
fn flow(d: &mut Draw, u: (f64, f64), u_top: bool, v: (f64, f64), v_top: bool) {
    let pts = if u_top == v_top { vec![u, (u.0, 3.0), (v.0, 3.0), v] } else { vec![u, v] };
    d.line(&pts);
    let before = pts.iter().rev().nth(1).copied().unwrap_or(u);
    d.head(before, v, 0.6, 0.25, true);
}

/// An operator at one outer side of the valve, mirrored for the right side.
fn actuator(d: &mut Draw, kind: &str, right: bool, w: f64, pneumatic: bool) {
    let x = |v: f64| if right { w - v } else { v };
    let y = 3.0;
    match kind {
        "lever" => {
            d.line(&[(x(2.0), y), (x(1.2), y)]);
            d.line(&[(x(1.2), y), (x(0.6), y - 1.4)]);
        }
        "button" => {
            d.line(&[(x(2.0), y), (x(1.0), y)]);
            let (a0, a1) = if right { (-90.0, 90.0) } else { (90.0, 270.0) };
            d.arc(x(1.0), y, 0.5, a0, a1);
        }
        "pedal" => {
            d.line(&[(x(2.0), y), (x(1.2), y)]);
            d.line(&[(x(0.6), y - 0.8), (x(1.4), y + 0.8)]);
        }
        "roller" => {
            d.line(&[(x(2.0), y), (x(1.2), y)]);
            d.ring(x(0.8), y, 0.4);
        }
        "plunger" => {
            d.line(&[(x(2.0), y), (x(1.0), y)]);
            d.line(&[(x(1.0), y - 0.6), (x(1.0), y + 0.6)]);
        }
        "solenoid" => {
            let (a, b) = (x(0.6), x(2.0));
            d.rect(a.min(b), y - 0.8, a.max(b), y + 0.8);
            d.line(&[(x(0.6), y + 0.8), (x(2.0), y - 0.8)]);
        }
        "pilot" => {
            d.dashed((x(2.0), y), (x(0.9), y), 0.25, 0.15);
            d.head((x(0.2), y), (x(0.9), y), 0.6, 0.35, !pneumatic);
        }
        _ => {}
    }
}
