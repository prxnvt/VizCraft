//! Connector > Arrows: the start and end arrowheads.

use crate::ribbon::Control::*;
use crate::ribbon::{ARROWS, Group, selected_connector};

/// The Connector tab's Arrows group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Arrows",
        vec![Stack(vec![
            Row(vec![
                Label { icon: "arrowStart", text: "Start" },
                Combo {
                    id: "connector.setArrows",
                    param: "start",
                    name: "Start Arrow",
                    width: 92.0,
                    items: |_| ARROWS.iter().map(|s| s.to_string()).collect(),
                    current: |a| selected_connector(a).map(|c| c.start_arrow.name().to_string()).unwrap_or_default(),
                },
            ]),
            Row(vec![
                Label { icon: "arrowEnd", text: "End" },
                Combo {
                    id: "connector.setArrows",
                    param: "end",
                    name: "End Arrow",
                    width: 92.0,
                    items: |_| ARROWS.iter().map(|s| s.to_string()).collect(),
                    current: |a| selected_connector(a).map(|c| c.end_arrow.name().to_string()).unwrap_or_default(),
                },
            ]),
        ])],
    )
}
