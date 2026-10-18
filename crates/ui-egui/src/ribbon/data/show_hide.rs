//! Data > Show/Hide: the Shape Data window and the Define Shape Data dialog.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Data tab's Show/Hide group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Show/Hide",
        vec![Stack(vec![
            Toggle { id: "view.shapeDataPane", icon: "panelRight", checked: |a| a.ui.panes.is_open("shapeData") },
            Small { id: "data.defineDialog", icon: "plus" },
        ])],
    )
}
