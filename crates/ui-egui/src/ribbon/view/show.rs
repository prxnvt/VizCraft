//! View > Show: the side panes and the ribbon.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The View tab's Show group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Show",
        vec![Stack(vec![
            Toggle { id: "view.navigationPane", icon: "panelLeft", checked: |a| a.session.view.nav_pane },
            Toggle { id: "view.propertiesPane", icon: "panelRight", checked: |a| a.session.view.props_pane },
            Toggle { id: "ui.collapseRibbon", icon: "ribbon", checked: |a| a.ui.ribbon_collapsed },
        ])],
    )
}
