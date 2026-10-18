//! Insert > Diagram Parts: the connector and text tools as big buttons.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Insert tab's Diagram Parts group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Diagram Parts",
        vec![
            Button { id: "ui.tool", icon: "connector", label: "Connector", size: Size::Big, params: |_| json!({"tool": "connector"}), checked: None },
            Button { id: "ui.tool", icon: "text", label: "Text", size: Size::Big, params: |_| json!({"tool": "text"}), checked: None },
        ],
    )
}
