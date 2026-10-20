//! Instrument bubble builder: the ISA-5.1 instrument and function symbol with its tag.
//!
//! Provenance: own design, drawn from the written rules of ANSI/ISA-5.1. The shape names the
//! system: a circle for a discrete instrument, a circle in a square for a shared display or
//! control, a hexagon for a computer function, a diamond in a square for a safety instrumented
//! function. A line through the middle names the location: none in the field, solid for the
//! front of a main panel, dashed for its rear, doubled for a local or auxiliary panel. The
//! functional letters sit above the line and the loop number below.
//!
//! Provenance: own design, drawn from the written rules of IEC 62424 for the oval. With
//! `standard` `iec62424` the symbol is the IEC 62424 process control equipment request
//! drawn on ISO 10628 diagrams: an oval, wider than tall, in the same frame and with the same
//! ports, so a page switched between the standards keeps every connection. A `tag` (`LIC-101`)
//! is split into its letters above the line and its number below, so the element's composed tag
//! shows with no extra pass.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written rules of ANSI/ISA-5.1";

const SOURCE_IEC: &str = "own design, drawn from the written rules of IEC 62424";

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
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(&["isa5_1", "iec62424"]),
            default: Def::Text("isa5_1"),
            description: "ISA-5.1 bubble, or the IEC 62424 oval of ISO 10628 diagrams.",
        },
        ParamSpec {
            key: "tag",
            label: "Tag",
            kind: ParamKind::Text { max: 24 },
            default: Def::Text(""),
            description: "The whole tag (`LIC-101`); when set, its letters and number are drawn instead of Letters and Number.",
        },
    ],
    sources: &[("ANSI/ISA-5.1", SOURCE), ("IEC 62424", SOURCE_IEC)],
    build,
};

/// The letters and the number a tag shows: split at its first `-`, else after its letters.
pub fn split_tag(tag: &str) -> (String, String) {
    let t = tag.trim();
    match t.split_once('-') {
        Some((a, b)) => (a.trim().to_string(), b.trim().to_string()),
        None => {
            let letters: String = t.chars().take_while(|c| c.is_alphabetic()).collect();
            let rest = t.get(letters.len()..).unwrap_or_default().trim().to_string();
            (letters, rest)
        }
    }
}

fn build(p: &Params, d: &mut Draw) {
    if p.text("standard") == "iec62424" {
        oval(p, d);
        return;
    }
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
    let (letters, number) = texts(p);
    let (letters, number) = (letters.as_str(), number.as_str());
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

/// The letters and number drawn: the tag's when one is given, else the two params.
fn texts(p: &Params) -> (String, String) {
    let tag = p.text("tag");
    if tag.trim().is_empty() { (p.text("letters").to_string(), p.text("number").to_string()) } else { split_tag(tag) }
}

/// The IEC 62424 request: an oval in the bubble's frame, the location line across it, the
/// letters above and the number below, the ports where the bubble has them.
fn oval(p: &Params, d: &mut Draw) {
    d.standard("iec62424");
    d.size(4.0, 4.0);
    let (c, rx, ry) = (2.0, 2.0, 1.4);
    // Four cubic quarters of an ellipse, closed and filled.
    d.ellipse(c, c, rx, ry);
    let (x0, x1) = (c - rx, c + rx);
    match p.text("location") {
        "panelFront" => d.line(&[(x0, c), (x1, c)]),
        "panelRear" => d.dashed((x0, c), (x1, c), 0.3, 0.2),
        "localFront" => {
            d.line(&[(x0 + 0.1, c - 0.1), (x1 - 0.1, c - 0.1)]);
            d.line(&[(x0 + 0.1, c + 0.1), (x1 - 0.1, c + 0.1)]);
        }
        "localRear" => {
            d.dashed((x0 + 0.1, c - 0.1), (x1 - 0.1, c - 0.1), 0.3, 0.2);
            d.dashed((x0 + 0.1, c + 0.1), (x1 - 0.1, c + 0.1), 0.3, 0.2);
        }
        _ => {}
    }
    // Stubs from the oval to the ports above and below it.
    d.line(&[(c, 0.0), (c, c - ry)]);
    d.line(&[(c, c + ry), (c, 4.0)]);
    let (letters, number) = texts(p);
    let fit = |n: usize| if n == 0 { 0.6 } else { (3.0 / (n as f64 * 0.6)).clamp(0.3, 0.6) };
    d.label("letters", &letters, c, c - 0.45, fit(letters.chars().count()), "middle");
    d.label("number", &number, c, c + 0.5, fit(number.chars().count()), "middle");
    d.port("n", c, 0.0, "up");
    d.port("e", 4.0, c, "right");
    d.port("s", c, 4.0, "down");
    d.port("w", 0.0, c, "left");
}
