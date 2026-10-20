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

/// The Lines group: one icon button per line type in two rows, the armed one checked; the
/// tooltips name them.
pub(super) fn group() -> Group {
    let b = |icon, label, params, checked| Button { id: "ui.tool", icon, label, size: Size::Icon, params, checked: Some(checked) };
    Group::new(
        "Lines",
        vec![Stack(vec![
            Row(vec![
                b("workingLine", "Working Line", working as fn(&CraftApp) -> Value, working_on as fn(&CraftApp) -> bool),
                b("pilotLine", "Pilot Line", pilot, pilot_on),
                b("drainLine", "Drain Line", drain, drain_on),
            ]),
            Row(vec![b("mechanicalLine", "Mechanical Link", mechanical, mechanical_on), b("signalLine", "Signal Line", signal, signal_on)]),
        ])],
    )
}
