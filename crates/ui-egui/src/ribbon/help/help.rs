//! Help > Help: the command list, the examples and About.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Help tab's Help group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Help",
        vec![
            Big { id: "ui.commands", icon: "commands" },
            Button { id: "ui.gallery", icon: "examples", label: "Examples", size: Size::Big, params: |_| json!({"mode": "examples"}), checked: None },
            Big { id: "help.about", icon: "info" },
        ],
    )
}
