//! View > Appearance: dark mode, the workspace preset (Engineering or Office) and on Apple macOS
//! the in-window menus.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// The View tab's Appearance group.
pub(in crate::ribbon) fn group() -> Group {
    let mut appearance = vec![Toggle { id: "view.dark", icon: "dark", checked: |a| a.session.view.dark_mode }];
    if cfg!(target_os = "macos") {
        appearance.push(Toggle { id: "ui.inWindowMenus", icon: "menuBar", checked: |a| a.ui.in_window_menus });
    }
    appearance.push(Menu {
        label: "Workspace",
        icon: "taskPanes",
        big: false,
        menu: vec![
            MenuEntry::new("Engineering", "ui.preset", json!({"name": "engineering"})).checked(|a| a.ui.preset == "engineering"),
            // No preset chosen yet draws the office look, so Office reads as current then too.
            MenuEntry::new("Office", "ui.preset", json!({"name": "office"})).checked(|a| a.ui.preset != "engineering"),
        ],
    });
    Group::new("Appearance", vec![Stack(appearance)])
}
