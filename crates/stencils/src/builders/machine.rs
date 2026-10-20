//! Machine builder: motors, generators, the utility supply and transformers (power, current and
//! voltage), for power and one-line diagrams.
//!
//! Provenance: own design, drawn from the written rules of IEC 60617-6 and of IEEE 315 for rotating machines and transformers. A
//! rotating machine is a circle holding M or G with a tilde for alternating current or a bar for
//! direct current; a transformer two touching circles, one per winding; a current transformer a
//! ring threaded by the line it measures, with a dot at the polarity mark; a voltage transformer
//! two small circles; the utility supply a box with a wave. Three-phase machine terminals are U1,
//! V1 and W1 as IEC 60034-8 names them.

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const STANDARDS: &[&str] = &["iec60617", "ieee315"];

const SOURCE: &str = "own design, drawn from the written rules of IEC 60617-6 and of IEEE 315 for rotating machines and transformers";

/// What the builder draws.
const ROLES: &[&str] = &["motor", "generator", "utility", "transformer", "currentTransformer", "voltageTransformer"];

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "machine",
    label: "Machine",
    description: "A motor, generator, utility supply or transformer. Ports: U1, V1 and W1 for a three-phase machine, 1 and 2 for a single-phase or direct-current one, 1 for the utility, 1 (primary) and 2 (secondary) for a power or voltage transformer, P1, P2, S1 and S2 for a current transformer.",
    params: &[
        ParamSpec { key: "role", label: "Role", kind: ParamKind::Choice(ROLES), default: Def::Text("motor"), description: "What it is." },
        ParamSpec {
            key: "current",
            label: "Current",
            kind: ParamKind::Choice(&["ac", "dc"]),
            default: Def::Text("ac"),
            description: "Alternating or direct current, for a machine.",
        },
        ParamSpec {
            key: "phases",
            label: "Phases",
            kind: ParamKind::Int { min: 1, max: 3 },
            default: Def::Int(3),
            description: "1 or 3, for an AC machine.",
        },
        ParamSpec {
            key: "standard",
            label: "Standard",
            kind: ParamKind::Choice(STANDARDS),
            default: Def::Text("iec60617"),
            description: "Which symbol set draws it.",
        },
        ParamSpec {
            key: "polarity",
            label: "Polarity mark",
            kind: ParamKind::Bool,
            default: Def::Bool(true),
            description: "A dot at the polarity mark of a current transformer.",
        },
        ParamSpec { key: "tag", label: "Tag", kind: ParamKind::Text { max: 16 }, default: Def::Text(""), description: "Device tag (M1, T1)." },
    ],
    sources: &[("IEC 60617", SOURCE), ("IEEE 315", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    let std = STANDARDS.iter().copied().find(|s| *s == p.text("standard")).unwrap_or("iec60617");
    d.standard(std);
    let role = ROLES.iter().copied().find(|r| *r == p.text("role")).unwrap_or("motor");
    let tag = p.text("tag");
    match role {
        "utility" => {
            d.size(6.0, 6.0);
            d.rect(1.0, 0.5, 5.0, 4.0);
            d.arc(2.25, 2.25, 0.75, 180.0, 360.0);
            d.arc(3.75, 2.25, 0.75, 0.0, 180.0);
            d.line(&[(3.0, 4.0), (3.0, 6.0)]);
            d.port("1", 3.0, 6.0, "down");
            d.label("tag", tag, 5.4, 2.25, 1.0, "start");
        }
        "transformer" | "voltageTransformer" => {
            let small = role == "voltageTransformer";
            let (h, r) = if small { (6.0, 1.0) } else { (8.0, 1.4) };
            d.size(4.0, h);
            let (c1, c2) = (h / 2.0 - r * 0.7, h / 2.0 + r * 0.7);
            d.circle(2.0, c1, r);
            d.circle(2.0, c2, r);
            d.line(&[(2.0, 0.0), (2.0, c1 - r)]);
            d.line(&[(2.0, c2 + r), (2.0, h)]);
            d.port("1", 2.0, 0.0, "up");
            d.port("2", 2.0, h, "down");
            d.label("tag", tag, 3.8, h / 2.0, 1.0, "start");
        }
        "currentTransformer" => {
            d.size(4.0, 6.0);
            d.line(&[(2.0, 0.0), (2.0, 6.0)]);
            d.ring(2.0, 3.0, 1.0);
            d.line(&[(2.9, 2.6), (4.0, 2.0)]);
            d.line(&[(2.9, 3.4), (4.0, 4.0)]);
            if p.flag("polarity") {
                d.circle(1.2, 1.7, 0.15);
            }
            d.port("P1", 2.0, 0.0, "up");
            d.port("P2", 2.0, 6.0, "down");
            d.port("S1", 4.0, 2.0, "right");
            d.port("S2", 4.0, 4.0, "right");
            d.label("tag", tag, 0.6, 3.0, 1.0, "end");
        }
        _ => {
            d.size(6.0, 6.0);
            d.circle(3.0, 4.0, 1.8);
            let dc = p.text("current") == "dc";
            let three = !dc && p.int("phases") >= 3;
            if three {
                d.line(&[(1.0, 0.0), (1.0, 1.2), (1.9, 2.6)]);
                d.line(&[(3.0, 0.0), (3.0, 2.2)]);
                d.line(&[(5.0, 0.0), (5.0, 1.2), (4.1, 2.6)]);
                d.port("U1", 1.0, 0.0, "up");
                d.port("V1", 3.0, 0.0, "up");
                d.port("W1", 5.0, 0.0, "up");
            } else {
                d.line(&[(2.0, 0.0), (2.0, 1.2), (2.4, 2.3)]);
                d.line(&[(4.0, 0.0), (4.0, 1.2), (3.6, 2.3)]);
                d.port("1", 2.0, 0.0, "up");
                d.port("2", 4.0, 0.0, "up");
            }
            let letter = if role == "generator" { "G" } else { "M" };
            d.label("letter", letter, 3.0, 3.6, 1.4, "middle");
            let sign = match (dc, three) {
                (true, _) => "=",
                (false, true) => "3~",
                (false, false) => "~",
            };
            d.label("current", sign, 3.0, 4.9, 0.9, "middle");
            d.label("tag", tag, 5.2, 4.0, 1.0, "start");
        }
    }
}
