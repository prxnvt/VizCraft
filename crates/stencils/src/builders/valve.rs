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
//! paths for the port count and positions.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ISO 1219-1 and ISO 1219-2";

const ACTUATORS: &[&str] = &["none", "lever", "button", "pedal", "roller", "plunger", "solenoid", "pilot"];

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "valve",
    label: "Directional Valve",
    description: "An ISO 1219 directional control valve: two or three positions, two to five ports, flow paths per position from a short text (`P-A B-T; X; P-B A-T`), an operator at each side, a spring or detent return. Ports leave the rest position only and are named by ISO 9461 letters or ISO 5599 numbers.",
    params: &[
        ParamSpec {
            key: "positions",
            label: "Positions",
            kind: ParamKind::Int { min: 2, max: 3 },
            default: Def::Int(2),
            description: "Switching positions.",
        },
        ParamSpec { key: "ports", label: "Ports", kind: ParamKind::Int { min: 2, max: 5 }, default: Def::Int(4), description: "Ports (2/2 to 5/3)." },
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
            kind: ParamKind::Choice(ACTUATORS),
            default: Def::Text("solenoid"),
            description: "Operator at the left side.",
        },
        ParamSpec {
            key: "actuatorRight",
            label: "Right Operator",
            kind: ParamKind::Choice(ACTUATORS),
            default: Def::Text("none"),
            description: "Operator at the right side.",
        },
        ParamSpec {
            key: "return",
            label: "Return",
            kind: ParamKind::Choice(&["spring", "detent", "none"]),
            default: Def::Text("spring"),
            description: "What brings it back to rest.",
        },
        ParamSpec {
            key: "rest",
            label: "Rest Position",
            kind: ParamKind::Int { min: 0, max: 2 },
            default: Def::Int(1),
            description: "Index of the rest position, from the left; its square carries the ports.",
        },
        ParamSpec {
            key: "portLabels",
            label: "Port Labels",
            kind: ParamKind::Choice(&["iso9461", "iso5599"]),
            default: Def::Text("iso9461"),
            description: "Letters (P, A, B, T) or numbers (1, 2, 4, 3).",
        },
        ParamSpec {
            key: "medium",
            label: "Medium",
            kind: ParamKind::Choice(&["hydraulic", "pneumatic"]),
            default: Def::Text("hydraulic"),
            description: "Hydraulic draws solid pilot triangles, pneumatic hollow ones; pneumatic exhausts are R and S.",
        },
    ],
    sources: &[("ISO 1219", SOURCE)],
    build,
};

/// A port of the valve: its letter, its number, which side of the square and how far across.
#[derive(Clone, Copy)]
struct ValvePort {
    letter: &'static str,
    number: &'static str,
    top: bool,
    off: f64,
    exhaust: bool,
}

fn port_set(n: i64, pneumatic: bool) -> Vec<ValvePort> {
    let vp = |letter, number, top, off, exhaust| ValvePort { letter, number, top, off, exhaust };
    let t = if pneumatic { "R" } else { "T" };
    match n {
        2 => vec![vp("P", "1", false, 2.0, false), vp("A", "2", true, 2.0, false)],
        3 => vec![vp("P", "1", false, 1.0, false), vp("A", "2", true, 2.0, false), vp(t, "3", false, 3.0, true)],
        5 => vec![
            vp("P", "1", false, 2.0, false),
            vp("A", "2", true, 1.0, false),
            vp("B", "4", true, 3.0, false),
            vp("R", "3", false, 1.0, true),
            vp("S", "5", false, 3.0, true),
        ],
        _ => vec![vp("P", "1", false, 1.0, false), vp("A", "2", true, 1.0, false), vp("B", "4", true, 3.0, false), vp(t, "3", false, 3.0, true)],
    }
}

fn default_paths(ports: i64, positions: i64) -> &'static str {
    match (ports, positions) {
        (2, 3) => "P-A; P| A|; A-P",
        (2, _) => "P-A; P| A|",
        (3, 3) => "P-A T|; X; A-T P|",
        (3, _) => "P-A T|; A-T P|",
        (5, 3) => "P-A B-S; X; P-B A-R",
        (5, _) => "P-A B-S; P-B A-R",
        (_, 3) => "P-A B-T; X; P-B A-T",
        _ => "P-A B-T; P-B A-T",
    }
}

/// The port a token names: by letter or number, with T and R the same exhaust below five ports.
fn find<'a>(set: &'a [ValvePort], name: &str) -> Option<&'a ValvePort> {
    let name = name.trim().to_ascii_uppercase();
    set.iter()
        .find(|p| p.letter == name || p.number == name)
        .or_else(|| if set.len() < 5 && matches!(name.as_str(), "T" | "R") { set.iter().find(|p| p.exhaust) } else { None })
}

fn build(p: &Params, d: &mut Draw) {
    d.standard("iso1219");
    let positions = p.int("positions").clamp(2, 3);
    let pneumatic = p.text("medium") == "pneumatic";
    let set = port_set(p.int("ports"), pneumatic);
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
    let text = p.text("paths");
    let text = if text.trim().is_empty() { default_paths(p.int("ports"), positions) } else { text };
    let groups: Vec<&str> = text.split(';').collect();
    if groups.len() != positions as usize {
        d.warn(format!("`paths`: {} groups for {positions} positions", groups.len()));
    }
    for (i, group) in groups.iter().take(positions as usize).enumerate() {
        let x0 = 2.0 + 4.0 * i as f64;
        let at = |vp: &ValvePort| (x0 + vp.off, if vp.top { 1.0 } else { 5.0 });
        for token in group.split_whitespace().take(16) {
            if token.eq_ignore_ascii_case("x") {
                for vp in &set {
                    blocked(d, at(vp), vp.top);
                }
            } else if let Some(name) = token.strip_suffix('|') {
                match find(&set, name) {
                    Some(vp) => blocked(d, at(vp), vp.top),
                    None => d.warn(format!("`paths`: no port `{}` on this valve", name.chars().take(8).collect::<String>())),
                }
            } else if let Some((a, b)) = token.split_once('-') {
                match (find(&set, a), find(&set, b)) {
                    (Some(u), Some(v)) => flow(d, at(u), u.top, at(v), v.top),
                    _ => d.warn(format!("`paths`: `{}` does not join two ports of this valve", token.chars().take(16).collect::<String>())),
                }
            } else {
                d.warn(format!("`paths`: `{}` is not a flow (`P-A`), a block (`P|`) or `X`", token.chars().take(16).collect::<String>()));
            }
        }
    }
    // The ports leave the rest square.
    let rx = 2.0 + 4.0 * rest as f64;
    let numbers = p.text("portLabels") == "iso5599";
    for vp in &set {
        let x = rx + vp.off;
        let id = if numbers { vp.number } else { vp.letter };
        let (edge, end, dir) = if vp.top { (1.0, 0.0, "up") } else { (5.0, 6.0, "down") };
        d.line(&[(x, edge), (x, end)]);
        d.port(id, x, end, dir);
        d.label(&format!("p{id}"), id, x + 0.3, (edge + end) / 2.0, 0.7, "start");
    }
    let left = p.text("actuatorLeft");
    let right = p.text("actuatorRight");
    actuator(d, left, false, w, pneumatic);
    actuator(d, right, true, w, pneumatic);
    let ret = p.text("return");
    if ret != "none" {
        // The return acts at the side of a free operator place, the right when both are free.
        let (on_right, y) = match (left, right) {
            (_, "none") => (true, 3.0),
            ("none", _) => (false, 3.0),
            _ => (true, 4.4),
        };
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
