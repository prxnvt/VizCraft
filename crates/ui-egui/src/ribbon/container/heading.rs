//! Container > Heading: where the heading runs and how it is drawn.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Container tab's Heading group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Heading",
        vec![Stack(vec![
            Menu {
                label: "Position",
                icon: "headingPosition",
                big: false,
                menu: vec![
                    ("Top", "container.set", json!({"headingPosition": "top"})),
                    ("Bottom", "container.set", json!({"headingPosition": "bottom"})),
                    ("Left", "container.set", json!({"headingPosition": "left"})),
                    ("Right", "container.set", json!({"headingPosition": "right"})),
                ],
            },
            Menu {
                label: "Heading Style",
                icon: "headingStyle",
                big: false,
                menu: vec![
                    ("Band", "container.set", json!({"headingStyle": "band"})),
                    ("Rule", "container.set", json!({"headingStyle": "line"})),
                    ("Text Only", "container.set", json!({"headingStyle": "plain"})),
                ],
            },
        ])],
    )
}
