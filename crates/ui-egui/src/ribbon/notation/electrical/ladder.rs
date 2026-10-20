//! Electrical > Ladder: the Ladder tool, a drag that draws a ladder frame with numbered rungs and
//! its rails L1 and L2 (Escape returns to the pointer).

use serde_json::{Value, json};

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};
use crate::{CraftApp, Tool};

fn ladder_params(app: &CraftApp) -> Value {
    json!({"tool": if app.tool == Tool::Ladder { "pointer" } else { "ladder" }})
}

fn ladder_armed(app: &CraftApp) -> bool {
    app.tool == Tool::Ladder
}

/// The Ladder group.
pub(super) fn group() -> Group {
    Group::new(
        "Ladder",
        vec![Button { id: "ui.tool", icon: "ladderTool", label: "Ladder Tool", size: Size::Big, params: ladder_params, checked: Some(ladder_armed) }],
    )
}
