//! View > Appearance: dark mode, and on macOS the in-window menus.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The View tab's Appearance group.
pub(in crate::ribbon) fn group() -> Group {
    let mut appearance = vec![Toggle { id: "view.dark", icon: "dark", checked: |a| a.session.view.dark_mode }];
    if cfg!(target_os = "macos") {
        appearance.push(Toggle { id: "ui.inWindowMenus", icon: "menuBar", checked: |a| a.ui.in_window_menus });
    }
    Group::new("Appearance", vec![Stack(appearance)])
}
