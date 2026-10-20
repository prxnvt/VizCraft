//! Circuit > Analyze: trace the pressure from the selected component, in any valve position or
//! with every valve at rest, into the Analysis dock and the canvas halos; Clear drops the result.

use serde_json::{Value, json};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

fn any(_app: &CraftApp) -> Value {
    json!({"positions": "any"})
}

fn rest(_app: &CraftApp) -> Value {
    json!({"positions": "rest"})
}

/// The Analyze group.
pub(super) fn group() -> Group {
    Group::new(
        "Analyze",
        vec![Stack(vec![
            Button { id: "fluid.trace", icon: "tracePressure", label: "Trace Pressure", size: Size::Small, params: any, checked: None },
            Button { id: "fluid.trace", icon: "traceRest", label: "Trace at Rest", size: Size::Small, params: rest, checked: None },
            Button { id: "analyze.clear", icon: "eraser", label: "Clear Trace", size: Size::Small, params: |_| json!({}), checked: None },
        ])],
    )
}
