//! Fluid power accessory builder: the ISO 1219 symbols that are not valves, pumps or cylinders.
//!
//! Provenance: own design, drawn from the written rules of ISO 1219-1. A reservoir is an open
//! topped box with the lines entering from above; an accumulator an upright capsule split across
//! the middle; a filter a square on its corner with a dashed element across the flow, a cooler
//! the same square with outward arrows; a check valve a ball before a V seat; a throttle two arcs
//! pinching the line, a flow control the throttle with a check valve bypass; a shut-off valve two
//! triangles tip to tip; a pressure gauge a circle with a pointer; a pressure switch a box with a
//! contact; a pressure source a circle with the flow triangle (solid for hydraulic, hollow for
//! pneumatic); a service unit a box holding a filter square; a silencer a small box with
//! baffles; an exhaust an open triangle; an electric motor a circle with the letter M drawn in
//! strokes; a limit switch a roller on a lever over a contact. Fluid ports are numbered `1`, `2`
//! (a reservoir's `T1` to `T3`), the shaft port is `shaft`, electrical ports `el` and mechanical
//! ones `ml`.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ISO 1219-1";

/// The accessories drawn.
pub const KINDS: &[&str] = &[
    "reservoir",
    "accumulator",
    "filter",
    "cooler",
    "checkValve",
    "throttle",
    "flowControl",
    "shutOff",
    "gauge",
    "pressureSwitch",
    "source",
    "serviceUnit",
    "silencer",
    "exhaust",
    "motor",
    "limitSwitch",
];

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "fluidPart",
    label: "Fluid Power Accessory",
    description: "An ISO 1219 accessory or energy part by `kind`: reservoir (ports T1 to T3), accumulator, filter, cooler, check valve (1 to 2 only), throttle, one-way flow control, shut-off valve, pressure gauge, pressure switch (1, el), pressure source, service unit, silencer, exhaust, electric motor (shaft) or limit switch (ml, el).",
    params: &[
        ParamSpec { key: "kind", label: "Kind", kind: ParamKind::Choice(KINDS), default: Def::Text("filter"), description: "Which accessory." },
        ParamSpec {
            key: "medium",
            label: "Medium",
            kind: ParamKind::Choice(&["hydraulic", "pneumatic"]),
            default: Def::Text("hydraulic"),
            description: "A pressure source's triangle is solid for hydraulic, hollow for pneumatic.",
        },
        ParamSpec {
            key: "tag",
            label: "Code",
            kind: ParamKind::Text { max: 32 },
            default: Def::Text(""),
            description: "The component code written beside the symbol (the element's tag).",
        },
    ],
    sources: &[("ISO 1219", SOURCE)],
    build,
};

/// A two-port part's flow line from the left port to `a` and from `b` to the right port.
fn through(d: &mut Draw, w: f64, y: f64, a: f64, b: f64) {
    d.line(&[(0.0, y), (a, y)]);
    d.line(&[(b, y), (w, y)]);
    d.port("1", 0.0, y, "left");
    d.port("2", w, y, "right");
}

/// A square on its corner centred at `(cx, cy)`, `r` from the centre to each corner.
fn diamond(d: &mut Draw, cx: f64, cy: f64, r: f64) {
    d.shape(&[(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)]);
}

/// Two arcs pinching a line at `cx`, and an adjusting arrow across.
fn throttle(d: &mut Draw, cx: f64, cy: f64, arrow: bool) {
    d.arc(cx, cy - 1.7, 1.4, 50.0, 130.0);
    d.arc(cx, cy + 1.7, 1.4, 230.0, 310.0);
    if arrow {
        d.line(&[(cx - 1.4, cy + 1.4), (cx + 1.2, cy - 1.2)]);
        d.head((cx - 1.4, cy + 1.4), (cx + 1.4, cy - 1.4), 0.5, 0.2, true);
    }
}

/// A check valve on a horizontal line: the V seat opening right at `x`, the ball in it.
fn check(d: &mut Draw, x: f64, y: f64) {
    d.line(&[(x + 1.0, y - 1.0), (x, y), (x + 1.0, y + 1.0)]);
    d.circle(x + 1.0, y, 0.7);
}

fn build(p: &Params, d: &mut Draw) {
    d.standard("iso1219");
    let pneumatic = p.text("medium") == "pneumatic";
    let kind = p.text("kind");
    let w = match kind {
        "reservoir" | "flowControl" => 8.0,
        "accumulator" | "gauge" | "source" | "silencer" | "exhaust" => 4.0,
        _ => 6.0,
    };
    let h = match kind {
        "accumulator" => 8.0,
        "flowControl" | "gauge" | "pressureSwitch" | "motor" => 6.0,
        _ => 4.0,
    };
    d.size(w, h);
    match kind {
        "reservoir" => {
            d.line(&[(0.5, 1.0), (0.5, 4.0), (7.5, 4.0), (7.5, 1.0)]);
            for (i, x) in [2.0, 4.0, 6.0].into_iter().enumerate() {
                d.line(&[(x, 0.0), (x, 3.0)]);
                d.port(&format!("T{}", i + 1), x, 0.0, "up");
            }
        }
        "accumulator" => {
            let mut pts = Vec::new();
            for k in 0..=12 {
                let a = (180.0 + 15.0 * k as f64).to_radians();
                pts.push((2.0 + 1.5 * a.cos(), 3.0 + 1.5 * a.sin()));
            }
            for k in 0..=12 {
                let a = (15.0 * k as f64).to_radians();
                pts.push((2.0 + 1.5 * a.cos(), 5.0 + 1.5 * a.sin()));
            }
            d.shape(&pts);
            d.line(&[(0.5, 4.0), (3.5, 4.0)]);
            d.line(&[(2.0, 6.5), (2.0, 8.0)]);
            d.port("1", 2.0, 8.0, "down");
        }
        "filter" | "cooler" => {
            through(d, w, 2.0, 1.0, 5.0);
            diamond(d, 3.0, 2.0, 2.0);
            if kind == "filter" {
                d.dashed((3.0, 0.3), (3.0, 3.7), 0.3, 0.2);
            } else {
                d.line(&[(1.6, 2.0), (4.4, 2.0)]);
                d.head((3.0, 2.0), (3.0, 0.4), 0.5, 0.25, true);
                d.head((3.0, 2.0), (3.0, 3.6), 0.5, 0.25, true);
            }
        }
        "checkValve" => {
            through(d, w, 2.0, 2.0, 3.7);
            check(d, 2.0, 2.0);
        }
        "throttle" => {
            through(d, w, 2.0, 3.0, 3.0);
            throttle(d, 3.0, 2.0, true);
        }
        "flowControl" => {
            through(d, w, 2.0, 4.0, 4.0);
            throttle(d, 4.0, 2.0, true);
            d.line(&[(1.5, 2.0), (1.5, 4.5), (3.2, 4.5)]);
            d.line(&[(4.9, 4.5), (6.5, 4.5), (6.5, 2.0)]);
            check(d, 3.2, 4.5);
        }
        "shutOff" => {
            through(d, w, 2.0, 1.0, 5.0);
            d.shape(&[(1.0, 0.9), (3.0, 2.0), (1.0, 3.1)]);
            d.shape(&[(5.0, 0.9), (3.0, 2.0), (5.0, 3.1)]);
        }
        "gauge" => {
            d.circle(2.0, 2.0, 1.6);
            d.line(&[(1.0, 3.0), (2.8, 1.2)]);
            d.head((1.0, 3.0), (3.0, 1.0), 0.5, 0.25, true);
            d.line(&[(2.0, 3.6), (2.0, 6.0)]);
            d.port("1", 2.0, 6.0, "down");
        }
        "pressureSwitch" => {
            d.rect(1.0, 0.5, 5.0, 3.5);
            d.line(&[(2.0, 2.6), (4.2, 1.4)]);
            d.line(&[(4.2, 2.6), (4.2, 1.9)]);
            d.line(&[(3.0, 3.5), (3.0, 6.0)]);
            d.port("1", 3.0, 6.0, "down");
            d.line(&[(5.0, 2.0), (6.0, 2.0)]);
            d.port("el", 6.0, 2.0, "right");
        }
        "source" => {
            d.circle(2.0, 2.5, 1.5);
            d.head((2.0, 2.9), (2.0, 1.0), 1.0, 0.55, !pneumatic);
            d.line(&[(2.0, 1.0), (2.0, 0.0)]);
            d.port("1", 2.0, 0.0, "up");
        }
        "serviceUnit" => {
            through(d, w, 2.0, 1.0, 5.0);
            d.rect(1.0, 0.5, 5.0, 3.5);
            d.line(&[(3.0, 0.9), (3.9, 2.0), (3.0, 3.1), (2.1, 2.0), (3.0, 0.9)]);
            d.dashed((2.2, 2.0), (3.8, 2.0), 0.25, 0.15);
        }
        "silencer" => {
            d.rect(1.0, 1.5, 3.0, 3.8);
            for y in [2.2, 2.9] {
                d.line(&[(1.0, y), (3.0, y)]);
            }
            d.line(&[(2.0, 0.0), (2.0, 1.5)]);
            d.port("1", 2.0, 0.0, "up");
        }
        "exhaust" => {
            d.line(&[(2.0, 0.0), (2.0, 1.4)]);
            d.head((2.0, 1.4), (2.0, 3.0), 1.4, 0.8, false);
            d.port("1", 2.0, 0.0, "up");
        }
        "motor" => {
            d.circle(3.0, 3.0, 2.4);
            d.line(&[(2.0, 4.0), (2.0, 2.0), (3.0, 3.2), (4.0, 2.0), (4.0, 4.0)]);
            d.line(&[(5.4, 2.8), (6.0, 2.8)]);
            d.line(&[(5.4, 3.2), (6.0, 3.2)]);
            d.port("shaft", 6.0, 3.0, "right");
        }
        _ => {
            // A limit switch: a roller on a lever over a normally open contact.
            d.ring(1.0, 2.0, 0.6);
            d.line(&[(1.6, 2.0), (2.4, 2.0), (4.2, 1.0)]);
            d.line(&[(4.2, 2.0), (6.0, 2.0)]);
            d.line(&[(0.0, 2.0), (0.4, 2.0)]);
            d.port("ml", 0.0, 2.0, "left");
            d.port("el", 6.0, 2.0, "right");
        }
    }
    d.label("tag", p.text("tag"), w + 0.3, 0.4, 0.8, "start");
}
