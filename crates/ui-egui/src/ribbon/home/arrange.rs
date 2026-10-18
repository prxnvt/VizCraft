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
    let mut out: Vec<MenuEntry> = edges.iter().map(|(e, l, _)| (*l, "arrange.align", json!({"edge": e}))).collect();
    out.extend(edges.iter().map(|(e, _, l)| (*l, "arrange.align", json!({"edge": e, "to": "page"}))));
    out
}

/// The Position menu: distribute and auto space.
fn position_menu() -> Vec<MenuEntry> {
    vec![
        ("Distribute Horizontally", "arrange.distribute", json!({"axis": "horizontal"})),
        ("Distribute Vertically", "arrange.distribute", json!({"axis": "vertical"})),
        ("Auto Space", "arrange.autoSpace", json!({})),
    ]
}

/// The Rotate menu: quarter turns and flips.
fn rotate_menu() -> Vec<MenuEntry> {
    vec![
        ("Rotate Right 90", "arrange.rotate", json!({"deg": 90})),
        ("Rotate Left 90", "arrange.rotate", json!({"deg": -90})),
        ("Flip Horizontal", "arrange.flip", json!({"axis": "horizontal"})),
        ("Flip Vertical", "arrange.flip", json!({"axis": "vertical"})),
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
                        ("Bring to Front", "arrange.order", json!({"how": "front"})),
                        ("Bring Forward", "arrange.order", json!({"how": "forward"})),
                    ],
                },
                Menu {
                    label: "Send to Back",
                    icon: "sendBack",
                    big: false,
                    menu: vec![
                        ("Send to Back", "arrange.order", json!({"how": "back"})),
                        ("Send Backward", "arrange.order", json!({"how": "backward"})),
                    ],
                },
                Menu {
                    label: "Group",
                    icon: "group",
                    big: false,
                    menu: vec![("Group", "arrange.group", json!({})), ("Ungroup", "arrange.ungroup", json!({}))],
                },
            ]),
            Stack(vec![Small { id: "edit.delete", icon: "delete" }]),
        ],
    )
}
