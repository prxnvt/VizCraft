//! Container > Heading: where the heading runs and how it is drawn.

use serde_json::json;

use crate::ribbon::Control::*;
use vizcraft_diagram::container::{HeadingPosition, HeadingStyle};

use super::selected_container;
use crate::ribbon::{Group, MenuEntry};

fn position_is(app: &crate::CraftApp, p: HeadingPosition) -> bool {
    selected_container(app).is_some_and(|c| c.heading_position == p)
}

fn style_is(app: &crate::CraftApp, s: HeadingStyle) -> bool {
    selected_container(app).is_some_and(|c| c.heading_style == s)
}

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
                    MenuEntry::new("Top", "container.set", json!({"headingPosition": "top"})).checked(|a| position_is(a, HeadingPosition::Top)),
                    MenuEntry::new("Bottom", "container.set", json!({"headingPosition": "bottom"})).checked(|a| position_is(a, HeadingPosition::Bottom)),
                    MenuEntry::new("Left", "container.set", json!({"headingPosition": "left"})).checked(|a| position_is(a, HeadingPosition::Left)),
                    MenuEntry::new("Right", "container.set", json!({"headingPosition": "right"})).checked(|a| position_is(a, HeadingPosition::Right)),
                ],
            },
            Menu {
                label: "Heading Style",
                icon: "headingStyle",
                big: false,
                menu: vec![
                    MenuEntry::new("Band", "container.set", json!({"headingStyle": "band"})).checked(|a| style_is(a, HeadingStyle::Band)),
                    MenuEntry::new("Rule", "container.set", json!({"headingStyle": "line"})).checked(|a| style_is(a, HeadingStyle::Line)),
                    MenuEntry::new("Text Only", "container.set", json!({"headingStyle": "plain"})).checked(|a| style_is(a, HeadingStyle::Plain)),
                ],
            },
        ])],
    )
}
