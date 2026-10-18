//! View > Show: rulers, page breaks, grid, guides, connection points, the side panes and the
//! ribbon.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The View tab's Show group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Show",
        vec![
            Stack(vec![
                Toggle { id: "view.rulers", icon: "ruler", checked: |a| a.session.view.aids.rulers },
                Toggle { id: "view.pageBreaks", icon: "pageBreaks", checked: |a| a.session.view.aids.page_breaks },
                Toggle { id: "view.connectionPoints", icon: "connectionPoints", checked: |a| a.session.view.aids.connection_points },
            ]),
            Stack(vec![
                Toggle { id: "view.grid", icon: "grid", checked: |a| a.session.view.aids.grid },
                Toggle { id: "view.guides", icon: "guides", checked: |a| a.session.view.aids.guides },
            ]),
            Stack(vec![
                Toggle { id: "view.navigationPane", icon: "panelLeft", checked: |a| a.session.view.nav_pane },
                Toggle { id: "view.propertiesPane", icon: "panelRight", checked: |a| a.session.view.props_pane },
                Toggle { id: "ui.collapseRibbon", icon: "ribbon", checked: |a| a.ui.ribbon_collapsed },
            ]),
        ],
    )
}
