//! Connector > Style: line colour and weight of the selected connector.

use crate::ribbon::Control::*;
use crate::ribbon::{Group, LINE_WEIGHTS, color32, selected_connector, short_num};

/// The Connector tab's Style group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Style",
        vec![Stack(vec![
            Colors {
                id: "shape.setStyle",
                param: "line",
                icon: "lineColor",
                name: "Line",
                show_label: true,
                none: None,
                current: |a| selected_connector(a).map(|c| color32(c.style.line)),
            },
            Row(vec![
                Label { icon: "lineWeight", text: "" },
                Combo {
                    id: "shape.setStyle",
                    param: "lineWidth",
                    name: "Line Weight",
                    width: 66.0,
                    items: |_| LINE_WEIGHTS.iter().map(|w| format!("{} px", short_num(f64::from(*w)))).collect(),
                    current: |a| selected_connector(a).map(|c| format!("{} px", short_num(c.style.line_width))).unwrap_or_default(),
                },
            ]),
        ])],
    )
}
