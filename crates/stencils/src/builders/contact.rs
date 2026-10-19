//! Contact builder: make and break contacts, one to six poles, plain or time delayed.
//!
//! Provenance: own design, drawn from the written rules of IEC 60617-7 and of the JIC and IEEE 315
//! ladder convention. IEC: a contact drawn upright with a blade swinging from the lower terminal,
//! a break contact touching a short tick on the upper one, ganged poles joined by a dashed
//! linkage, a delay shown by an arc on the operating stem. JIC and IEEE 315: two short parallel
//! plates across the line, a break contact crossed by a slash. Terminal numbers follow the IEC 61346 and EN 50005
//! practice: a sequence digit per pole, then 3 and 4 for a make contact, 1 and 2 for a break.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const STANDARDS: &[&str] = &["iec60617", "jic", "ieee315"];

const SOURCE: &str = "own design, drawn from the written rules of IEC 60617-7 and of the JIC and IEEE 315 ladder convention";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "contact",
    label: "Contact",
    description: "A relay or contactor contact: make (no) or break (nc), plain or on or off delayed, one to six ganged poles. IEC 60617 draws it upright with a swinging blade; JIC and IEEE 315 draw it across a ladder rung as two plates. Ports per pole: 13 and 14 for a make contact, 11 and 12 for a break, the first digit counting the poles.",
    params: &[
        ParamSpec {
            key: "form",
            label: "Form",
            kind: ParamKind::Choice(&["no", "nc"]),
            default: Def::Text("no"),
            description: "Make (normally open) or break (normally closed).",
        },
        ParamSpec {
            key: "timing",
            label: "Timing",
            kind: ParamKind::Choice(&["none", "onDelay", "offDelay"]),
            default: Def::Text("none"),
            description: "Delayed when the operating device is energised (onDelay) or released (offDelay).",
        },
        ParamSpec { key: "poles", label: "Poles", kind: ParamKind::Int { min: 1, max: 6 }, default: Def::Int(1), description: "Ganged poles." },
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

fn build(p: &Params, d: &mut Draw) {
    let nc = p.text("form") == "nc";
    let timing = p.text("timing");
    let poles = p.int("poles").clamp(1, 6);
    let std = standard(p);
    d.standard(std);
    let (a, b) = if nc { ("1", "2") } else { ("3", "4") };
    if std == "iec60617" {
        // Upright: one column per pole, two modules apart; a delay mark needs a module more on
        // the left.
        let left = if timing == "none" { 1.0 } else { 2.0 };
        let w = left + 2.0 * poles as f64 - 1.0;
        d.size(w, 4.0);
        let mut mids = Vec::new();
        for k in 0..poles {
            let cx = left + 2.0 * k as f64;
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
            let n = k + 1;
            d.port(&format!("{n}{a}"), cx, 0.0, "up");
            d.port(&format!("{n}{b}"), cx, 4.0, "down");
            d.label(&format!("t{n}{a}"), &format!("{n}{a}"), cx + 0.3, 0.5, 0.8, "start");
            d.label(&format!("t{n}{b}"), &format!("{n}{b}"), cx + 0.3, 3.5, 0.8, "start");
        }
        if let (Some(first), Some(last)) = (mids.first().copied(), mids.last().copied()) {
            if poles > 1 {
                d.dashed(first, last, 0.3, 0.2);
            }
            if timing != "none" {
                // The operating stem runs left from the first blade to the delay arc.
                let end = (first.0 - 1.2, first.1);
                d.line(&[end, first]);
                let (a0, a1) = if timing == "onDelay" { (90.0, 270.0) } else { (-90.0, 90.0) };
                d.arc(end.0, end.1, 0.4, a0, a1);
            }
        }
        d.label("tag", p.text("tag"), w + 0.4, 2.0, 1.2, "start");
    } else {
        // Across a rung: one row per pole, two modules apart; a delay mark needs a module more
        // on top.
        let top = if timing == "none" { 1.0 } else { 2.0 };
        let h = top + 2.0 * poles as f64 - 1.0;
        d.size(4.0, h);
        for k in 0..poles {
            let cy = top + 2.0 * k as f64;
            d.line(&[(0.0, cy), (1.6, cy)]);
            d.line(&[(2.4, cy), (4.0, cy)]);
            d.line(&[(1.6, cy - 0.7), (1.6, cy + 0.7)]);
            d.line(&[(2.4, cy - 0.7), (2.4, cy + 0.7)]);
            if nc {
                d.line(&[(1.2, cy + 0.9), (2.8, cy - 0.9)]);
            }
            let n = k + 1;
            d.port(&format!("{n}{a}"), 0.0, cy, "left");
            d.port(&format!("{n}{b}"), 4.0, cy, "right");
            d.label(&format!("t{n}{a}"), &format!("{n}{a}"), 0.6, cy - 0.45, 0.6, "middle");
            d.label(&format!("t{n}{b}"), &format!("{n}{b}"), 3.4, cy - 0.45, 0.6, "middle");
        }
        if poles > 1 {
            d.dashed((2.0, top + 0.7), (2.0, h - 0.7), 0.3, 0.2);
        }
        if timing != "none" {
            d.line(&[(2.0, top - 0.7), (2.0, top - 1.4)]);
            let (a0, a1) = if timing == "onDelay" { (180.0, 360.0) } else { (0.0, 180.0) };
            d.arc(2.0, top - 1.4, 0.4, a0, a1);
        }
        d.label("tag", p.text("tag"), 4.4, top, 1.0, "start");
    }
}
