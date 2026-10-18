//! Picture > Picture Styles: the border (the picture shape's outline) colour and weight, through
//! `shape.setStyle`.

use crate::ribbon::Control::*;
use crate::ribbon::{Group, LINE_WEIGHTS, color32, selected_style, short_num};

/// The Picture tab's Picture Styles group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Picture Styles",
        vec![Stack(vec![
            Colors {
                id: "shape.setStyle",
                param: "line",
                icon: "lineColor",
                name: "Border",
                show_label: true,
                none: Some("No Border"),
                current: |a| selected_style(a).map(|s| color32(s.line)),
            },
            Row(vec![
                Label { icon: "lineWeight", text: "" },
                Combo {
                    id: "shape.setStyle",
                    param: "lineWidth",
                    name: "Border Weight",
                    width: 66.0,
                    items: |_| LINE_WEIGHTS.iter().map(|w| format!("{} px", short_num(f64::from(*w)))).collect(),
                    current: |a| selected_style(a).map(|s| format!("{} px", short_num(s.line_width))).unwrap_or_default(),
                },
            ]),
        ])],
    )
}
