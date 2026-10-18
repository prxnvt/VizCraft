//! Data > Show/Hide: the Inspector window and the Define Shape Data dialog.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Data tab's Show/Hide group. The window command keeps its id (`view.shapeDataPane`); the
/// button is labelled with the pane's title.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Show/Hide",
        vec![Stack(vec![
            Button {
                id: "view.shapeDataPane",
                icon: "panelRight",
                label: "Inspector",
                size: Size::Small,
                params: |_| json!({}),
                checked: Some(|a| a.ui.panes.is_open("shapeData")),
            },
            Small { id: "data.defineDialog", icon: "plus" },
        ])],
    )
}
