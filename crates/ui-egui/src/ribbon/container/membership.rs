//! Container > Membership: lock, select contents and disband.

use serde_json::json;

use super::selected_container;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Container tab's Membership group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Membership",
        vec![
            Button {
                id: "container.set",
                icon: "lock",
                label: "Lock",
                size: Size::Big,
                params: |a| json!({"locked": !selected_container(a).is_some_and(|c| c.locked)}),
                checked: None,
            },
            Stack(vec![Small { id: "container.selectContents", icon: "selectAll" }, Small { id: "container.disband", icon: "disband" }]),
        ],
    )
}
