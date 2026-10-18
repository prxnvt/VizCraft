//! Callout > Leader: the leader line on or off and the side of the target it meets.

use serde_json::json;

use super::selected_callout;
use crate::ribbon::Control::*;
use vizcraft_diagram::container::Side;

use crate::CraftApp;
use crate::ribbon::{Group, MenuEntry, Size};

fn side_is(app: &CraftApp, side: Side) -> bool {
    selected_callout(app).is_some_and(|c| c.side == side)
}

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
                    MenuEntry::new("Facing the Callout", "callout.set", json!({"side": "auto"})).checked(|a| side_is(a, Side::Auto)),
                    MenuEntry::new("Top", "callout.set", json!({"side": "top"})).checked(|a| side_is(a, Side::Top)),
                    MenuEntry::new("Bottom", "callout.set", json!({"side": "bottom"})).checked(|a| side_is(a, Side::Bottom)),
                    MenuEntry::new("Left", "callout.set", json!({"side": "left"})).checked(|a| side_is(a, Side::Left)),
                    MenuEntry::new("Right", "callout.set", json!({"side": "right"})).checked(|a| side_is(a, Side::Right)),
                ],
            },
        ],
    )
}
