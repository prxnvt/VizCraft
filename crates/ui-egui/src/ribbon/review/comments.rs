//! Review > Comments: New Comment opens the Comments pane on its new-comment box, Delete removes
//! the comments on the selected shape, Previous and Next walk the anchors, and Show Comments
//! toggles the pane (and with it the markers on the canvas).

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

fn pane_open(app: &CraftApp) -> bool {
    app.ui.panes.is_open("comments")
}

/// The Review tab's Comments group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Comments",
        vec![
            Button { id: "ui.pane.comments", icon: "commentNew", label: "New Comment", size: Size::Big, params: |_| json!({"value": true}), checked: None },
            Stack(vec![
                Toggle { id: "ui.pane.comments", icon: "comment", checked: pane_open },
                Small { id: "comment.delete", icon: "commentDelete" },
            ]),
            Stack(vec![Small { id: "comment.previous", icon: "commentPrev" }, Small { id: "comment.next", icon: "commentNext" }]),
        ],
    )
}
