//! Home > Insert (engineering workspace): the next step after the selection, connecting the
//! selected shapes, the type-to-insert picker and the Patterns pane.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The engineering Home tab's Insert group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Insert",
        vec![
            Big { id: "insert.next", icon: "plus" },
            Stack(vec![
                Small { id: "insert.connect", icon: "connector" },
                Small { id: "ui.insertPicker", icon: "commands" },
                Button {
                    id: "ui.pane.patterns",
                    icon: "pattern",
                    label: "Patterns",
                    size: Size::Small,
                    params: |_| json!({"value": true}),
                    checked: None,
                },
            ]),
        ],
    )
}
