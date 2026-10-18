//! Design > Layout: re-layout the page and route connectors around shapes.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// The Re-Layout menu: each algorithm in each direction it honours (radial and force have none).
/// No entry declares a check: `layout.full` is a one-shot arrangement that records nothing on the
/// page (`Page.view.layout` is written by no command), so there is no current choice to show.
fn relayout_menu() -> Vec<MenuEntry> {
    let mut out: Vec<MenuEntry> = Vec::new();
    for (algorithm, names) in [
        ("layered", ["Layered, Top to Bottom", "Layered, Left to Right", "Layered, Bottom to Top", "Layered, Right to Left"]),
        ("tree", ["Tree, Top to Bottom", "Tree, Left to Right", "Tree, Bottom to Top", "Tree, Right to Left"]),
    ] {
        for (name, direction) in names.into_iter().zip(["TB", "LR", "BT", "RL"]) {
            out.push(MenuEntry::new(name, "layout.full", json!({"algorithm": algorithm, "direction": direction})));
        }
    }
    out.push(MenuEntry::new("Radial", "layout.full", json!({"algorithm": "radial"})));
    out.push(MenuEntry::new("Force-Directed", "layout.full", json!({"algorithm": "force"})));
    out
}

/// The Design tab's Layout group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Layout",
        vec![Menu { label: "Re-Layout", icon: "layout", big: true, menu: relayout_menu() }, Big { id: "layout.route", icon: "routeAround" }],
    )
}
