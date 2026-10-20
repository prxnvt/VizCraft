//! Switchgear builder: fuses, circuit breakers, disconnectors and overload relays, one to four
//! poles, as they are drawn in power circuits.
//!
//! Provenance: own design, drawn from the written rules of IEC 60617-7 and of the IEEE 315 and JIC power circuit convention. IEC:
//! a fuse is a narrow rectangle with the line running through it, a circuit breaker a switch
//! blade with a cross on its fixed contact, a disconnector a blade with a short bar on its fixed
//! contact, the thermal element of an overload relay a rectangle with a step inside. IEEE 315 and
//! JIC: a fuse is a lozenge on the line, a breaker a blade with an arc beside it, a disconnect a
//! plain blade, an overload heater a loop. Ganged poles are joined by a dashed linkage. Terminals
//! count down the poles in pairs, odd above and even below, as IEC 60445 numbers main circuit
//! terminals.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const STANDARDS: &[&str] = &["iec60617", "jic", "ieee315"];

const SOURCE: &str = "own design, drawn from the written rules of IEC 60617-7 and of the IEEE 315 and JIC power circuit convention";

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "switchgear",
    label: "Switchgear",
    description: "A fuse, circuit breaker, disconnector or overload relay element in a power circuit, one to four poles. Ports per pole: an odd number above and the next even number below (1 and 2, 3 and 4).",
    params: &[
        ParamSpec {
            key: "device",
            label: "Device",
            kind: ParamKind::Choice(&["fuse", "breaker", "disconnect", "overload"]),
            default: Def::Text("fuse"),
            description: "Which protective or switching device it is.",
        },
        ParamSpec { key: "poles", label: "Poles", kind: ParamKind::Int { min: 1, max: 4 }, default: Def::Int(1), description: "Ganged poles." },
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(STANDARDS),
            default: Def::Text("iec60617"),
            description: "Which symbol set draws it.",
        },
        ParamSpec { key: "tag", label: "Tag", kind: ParamKind::Text { max: 16 }, default: Def::Text(""), description: "Device tag (F1, Q1, CB1)." },
    ],
    sources: &[("IEC 60617", SOURCE), ("JIC", SOURCE), ("IEEE 315", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    let std = STANDARDS.iter().copied().find(|s| *s == p.text("standard")).unwrap_or("iec60617");
    d.standard(std);
    let device = p.text("device");
    let poles = p.int("poles").clamp(1, 4);
    let iec = std == "iec60617";
    // Blades swing to the left of their pole, so the first pole stands two modules in.
    let left = 2.0;
    let w = left + 2.0 * (poles as f64 - 1.0) + 1.0;
    d.size(w, 6.0);
    let mut mids = Vec::new();
    for k in 0..poles {
        let cx = left + 2.0 * k as f64;
        match device {
            "breaker" | "disconnect" => {
                d.line(&[(cx, 0.0), (cx, 2.0)]);
                d.line(&[(cx, 6.0), (cx, 4.0)]);
                d.line(&[(cx, 4.0), (cx - 1.0, 2.3)]);
                mids.push((cx - 0.5, 3.15));
                match (device, iec) {
                    ("breaker", true) => {
                        d.line(&[(cx - 0.35, 1.65), (cx + 0.35, 2.35)]);
                        d.line(&[(cx - 0.35, 2.35), (cx + 0.35, 1.65)]);
                    }
                    ("breaker", false) => d.arc(cx + 0.3, 3.1, 1.3, 125.0, 235.0),
                    (_, true) => d.line(&[(cx - 0.4, 2.0), (cx + 0.4, 2.0)]),
                    _ => d.ring(cx, 2.0, 0.15),
                }
            }
            "overload" => {
                if iec {
                    d.line(&[(cx, 0.0), (cx, 1.5)]);
                    d.line(&[(cx, 4.5), (cx, 6.0)]);
                    d.rect(cx - 0.6, 1.5, cx + 0.6, 4.5);
                    d.line(&[(cx, 1.5), (cx, 2.4), (cx - 0.35, 2.4), (cx - 0.35, 3.6), (cx, 3.6), (cx, 4.5)]);
                } else {
                    d.line(&[(cx, 0.0), (cx, 2.0)]);
                    d.line(&[(cx, 4.0), (cx, 6.0)]);
                    d.arc(cx, 3.0, 1.0, 270.0, 450.0);
                }
            }
            _ => {
                if iec {
                    d.line(&[(cx, 0.0), (cx, 6.0)]);
                    d.rect(cx - 0.4, 1.5, cx + 0.4, 4.5);
                    d.line(&[(cx, 1.5), (cx, 4.5)]);
                } else {
                    d.line(&[(cx, 0.0), (cx, 1.5)]);
                    d.line(&[(cx, 4.5), (cx, 6.0)]);
                    d.shape(&[(cx, 1.5), (cx + 0.5, 3.0), (cx, 4.5), (cx - 0.5, 3.0)]);
                }
            }
        }
        let (a, b) = (2 * k + 1, 2 * k + 2);
        d.port(&a.to_string(), cx, 0.0, "up");
        d.port(&b.to_string(), cx, 6.0, "down");
        d.label(&format!("t{a}"), &a.to_string(), cx + 0.3, 0.5, 0.8, "start");
        d.label(&format!("t{b}"), &b.to_string(), cx + 0.3, 5.5, 0.8, "start");
    }
    if let (Some(first), Some(last)) = (mids.first().copied(), mids.last().copied())
        && poles > 1
    {
        d.dashed(first, last, 0.3, 0.2);
    }
    d.label("tag", p.text("tag"), w + 0.4, 3.0, 1.2, "start");
}
