//! Circuit > Analyze: trace the pressure from the selected component (any valve position) into
//! the Analysis dock and the canvas halos, or with every valve at rest; Clear drops the result.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// The Analyze group.
pub(super) fn group() -> Group {
    Group::new(
        "Analyze",
        vec![
            Split {
                id: "fluid.trace",
                icon: "tracePressure",
                big: true,
                menu: vec![
                    MenuEntry::new("In Any Valve Position", "fluid.trace", json!({"positions": "any"})),
                    MenuEntry::new("With Valves at Rest", "fluid.trace", json!({"positions": "rest"})),
                ],
            },
            Small { id: "analyze.clear", icon: "close" },
        ],
    )
}
