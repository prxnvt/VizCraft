//! Process > Diagram Validation: check the whole document (Check also opens the dock), choose the rules it is held to, switch
//! live checking, step through the issues and open the Problems tab of the dock. Every control
//! runs a `validate.*` or `ui.dock` command.

use serde_json::json;

use crate::CraftApp;
use crate::dock::DockTab;
use crate::dock::problems::{live_on, rules_menu};
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// Whether the dock shows the Problems tab.
fn problems_open(app: &CraftApp) -> bool {
    app.ui.dock.open && app.ui.dock.tab == DockTab::Problems
}

/// The validation controls: Check, the rules menu, Live Checking, Problems, Next and Previous.
/// The Process tab's Diagram Validation group and a notation tab's Check group both show them.
pub(in crate::ribbon) fn controls() -> Vec<crate::ribbon::Control> {
    vec![
        Big { id: "validate.run", icon: "check" },
        Stack(vec![
            Custom(rules_menu),
            Button {
                id: "validate.setProfile",
                icon: "liveChecking",
                label: "Live Checking",
                size: Size::Small,
                params: |a| json!({"enabled": !live_on(a)}),
                checked: Some(live_on),
            },
            Button {
                id: "ui.dock",
                icon: "problems",
                label: "Problems",
                size: Size::Small,
                params: |a| json!({"tab": "problems", "open": !problems_open(a)}),
                checked: Some(problems_open),
            },
        ]),
        Stack(vec![Small { id: "validate.next", icon: "nextIssue" }, Small { id: "validate.previous", icon: "prevIssue" }]),
    ]
}

/// The Process tab's Diagram Validation group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Diagram Validation", controls())
}
