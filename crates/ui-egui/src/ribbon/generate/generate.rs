//! Generate > Generate: Preview, Generate and Check Output run the generate commands with the
//! Generate dock tab's fields (and open it); the toggle shows or hides that tab.

use serde_json::json;

use crate::CraftApp;
use crate::dock::DockTab;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// Whether the dock shows the Generate tab.
fn dock_on(app: &CraftApp) -> bool {
    app.ui.dock.open && app.ui.dock.tab == DockTab::Generate
}

/// The Generate tab's Generate group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Generate",
        vec![
            Big { id: "generate.preview", icon: "generatePreview" },
            Big { id: "generate.run", icon: "generate" },
            Big { id: "generate.check", icon: "generateCheck" },
            Button {
                id: "ui.dock",
                icon: "dock",
                label: "Generate Dock",
                size: Size::Small,
                params: |a| json!({"tab": "generate", "open": !dock_on(a)}),
                checked: Some(dock_on),
            },
        ],
    )
}
