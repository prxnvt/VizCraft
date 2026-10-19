//! Sequence > Messages: the Connector tool armed with one of the message sorts (a drag from stem
//! to stem then draws that sort at the row under the pointer), a reply to the selected call
//! (`sequence.reply`), and a self call on the selected lifeline. Lost and found are set from the
//! Inspector's Sort field.

use serde_json::json;

use super::{selected_lifeline, selected_message};
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};
use crate::{CraftApp, Tool};

/// Whether the Connector tool is armed with connector type `kind`.
fn armed(app: &CraftApp, kind: &str) -> bool {
    app.tool == Tool::Connector && app.connector_kind == kind
}

/// The Sequence tab's Messages group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Messages",
        vec![
            Stack(vec![
                Button {
                    id: "ui.tool",
                    icon: "msgSync",
                    label: "Sync",
                    size: Size::Small,
                    params: |_| json!({"tool": "connector", "kind": "uml.syncMessage"}),
                    checked: Some(|a| armed(a, "uml.syncMessage")),
                },
                Button {
                    id: "ui.tool",
                    icon: "msgAsync",
                    label: "Async",
                    size: Size::Small,
                    params: |_| json!({"tool": "connector", "kind": "uml.asyncMessage"}),
                    checked: Some(|a| armed(a, "uml.asyncMessage")),
                },
                Button {
                    id: "ui.tool",
                    icon: "msgReply",
                    label: "Reply",
                    size: Size::Small,
                    params: |_| json!({"tool": "connector", "kind": "uml.replyMessage"}),
                    checked: Some(|a| armed(a, "uml.replyMessage")),
                },
            ]),
            Stack(vec![
                Button {
                    id: "ui.tool",
                    icon: "msgCreate",
                    label: "Create",
                    size: Size::Small,
                    params: |_| json!({"tool": "connector", "kind": "uml.createMessage"}),
                    checked: Some(|a| armed(a, "uml.createMessage")),
                },
                Button {
                    id: "ui.tool",
                    icon: "msgDestroy",
                    label: "Destroy",
                    size: Size::Small,
                    params: |_| json!({"tool": "connector", "kind": "uml.destroyMessage"}),
                    checked: Some(|a| armed(a, "uml.destroyMessage")),
                },
            ]),
            Stack(vec![
                Button {
                    id: "sequence.reply",
                    icon: "msgReply",
                    label: "Reply to Call",
                    size: Size::Small,
                    params: |a| json!({"call": selected_message(a)}),
                    checked: None,
                },
                Button {
                    id: "sequence.addMessage",
                    icon: "msgSync",
                    label: "Self Call",
                    size: Size::Small,
                    params: |a| {
                        let l = selected_lifeline(a);
                        json!({"from": l, "to": l})
                    },
                    checked: None,
                },
            ]),
        ],
    )
}
