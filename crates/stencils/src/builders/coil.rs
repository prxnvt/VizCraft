//! Coil builder: the operating coil of a relay, contactor or timer, or a solenoid actuator.
//!
//! Provenance: own design, drawn from the written rules of IEC 60617-7 and of the JIC and IEEE 315
//! ladder convention. IEC: an operating device is a rectangle between its two terminals, a
//! delayed one carries an extra compartment with a cross, a solenoid actuator a diagonal. JIC and
//! IEEE 315: a circle on the rung holding the device letters. Terminals are A1 and A2 as IEC 61346 and EN 50005 practice
//! names coil terminals.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const STANDARDS: &[&str] = &["iec60617", "jic", "ieee315"];

const SOURCE: &str = "own design, drawn from the written rules of IEC 60617-7 and of the JIC and IEEE 315 ladder convention";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "coil",
    label: "Coil",
    description: "The operating coil of a relay, contactor or timer, or a solenoid. IEC 60617 draws a rectangle between A1 and A2; JIC and IEEE 315 a circle on the rung with the device letters or tag inside.",
    params: &[
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(STANDARDS),
            default: Def::Text("iec60617"),
            description: "Which symbol set draws it.",
        },
        ParamSpec {
            key: "kind",
            label: "Kind",
            kind: ParamKind::Choice(&["relay", "contactor", "timer", "solenoid"]),
            default: Def::Text("relay"),
            description: "What the coil operates.",
        },
        ParamSpec { key: "tag", label: "Tag", kind: ParamKind::Text { max: 16 }, default: Def::Text(""), description: "Device tag (K1, CR1)." },
    ],
    sources: &[("IEC 60617", SOURCE), ("JIC", SOURCE), ("IEEE 315", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    let std = STANDARDS.iter().copied().find(|s| *s == p.text("standard")).unwrap_or("iec60617");
    d.standard(std);
    let kind = p.text("kind");
    let tag = p.text("tag");
    if std == "iec60617" {
        d.size(4.0, 4.0);
        d.line(&[(2.0, 0.0), (2.0, 1.0)]);
        d.line(&[(2.0, 3.0), (2.0, 4.0)]);
        d.rect(0.75, 1.0, 3.25, 3.0);
        match kind {
            "timer" => {
                d.line(&[(1.6, 1.0), (1.6, 3.0)]);
                d.line(&[(0.75, 1.0), (1.6, 3.0)]);
                d.line(&[(0.75, 3.0), (1.6, 1.0)]);
            }
            "solenoid" => d.line(&[(0.75, 3.0), (3.25, 1.0)]),
            _ => {}
        }
        d.port("A1", 2.0, 0.0, "up");
        d.port("A2", 2.0, 4.0, "down");
        d.label("tA1", "A1", 2.3, 0.5, 0.8, "start");
        d.label("tA2", "A2", 2.3, 3.5, 0.8, "start");
        d.label("tag", tag, 3.7, 2.0, 1.2, "start");
    } else {
        d.size(4.0, 2.0);
        d.line(&[(0.0, 1.0), (1.2, 1.0)]);
        d.line(&[(2.8, 1.0), (4.0, 1.0)]);
        d.circle(2.0, 1.0, 0.8);
        let letters = match kind {
            "contactor" => "M",
            "timer" => "TR",
            "solenoid" => "SOL",
            _ => "CR",
        };
        let text = if tag.trim().is_empty() { letters } else { tag };
        let size = if text.chars().count() > 3 { 0.45 } else { 0.6 };
        d.label("tag", text, 2.0, 1.0, size, "middle");
        d.port("A1", 0.0, 1.0, "left");
        d.port("A2", 4.0, 1.0, "right");
        d.label("tA1", "A1", 0.5, 0.45, 0.5, "middle");
        d.label("tA2", "A2", 3.5, 0.45, 0.5, "middle");
    }
}
