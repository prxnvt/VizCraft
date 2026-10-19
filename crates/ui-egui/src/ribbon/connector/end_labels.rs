//! Connector > End Labels: the text beside each end of the selected connector (a cardinality
//! such as `1` or `N`, a multiplicity such as `0..*`). Each field runs `connector.setEndLabels`;
//! typing any text works, the list offers the common ones.

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, selected_connector};

/// The labels the lists offer: none, then Chen cardinalities, then UML multiplicities.
const COMMON: [&str; 8] = ["", "1", "N", "M", "0..1", "1..*", "0..*", "*"];

fn items(_: &CraftApp) -> Vec<String> {
    COMMON.iter().map(|s| s.to_string()).collect()
}

/// The Connector tab's End Labels group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "End Labels",
        vec![Stack(vec![
            Row(vec![
                Label { icon: "endLabels", text: "Start" },
                Combo {
                    id: "connector.setEndLabels",
                    param: "start",
                    name: "Start Label",
                    width: 66.0,
                    items,
                    current: |a| selected_connector(a).map(|c| c.start_label.clone()).unwrap_or_default(),
                },
            ]),
            Row(vec![
                Label { icon: "endLabels", text: "End" },
                Combo {
                    id: "connector.setEndLabels",
                    param: "end",
                    name: "End Label",
                    width: 66.0,
                    items,
                    current: |a| selected_connector(a).map(|c| c.end_label.clone()).unwrap_or_default(),
                },
            ]),
        ])],
    )
}
