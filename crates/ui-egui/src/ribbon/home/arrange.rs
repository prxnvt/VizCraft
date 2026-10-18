//! Home > Arrange: align, position (distribute and auto space), z order, group, rotate and flip,
//! and delete. Every entry runs an `arrange.*` command on the selection.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// The Align menu: each edge or centre line, to the first selected shape, then to the page.
fn align_menu() -> Vec<MenuEntry> {
    let edges = [
        ("left", "Align Left", "Align Left to Page"),
        ("center", "Align Center", "Align Center to Page"),
        ("right", "Align Right", "Align Right to Page"),
        ("top", "Align Top", "Align Top to Page"),
        ("middle", "Align Middle", "Align Middle to Page"),
        ("bottom", "Align Bottom", "Align Bottom to Page"),
    ];
    let mut out: Vec<MenuEntry> = edges.iter().map(|(e, l, _)| MenuEntry::new(l, "arrange.align", json!({"edge": e}))).collect();
    out.extend(edges.iter().map(|(e, _, l)| MenuEntry::new(l, "arrange.align", json!({"edge": e, "to": "page"}))));
    out
}

/// The Position menu: distribute and auto space.
fn position_menu() -> Vec<MenuEntry> {
    vec![
        MenuEntry::new("Distribute Horizontally", "arrange.distribute", json!({"axis": "horizontal"})),
        MenuEntry::new("Distribute Vertically", "arrange.distribute", json!({"axis": "vertical"})),
        MenuEntry::new("Auto Space", "arrange.autoSpace", json!({})),
    ]
}

/// The Rotate menu: quarter turns and flips.
fn rotate_menu() -> Vec<MenuEntry> {
    vec![
        MenuEntry::new("Rotate Right 90", "arrange.rotate", json!({"deg": 90})),
        MenuEntry::new("Rotate Left 90", "arrange.rotate", json!({"deg": -90})),
        MenuEntry::new("Flip Horizontal", "arrange.flip", json!({"axis": "horizontal"})),
        MenuEntry::new("Flip Vertical", "arrange.flip", json!({"axis": "vertical"})),
    ]
}

/// The Home tab's Arrange group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Arrange",
        vec![
            Stack(vec![
                Menu { label: "Align", icon: "align", big: false, menu: align_menu() },
                Menu { label: "Position", icon: "position", big: false, menu: position_menu() },
                Menu { label: "Rotate", icon: "rotateRight", big: false, menu: rotate_menu() },
            ]),
            Stack(vec![
                Menu {
                    label: "Bring to Front",
                    icon: "bringFront",
                    big: false,
                    menu: vec![
                        MenuEntry::new("Bring to Front", "arrange.order", json!({"how": "front"})),
                        MenuEntry::new("Bring Forward", "arrange.order", json!({"how": "forward"})),
                    ],
                },
                Menu {
                    label: "Send to Back",
                    icon: "sendBack",
                    big: false,
                    menu: vec![
                        MenuEntry::new("Send to Back", "arrange.order", json!({"how": "back"})),
                        MenuEntry::new("Send Backward", "arrange.order", json!({"how": "backward"})),
                    ],
                },
                Menu {
                    label: "Group",
                    icon: "group",
                    big: false,
                    menu: vec![MenuEntry::new("Group", "arrange.group", json!({})), MenuEntry::new("Ungroup", "arrange.ungroup", json!({}))],
                },
            ]),
            Stack(vec![Small { id: "edit.delete", icon: "delete" }]),
        ],
    )
}
