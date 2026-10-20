//! Circuit > Lines: each button arms the Pipe tool drawing one line type (`ui.tool {tool:
//! "pipe", line}`), checked while armed; a second click returns to the pointer.

use serde_json::{Value, json};

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};
use crate::{CraftApp, Tool};

/// Whether the Pipe or Wire tool is armed with `line`.
fn armed(app: &CraftApp, line: &str) -> bool {
    matches!(app.tool, Tool::Wire { .. }) && app.line_type.as_deref() == Some(line)
}

/// `ui.tool` arming `line`, or the pointer while it is armed.
fn toggle(app: &CraftApp, line: &str) -> Value {
    if armed(app, line) { json!({"tool": "pointer"}) } else { json!({"tool": "pipe", "line": line}) }
}

fn working(app: &CraftApp) -> Value {
    toggle(app, "fluidpower.working")
}

fn working_on(app: &CraftApp) -> bool {
    armed(app, "fluidpower.working")
}

fn pilot(app: &CraftApp) -> Value {
    toggle(app, "fluidpower.pilot")
}

fn pilot_on(app: &CraftApp) -> bool {
    armed(app, "fluidpower.pilot")
}

fn drain(app: &CraftApp) -> Value {
    toggle(app, "fluidpower.drain")
}

fn drain_on(app: &CraftApp) -> bool {
    armed(app, "fluidpower.drain")
}

fn mechanical(app: &CraftApp) -> Value {
    toggle(app, "fluidpower.mechanical")
}

fn mechanical_on(app: &CraftApp) -> bool {
    armed(app, "fluidpower.mechanical")
}

fn signal(app: &CraftApp) -> Value {
    toggle(app, "fluidpower.signal")
}

fn signal_on(app: &CraftApp) -> bool {
    armed(app, "fluidpower.signal")
}

/// The Lines group.
pub(super) fn group() -> Group {
    Group::new(
        "Lines",
        vec![
            Button { id: "ui.tool", icon: "workingLine", label: "Working", size: Size::Big, params: working, checked: Some(working_on) },
            Stack(vec![
                Button { id: "ui.tool", icon: "pilotLine", label: "Pilot", size: Size::Small, params: pilot, checked: Some(pilot_on) },
                Button { id: "ui.tool", icon: "drainLine", label: "Drain", size: Size::Small, params: drain, checked: Some(drain_on) },
            ]),
            Stack(vec![
                Button {
                    id: "ui.tool",
                    icon: "mechanicalLine",
                    label: "Mechanical",
                    size: Size::Small,
                    params: mechanical,
                    checked: Some(mechanical_on),
                },
                Button { id: "ui.tool", icon: "signalLine", label: "Signal", size: Size::Small, params: signal, checked: Some(signal_on) },
            ]),
        ],
    )
}
