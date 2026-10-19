//! Instrument bubble builder: the ISA-5.1 instrument and function symbol with its tag.
//!
//! Provenance: own design, drawn from the written rules of ANSI/ISA-5.1. The shape names the
//! system: a circle for a discrete instrument, a circle in a square for a shared display or
//! control, a hexagon for a computer function, a diamond in a square for a safety instrumented
//! function. A line through the middle names the location: none in the field, solid for the
//! front of a main panel, dashed for its rear, doubled for a local or auxiliary panel. The
//! functional letters sit above the line and the loop number below.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ANSI/ISA-5.1";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "bubble",
    label: "Instrument",
    description: "An ISA-5.1 instrument bubble: the system (discrete, shared display, computer or safety) sets the outline, the location (field, panel front or rear, local panel front or rear) the line through it, and the functional letters and loop number are drawn inside.",
    params: &[
        ParamSpec {
            key: "system",
            label: "System",
            kind: ParamKind::Choice(&["discrete", "sharedDisplay", "computer", "safety"]),
            default: Def::Text("discrete"),
            description: "Which kind of device performs the function.",
        },
        ParamSpec {
            key: "location",
            label: "Location",
            kind: ParamKind::Choice(&["field", "panelFront", "panelRear", "localFront", "localRear"]),
            default: Def::Text("field"),
            description: "Where the operator reaches it.",
        },
        ParamSpec {
            key: "letters",
            label: "Letters",
            kind: ParamKind::Text { max: 6 },
            default: Def::Text("TI"),
            description: "Functional identification letters (TIC, PT).",
        },
        ParamSpec { key: "number", label: "Number", kind: ParamKind::Text { max: 8 }, default: Def::Text("100"), description: "Loop number." },
    ],
    sources: &[("ANSI/ISA-5.1", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    d.standard("isa5_1");
    d.size(4.0, 4.0);
    // The body fills the frame, so the ports at the middle of each side touch it: a square for
    // shared display and safety, a hexagon for computer, and the circle inside or alone.
    let (c, r) = (2.0, 2.0);
    let half = if matches!(p.text("system"), "sharedDisplay" | "safety") { r } else { 0.0 };
    match p.text("system") {
        "sharedDisplay" => {
            d.rect(c - half, c - half, c + half, c + half);
            d.ring(c, c, r);
        }
        "safety" => {
            d.rect(c - half, c - half, c + half, c + half);
            d.line(&[(c, c - r), (c + r, c), (c, c + r), (c - r, c), (c, c - r)]);
        }
        "computer" => {
            let a = r * 0.5;
            d.shape(&[(c - r, c), (c - a, c - r), (c + a, c - r), (c + r, c), (c + a, c + r), (c - a, c + r)]);
        }
        _ => d.circle(c, c, r),
    }
    // The location line spans the body at mid height.
    let (x0, x1) = (c - r, c + r);
    match p.text("location") {
        "panelFront" => d.line(&[(x0, c), (x1, c)]),
        "panelRear" => d.dashed((x0, c), (x1, c), 0.3, 0.2),
        "localFront" => {
            d.line(&[(x0 + 0.05, c - 0.1), (x1 - 0.05, c - 0.1)]);
            d.line(&[(x0 + 0.05, c + 0.1), (x1 - 0.05, c + 0.1)]);
        }
        "localRear" => {
            d.dashed((x0 + 0.05, c - 0.1), (x1 - 0.05, c - 0.1), 0.3, 0.2);
            d.dashed((x0 + 0.05, c + 0.1), (x1 - 0.05, c + 0.1), 0.3, 0.2);
        }
        _ => {}
    }
    let letters = p.text("letters");
    let number = p.text("number");
    // The text shrinks to fit the body: a diamond leaves the least room across.
    let room = if p.text("system") == "safety" { 1.8 } else { 2.6 };
    let fit = |n: usize| if n == 0 { 0.7 } else { (room / (n as f64 * 0.6)).clamp(0.3, 0.7) };
    d.label("letters", letters, c, c - 0.5, fit(letters.chars().count()), "middle");
    d.label("number", number, c, c + 0.55, fit(number.chars().count()), "middle");
    d.port("n", c, 0.0, "up");
    d.port("e", 4.0, c, "right");
    d.port("s", c, 4.0, "down");
    d.port("w", 0.0, c, "left");
}
