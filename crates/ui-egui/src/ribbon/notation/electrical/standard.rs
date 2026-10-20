//! Electrical > Standard: the project's symbol set (IEC 60617, ANSI IEEE 315 or JIC) and
//! designation format (IEC 81346, JIC or ASME Y14.44), each a small menu with the current choice
//! checked (a choice runs `electrical.setStandards`, which redraws in place in one undo step),
//! and the Lines menu: the line the Wire tool draws (`ui.tool {tool: "wire", line}`), checked.

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

fn symbols(app: &CraftApp) -> String {
    let s = app.session.doc.standards.symbols.clone();
    if s.is_empty() { "iec".into() } else { s }
}

fn tags(app: &CraftApp) -> &'static str {
    vizcraft_notation::electrical_pass::tag_format(&app.session.doc)
}

fn entry(label: &'static str, params: serde_json::Value, checked: fn(&CraftApp) -> bool) -> MenuEntry {
    MenuEntry { label, id: "electrical.setStandards", params, checked: Some(checked) }
}

fn line(label: &'static str, params: serde_json::Value, checked: fn(&CraftApp) -> bool) -> MenuEntry {
    MenuEntry { label, id: "ui.tool", params, checked: Some(checked) }
}

/// Whether the Wire tool is armed drawing `line` (the default line counts as the plain wire).
fn drawing(app: &CraftApp, line: &str) -> bool {
    matches!(app.tool, crate::Tool::Wire { pipe: false }) && crate::canvas::interact::wire::line_type(app, false).as_deref() == Some(line)
}

/// The Standard group.
pub(super) fn group() -> Group {
    Group::new(
        "Standard",
        vec![Stack(vec![
            Menu {
                label: "Symbol Set",
                icon: "symbolStandard",
                big: false,
                menu: vec![
                    entry("IEC 60617", json!({"symbols": "iec"}), |a| symbols(a) == "iec"),
                    entry("ANSI (IEEE 315)", json!({"symbols": "ansi"}), |a| symbols(a) == "ansi"),
                    entry("JIC", json!({"symbols": "jic"}), |a| symbols(a) == "jic"),
                ],
            },
            Menu {
                label: "Tag Format",
                icon: "tagFormat",
                big: false,
                menu: vec![
                    entry("IEC 81346 (=F1+A2-K1)", json!({"tags": "iec81346"}), |a| tags(a) == "iec81346"),
                    entry("JIC (CR1)", json!({"tags": "jic"}), |a| tags(a) == "jic"),
                    entry("ASME Y14.44 (1A2K1)", json!({"tags": "y1444"}), |a| tags(a) == "y1444"),
                ],
            },
            Menu {
                label: "Lines",
                icon: "lineWire",
                big: false,
                menu: vec![
                    line("Wire Line", json!({"tool": "wire", "line": "electrical.wire"}), |a| drawing(a, "electrical.wire")),
                    line("Bus Line", json!({"tool": "wire", "line": "electrical.bus"}), |a| drawing(a, "electrical.bus")),
                    line("Signal Line", json!({"tool": "wire", "line": "electrical.signal"}), |a| drawing(a, "electrical.signal")),
                ],
            },
        ])],
    )
}
