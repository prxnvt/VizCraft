//! Container > Size: margins, fit to contents and automatic fitting.

use serde_json::json;

use super::selected_container;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size, short_num};

/// Margins offered by the margins field, in world units.
const MARGINS: &[u32] = &[0, 5, 10, 15, 20, 30, 40, 60];

/// The Container tab's Size group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Size",
        vec![
            Big { id: "container.fit", icon: "fitContents" },
            Stack(vec![
                Row(vec![
                    Label { icon: "margins", text: "Margins" },
                    Combo {
                        id: "container.set",
                        param: "margins",
                        name: "Margins",
                        width: 66.0,
                        items: |_| MARGINS.iter().map(|m| format!("{m} px")).collect(),
                        current: |a| selected_container(a).map(|c| format!("{} px", short_num(c.margin))).unwrap_or_default(),
                    },
                ]),
                Button {
                    id: "container.set",
                    icon: "autoFit",
                    label: "Auto Fit",
                    size: Size::Small,
                    params: |a| json!({"autoFit": !selected_container(a).is_some_and(|c| c.auto_fit)}),
                    checked: Some(|a| selected_container(a).is_some_and(|c| c.auto_fit)),
                },
            ]),
        ],
    )
}
