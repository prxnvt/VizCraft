//! Ink > Convert and Close: turn strokes into shapes, and leave the ink tools.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Ink tab's Convert group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Convert",
        vec![
            Big { id: "ink.toShapes", icon: "inkToShapes" },
            Button { id: "ui.tool", icon: "close", label: "Close Ink Tools", size: Size::Big, params: |_| json!({"tool": "pointer"}), checked: None },
        ],
    )
}
