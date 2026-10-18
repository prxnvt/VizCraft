//! Home > Tools: the canvas tool (pointer, connector, text).

use serde_json::json;

use crate::Tool;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Home tab's Tools group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Tools",
        vec![Stack(vec![
            Button {
                id: "ui.tool",
                icon: "pointer",
                label: "Pointer",
                size: Size::Small,
                params: |_| json!({"tool": "pointer"}),
                checked: Some(|a| a.tool == Tool::Pointer),
            },
            Button {
                id: "ui.tool",
                icon: "connector",
                label: "Connector",
                size: Size::Small,
                params: |_| json!({"tool": "connector"}),
                checked: Some(|a| a.tool == Tool::Connector),
            },
            Button {
                id: "ui.tool",
                icon: "text",
                label: "Text",
                size: Size::Small,
                params: |_| json!({"tool": "text"}),
                checked: Some(|a| a.tool == Tool::Text),
            },
        ])],
    )
}
