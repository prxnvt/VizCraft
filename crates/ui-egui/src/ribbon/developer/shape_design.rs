//! Developer > Shape Design: the Operations menu (`shapeop.*`), Shape Name (`shape.rename`), and
//! the Behavior and Protection menus, whose rows toggle a flag of the selected shapes (or of the
//! document) and show a check mark while it is on for the first selected shape.

use serde_json::json;
use vizcraft_diagram::Shape;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, selected_shape};

/// Whether the first selected shape has `f` on.
fn on(app: &CraftApp, f: fn(&Shape) -> bool) -> bool {
    selected_shape(app).is_some_and(f)
}

/// The Developer tab's Shape Design group.
pub(in crate::ribbon) fn group() -> Group {
    let op = |label: &'static str, id: &'static str| MenuEntry::new(label, id, json!({}));
    let behavior = |label: &'static str, key: &'static str| MenuEntry::new(label, "shape.setBehavior", json!({ key: "toggle" }));
    let lock = |label: &'static str, key: &'static str| MenuEntry::new(label, "shape.setLocks", json!({ key: "toggle" }));
    Group::new(
        "Shape Design",
        vec![
            Menu {
                label: "Operations",
                icon: "shapeOps",
                big: true,
                menu: vec![
                    op("Union", "shapeop.union"),
                    op("Combine", "shapeop.combine"),
                    op("Fragment", "shapeop.fragment"),
                    op("Intersect", "shapeop.intersect"),
                    op("Subtract", "shapeop.subtract"),
                    op("Join", "shapeop.join"),
                    op("Trim", "shapeop.trim"),
                    MenuEntry::new("Offset Out", "shapeop.offset", json!({"distance": 10})),
                    MenuEntry::new("Offset In", "shapeop.offset", json!({"distance": -10})),
                ],
            },
            Big { id: "shape.rename", icon: "shapeName" },
            Stack(vec![
                Menu {
                    label: "Behavior",
                    icon: "behavior",
                    big: false,
                    menu: vec![
                        behavior("Container", "container").checked(|a| on(a, |s| s.behavior.container)),
                        behavior("Non-printing", "nonPrinting").checked(|a| on(a, |s| s.behavior.non_printing)),
                        behavior("Hidden", "hidden").checked(|a| on(a, |s| s.behavior.hidden)),
                        behavior("No Glue", "noGlue").checked(|a| on(a, |s| s.behavior.no_glue)),
                        behavior("No Snap", "noSnap").checked(|a| on(a, |s| s.behavior.no_snap)),
                    ],
                },
                Menu {
                    label: "Protection",
                    icon: "lock",
                    big: false,
                    menu: vec![
                        lock("Width", "width").checked(|a| on(a, |s| s.locks.width)),
                        lock("Height", "height").checked(|a| on(a, |s| s.locks.height)),
                        lock("Aspect Ratio", "aspect").checked(|a| on(a, |s| s.locks.aspect)),
                        lock("Position", "position").checked(|a| on(a, |s| s.locks.position)),
                        lock("Rotation", "rotation").checked(|a| on(a, |s| s.locks.rotation)),
                        lock("Deletion", "delete").checked(|a| on(a, |s| s.locks.delete)),
                        lock("Selection", "select").checked(|a| on(a, |s| s.locks.select)),
                        lock("Text", "text").checked(|a| on(a, |s| s.locks.text)),
                        MenuEntry::new("Protect Shapes", "doc.protect", json!({"shapes": "toggle"})).checked(|a| a.session.doc.protection.shapes),
                        MenuEntry::new("Protect Backgrounds", "doc.protect", json!({"backgrounds": "toggle"}))
                            .checked(|a| a.session.doc.protection.backgrounds),
                    ],
                },
            ]),
        ],
    )
}
