//! Home > Shape Styles: fill, line colour and line weight.

use crate::ribbon::Control::*;
use crate::ribbon::{Group, LINE_WEIGHTS, color32, selected_shape, selected_style, short_num};

/// The Home tab's Shape Styles group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Shape Styles",
        vec![Stack(vec![
            Colors {
                id: "shape.setStyle",
                param: "fill",
                icon: "fill",
                name: "Fill",
                show_label: true,
                none: Some("No Fill"),
                current: |a| selected_shape(a).map(|s| color32(s.style.fill)),
            },
            Colors {
                id: "shape.setStyle",
                param: "line",
                icon: "lineColor",
                name: "Line",
                show_label: true,
                none: None,
                current: |a| selected_style(a).map(|s| color32(s.line)),
            },
            Row(vec![
                Label { icon: "lineWeight", text: "" },
                Combo {
                    id: "shape.setStyle",
                    param: "lineWidth",
                    name: "Line Weight",
                    width: 66.0,
                    items: |_| LINE_WEIGHTS.iter().map(|w| format!("{} px", short_num(f64::from(*w)))).collect(),
                    current: |a| selected_style(a).map(|s| format!("{} px", short_num(s.line_width))).unwrap_or_default(),
                },
            ]),
        ])],
    )
}
