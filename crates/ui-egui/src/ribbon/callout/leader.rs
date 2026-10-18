//! Callout > Leader: the leader line on or off and the side of the target it meets.

use serde_json::json;

use super::selected_callout;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Callout tab's Leader group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Leader",
        vec![
            Button {
                id: "callout.set",
                icon: "leader",
                label: "Leader",
                size: Size::Big,
                params: |a| json!({"leader": !selected_callout(a).is_some_and(|c| c.leader)}),
                checked: None,
            },
            Menu {
                label: "Target Side",
                icon: "position",
                big: false,
                menu: vec![
                    ("Facing the Callout", "callout.set", json!({"side": "auto"})),
                    ("Top", "callout.set", json!({"side": "top"})),
                    ("Bottom", "callout.set", json!({"side": "bottom"})),
                    ("Left", "callout.set", json!({"side": "left"})),
                    ("Right", "callout.set", json!({"side": "right"})),
                ],
            },
        ],
    )
}
