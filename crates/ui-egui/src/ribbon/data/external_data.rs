//! Data > External Data: link a CSV or XLSX table to shape data, refresh every link, and the
//! External Data pane.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Data tab's External Data group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "External Data",
        vec![
            Big { id: "ui.linkData", icon: "linkData" },
            Stack(vec![
                Small { id: "datalink.refresh", icon: "refresh" },
                Button {
                    id: "ui.pane.externalData",
                    icon: "externalData",
                    label: "External Data",
                    size: Size::Small,
                    params: |_| json!({}),
                    checked: Some(|a| a.ui.panes.is_open("externalData")),
                },
            ]),
        ],
    )
}
