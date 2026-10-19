//! Notation > Steps: number connectors in the order of a request. The Steps tool numbers by
//! click, Renumber closes gaps and Clear Step removes the selected connector's number.

use serde_json::{Value, json};

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};
use crate::{CraftApp, Tool};

/// The Steps tool's toggle: arms the tool, or returns to the pointer while it is armed.
fn tool_params(app: &CraftApp) -> Value {
    json!({"tool": if app.tool == Tool::Steps { "pointer" } else { "steps" }})
}

fn tool_armed(app: &CraftApp) -> bool {
    app.tool == Tool::Steps
}

/// `connector.setStep` clearing the selected connector's number.
fn clear_params(app: &CraftApp) -> Value {
    let page = app.session.doc.page();
    let id = app.session.sel.items.iter().find(|i| page.is_some_and(|p| p.connector(i).is_some())).cloned();
    json!({"id": id, "step": null})
}

/// The notation tab's Steps group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Steps",
        vec![
            Button { id: "ui.tool", icon: "steps", label: "Steps", size: Size::Big, params: tool_params, checked: Some(tool_armed) },
            Stack(vec![
                Small { id: "connector.renumberSteps", icon: "numberShapes" },
                Button { id: "connector.setStep", icon: "close", label: "Clear Step", size: Size::Small, params: clear_params, checked: None },
            ]),
        ],
    )
}
