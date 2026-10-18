//! Connector > Route: straight, orthogonal or curved.

use serde_json::json;
use vizcraft_diagram::Route;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size, selected_connector};

/// The Connector tab's Route group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Route",
        vec![Stack(vec![
            Button {
                id: "connector.setRoute",
                icon: "routeStraight",
                label: "Straight",
                size: Size::Small,
                params: |_| json!({"route": "straight"}),
                checked: Some(|a| selected_connector(a).is_some_and(|c| c.route == Route::Straight)),
            },
            Button {
                id: "connector.setRoute",
                icon: "routeOrthogonal",
                label: "Orthogonal",
                size: Size::Small,
                params: |_| json!({"route": "orthogonal"}),
                checked: Some(|a| selected_connector(a).is_some_and(|c| c.route == Route::Orthogonal)),
            },
            Button {
                id: "connector.setRoute",
                icon: "routeCurved",
                label: "Curved",
                size: Size::Small,
                params: |_| json!({"route": "curved"}),
                checked: Some(|a| selected_connector(a).is_some_and(|c| c.route == Route::Curved)),
            },
        ])],
    )
}
