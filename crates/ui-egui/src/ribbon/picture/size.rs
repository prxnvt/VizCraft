//! Picture > Size: the Crop tool.

use serde_json::json;

use crate::Tool;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Picture tab's Size group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Size",
        vec![Button {
            id: "ui.tool",
            icon: "crop",
            label: "Crop",
            size: Size::Small,
            params: |a| json!({"tool": if a.tool == Tool::Crop { "pointer" } else { "crop" }}),
            checked: Some(|a| a.tool == Tool::Crop),
        }],
    )
}
