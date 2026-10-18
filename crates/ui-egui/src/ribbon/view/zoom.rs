//! View > Zoom: zoom buttons and the zoom level field.

use crate::ribbon::Control::*;
use crate::ribbon::{Group, ZOOM_STEPS};

/// The View tab's Zoom group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Zoom",
        vec![
            Big { id: "view.zoom", icon: "zoom" },
            Big { id: "view.zoomIn", icon: "zoomIn" },
            Big { id: "view.zoomOut", icon: "zoomOut" },
            Stack(vec![
                Combo {
                    id: "view.zoom",
                    param: "value",
                    name: "Zoom Level",
                    width: 76.0,
                    items: |_| ZOOM_STEPS.iter().map(|z| format!("{z}%")).collect(),
                    current: |a| format!("{}%", (a.session.view.zoom * 100.0).round()),
                },
                Small { id: "view.zoom100", icon: "zoomActual" },
                Small { id: "view.zoomFit", icon: "zoomFit" },
            ]),
        ],
    )
}
