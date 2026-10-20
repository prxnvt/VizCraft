//! Process valve builder: the valve and fitting symbols of P&IDs, with an actuator, a fail
//! position, a normally closed body and the end joints.
//!
//! Provenance: own design, drawn from the written rules of ISO 10628-2, ISO 14617-8 and ANSI/ISA-5.1. A valve body is two
//! triangles meeting point to point across the flow (a gate valve); a globe valve adds a dot
//! where they meet, a ball valve a circle, a plug valve a small square, a needle valve a small
//! downward wedge, a diaphragm valve a dome over the seat, and a butterfly valve is a disc drawn
//! as a slanted line between two end bars. An angle valve turns one triangle down, a three-way
//! valve adds a third; a safety valve is an angle body with a spring over it. A check valve
//! shows its one-way seat: a seat bar and a ball (ISA and ISO 10628) or an arrowhead over the
//! body (the ISO 14617 supplementary check symbol composed onto the basic valve). A normally
//! closed valve is drawn with a solid body. The actuator stands on a stem above the body: a
//! handwheel bar, a diaphragm dome, a piston box, a motor circle, a solenoid box or a
//! self-acting dome with its sensing line; the fail position is an arrow on the stem (open up,
//! closed down) or the letters FL for fail locked. End joints are marks at the ports: a bar for
//! flanged, a dot for welded, a bracket for socket welded, nothing for threaded or plain.
//!
//! The frame is four modules wide and four high (five for angle, three-way and safety bodies),
//! the actuator always has room above the body, so changing the actuator or the standard never
//! moves a port.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ISO 10628-2, ISO 14617-8 and ANSI/ISA-5.1";

/// The bodies drawn.
pub const BODIES: &[&str] = &["gate", "globe", "ball", "butterfly", "plug", "needle", "check", "diaphragm", "angle", "threeWay", "safety"];

/// The actuators drawn.
pub const ACTUATORS: &[&str] = &["none", "manual", "diaphragm", "piston", "motor", "solenoid", "selfActing"];

/// The fail positions (ISA-5.1).
pub const FAILS: &[&str] = &["none", "FO", "FC", "FL", "FL/DO", "FL/DC"];

/// The end joints (ASME Y32.2.3 joint styles).
pub const ENDS: &[&str] = &["plain", "flanged", "threaded", "welded", "socketWelded"];

/// The standards drawn.
pub const STANDARDS: &[&str] = &["iso10628", "isa", "iso14617"];

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "processValve",
    label: "Process Valve",
    description: "A P&ID valve or fitting: the body (gate, globe, ball, butterfly, plug, needle, check, diaphragm, angle, three-way, safety), an actuator on a stem above it, the fail position, a solid body for a normally closed valve and the end joints, drawn as ISO 10628, ISA-5.1 or ISO 14617. The flow ports sit on the module grid at the body's ends and never move when the actuator or the standard changes.",
    params: &[
        ParamSpec { key: "body", label: "Body", kind: ParamKind::Choice(BODIES), default: Def::Text("gate"), description: "The valve body." },
        ParamSpec {
            key: "actuator",
            label: "Actuator",
            kind: ParamKind::Choice(ACTUATORS),
            default: Def::Text("none"),
            description: "What moves the valve.",
        },
        ParamSpec {
            key: "fail",
            label: "Fail Position",
            kind: ParamKind::Choice(FAILS),
            default: Def::Text("none"),
            description: "Where the valve goes when its power fails: open, closed or locked (drifting open or closed).",
        },
        ParamSpec {
            key: "normallyClosed",
            label: "Normally Closed",
            kind: ParamKind::Bool,
            default: Def::Bool(false),
            description: "Drawn with a solid body.",
        },
        ParamSpec {
            key: "ends",
            label: "End Joints",
            kind: ParamKind::Choice(ENDS),
            default: Def::Text("plain"),
            description: "How the valve joins the pipe.",
        },
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(STANDARDS),
            default: Def::Text("iso10628"),
            description: "The symbol standard drawn.",
        },
    ],
    sources: &[("ISO 10628-2", SOURCE), ("ANSI/ISA-5.1", SOURCE), ("ISO 14617-8", SOURCE)],
    build,
};

/// The middle of the body across the flow.
const MID: f64 = 3.0;
/// The top of the body.
const TOP: f64 = 2.0;
/// The bottom of the body.
const BOTTOM: f64 = 4.0;

fn build(p: &Params, d: &mut Draw) {
    // Every standard here draws on the 2.5 mm module unless the page gives another.
    d.standard("iso10628");
    let body = p.text("body");
    let standard = p.text("standard");
    let solid = p.flag("normallyClosed");
    let tall = matches!(body, "angle" | "threeWay" | "safety");
    d.size(4.0, if tall { 5.0 } else { 4.0 });
    let left = [(0.0, TOP), (0.0, BOTTOM), (2.0, MID)];
    let right = [(4.0, TOP), (4.0, BOTTOM), (2.0, MID)];
    let down = [(1.0, 5.0), (3.0, 5.0), (2.0, MID)];
    match body {
        "butterfly" => {
            d.line(&[(0.0, TOP), (0.0, BOTTOM)]);
            d.line(&[(4.0, TOP), (4.0, BOTTOM)]);
            d.line(&[(0.0, MID), (4.0, MID)]);
            d.line(&[(1.4, BOTTOM - 0.1), (2.6, TOP + 0.1)]);
            d.circle(2.0, MID, 0.18);
        }
        "angle" => triangles(d, &[left, down], solid),
        "safety" => triangles(d, &[right, down], solid),
        "threeWay" => triangles(d, &[left, right, down], solid),
        _ => triangles(d, &[left, right], solid),
    }
    match body {
        "globe" => d.circle(2.0, MID, 0.35),
        "ball" => d.ring(2.0, MID, 0.55),
        "plug" => d.rect(1.7, MID - 0.3, 2.3, MID + 0.3),
        "needle" => d.shape(&[(1.7, TOP + 0.3), (2.3, TOP + 0.3), (2.0, MID)]),
        "diaphragm" => d.arc(2.0, MID, 0.7, 180.0, 360.0),
        "check" if standard == "iso14617" => {
            // The supplementary check symbol composed onto the body: a one-way arrowhead.
            d.line(&[(1.2, TOP - 0.5), (2.8, TOP - 0.5)]);
            d.head((1.2, TOP - 0.5), (3.0, TOP - 0.5), 0.5, 0.25, true);
        }
        "check" => {
            d.line(&[(2.0, TOP + 0.2), (2.0, BOTTOM - 0.2)]);
            d.circle(2.55, MID, 0.3);
        }
        _ => {}
    }
    let actuator = p.text("actuator");
    if body == "safety" {
        // The spring that holds the seat shut, on the stem.
        let mut pts = vec![(2.0, MID)];
        for k in 1..=6 {
            let x = if k % 2 == 1 { 1.5 } else { 2.5 };
            pts.push((x, MID - 0.4 * k as f64));
        }
        pts.push((2.0, 0.4));
        d.line(&pts);
    } else if actuator != "none" {
        actuator_on(d, actuator, standard);
    }
    let fail = p.text("fail");
    if actuator != "none" && fail != "none" {
        match fail {
            "FO" => {
                d.line(&[(3.6, 1.9), (3.6, 0.7)]);
                d.head((3.6, 1.9), (3.6, 0.5), 0.4, 0.2, true);
            }
            "FC" => {
                d.line(&[(3.6, 0.5), (3.6, 1.7)]);
                d.head((3.6, 0.5), (3.6, 1.9), 0.4, 0.2, true);
            }
            other => d.label("fail", other, 3.3, 1.7, 0.55, "start"),
        }
    }
    ends(d, p.text("ends"), body);
    // The flow ports at the body's ends; an angle body turns its outlet down.
    match body {
        "angle" => {
            d.port("1", 0.0, MID, "left");
            d.port("2", 2.0, 5.0, "down");
        }
        "safety" => {
            d.port("1", 2.0, 5.0, "down");
            d.port("2", 4.0, MID, "right");
        }
        "threeWay" => {
            d.port("1", 0.0, MID, "left");
            d.port("2", 4.0, MID, "right");
            d.port("3", 2.0, 5.0, "down");
        }
        _ => {
            d.port("1", 0.0, MID, "left");
            d.port("2", 4.0, MID, "right");
        }
    }
}

/// The body triangles, solid (hatched densely) for a normally closed valve.
fn triangles(d: &mut Draw, tris: &[[(f64, f64); 3]], solid: bool) {
    for t in tris {
        d.shape(t);
        if solid {
            // Lines across the triangle from its base towards its point, close enough to read
            // as a filled body at any zoom.
            let [a, b, apex] = *t;
            for k in 1..10 {
                let f = k as f64 / 10.0;
                let from = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
                d.line(&[from, apex]);
            }
        }
    }
}

/// The actuator on its stem above the body.
fn actuator_on(d: &mut Draw, kind: &str, standard: &str) {
    match kind {
        "manual" => {
            d.line(&[(2.0, MID), (2.0, 0.8)]);
            d.line(&[(1.0, 0.8), (3.0, 0.8)]);
        }
        "diaphragm" => {
            d.line(&[(2.0, MID), (2.0, 1.4)]);
            d.line(&[(0.8, 1.4), (3.2, 1.4)]);
            if standard == "iso14617" {
                // A flat capsule rather than a dome.
                d.line(&[(0.8, 1.4), (0.8, 0.6), (3.2, 0.6), (3.2, 1.4)]);
            } else {
                d.arc(2.0, 1.4, 1.2, 180.0, 360.0);
            }
        }
        "piston" => {
            d.line(&[(2.0, MID), (2.0, 1.6)]);
            d.rect_line(1.0, 0.2, 3.0, 1.6);
            d.line(&[(1.0, 0.9), (3.0, 0.9)]);
        }
        "motor" => {
            d.line(&[(2.0, MID), (2.0, 1.6)]);
            d.ring(2.0, 0.85, 0.75);
            d.label("act", "M", 2.0, 0.85, 0.7, "middle");
        }
        "solenoid" => {
            d.line(&[(2.0, MID), (2.0, 1.6)]);
            d.rect_line(1.2, 0.2, 2.8, 1.6);
            d.label("act", "S", 2.0, 0.9, 0.7, "middle");
        }
        "selfActing" => {
            d.line(&[(2.0, MID), (2.0, 1.4)]);
            d.line(&[(1.2, 1.4), (2.8, 1.4)]);
            d.arc(2.0, 1.4, 0.8, 180.0, 360.0);
            // The sensing line to the downstream pipe.
            d.line(&[(2.8, 1.0), (3.6, 1.0), (3.6, TOP - 0.2)]);
        }
        _ => {}
    }
}

/// End joint marks at the flow ends of the body.
fn ends(d: &mut Draw, kind: &str, body: &str) {
    let mut at: Vec<(f64, f64, bool)> = Vec::new();
    if !matches!(body, "safety") {
        at.push((0.0, MID, true));
    }
    if !matches!(body, "angle") {
        at.push((4.0, MID, true));
    }
    if matches!(body, "angle" | "threeWay" | "safety") {
        at.push((2.0, 5.0, false));
    }
    for (x, y, across) in at {
        let inward = if x > 2.0 { -1.0 } else { 1.0 };
        match kind {
            "flanged" if across => d.line(&[(x + inward * 0.25, y - 1.0), (x + inward * 0.25, y + 1.0)]),
            "flanged" => d.line(&[(x - 1.0, y - 0.25), (x + 1.0, y - 0.25)]),
            "welded" => d.circle(x, y, 0.15),
            "socketWelded" if across => d.line(&[(x + inward * 0.4, y - 0.5), (x, y - 0.5), (x, y + 0.5), (x + inward * 0.4, y + 0.5)]),
            "socketWelded" => d.line(&[(x - 0.5, y - 0.4), (x - 0.5, y), (x + 0.5, y), (x + 0.5, y - 0.4)]),
            _ => {}
        }
    }
}
