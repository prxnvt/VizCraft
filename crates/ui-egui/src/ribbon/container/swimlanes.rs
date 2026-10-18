//! Container > Swimlanes: lanes in and out, phases and orientation, for a selected lane set or
//! lane.

use serde_json::json;
use vizcraft_diagram::container::{self as ct, Role};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size, selected_shape};

/// Where a new phase goes: the middle of the selected set's lanes.
fn phase_at(app: &CraftApp) -> serde_json::Value {
    let Some(page) = app.session.doc.page() else { return json!({}) };
    let set = selected_shape(app).and_then(|s| match ct::role(s) {
        Some(Role::LaneSet) => Some(s),
        Some(Role::Lane) => ct::set_of(page, &s.id).and_then(|id| page.shape(&id)),
        _ => None,
    });
    match set {
        Some(s) => {
            let vertical = s.container.as_ref().is_some_and(|c| c.vertical);
            json!({"at": ((if vertical { s.h } else { s.w }) / 2.0).round()})
        }
        None => json!({}),
    }
}

/// The Container tab's Swimlanes group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Swimlanes",
        vec![
            Big { id: "lane.add", icon: "laneAdd" },
            Stack(vec![
                Small { id: "lane.remove", icon: "delete" },
                Button { id: "lane.addPhase", icon: "phase", label: "Add Phase", size: Size::Small, params: phase_at, checked: None },
                Small { id: "lane.flip", icon: "flipLanes" },
            ]),
        ],
    )
}
