//! Sheet > Revisions and Parts: Revise (`sheet.revise`, which opens the Revise dialog), Balloon
//! (`sheet.balloon` on the selected part) and Renumber (`sheet.renumber`, which recomposes the
//! page and zone parts of tags).

use serde_json::{Value, json};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size, selected_shape};

/// The part to balloon: the selected shape's element (or the shape, which the command resolves).
fn balloon_params(app: &CraftApp) -> Value {
    match selected_shape(app) {
        Some(s) => json!({"part": s.element.clone().unwrap_or_else(|| s.id.clone())}),
        None => json!({}),
    }
}

/// The Sheet tab's Revisions and Parts group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Revisions and Parts",
        vec![
            Button { id: "sheet.revise", icon: "note", label: "Revise", size: Size::Big, params: |_| json!({}), checked: None },
            Button { id: "sheet.balloon", icon: "callout", label: "Balloon", size: Size::Big, params: balloon_params, checked: None },
            Big { id: "sheet.renumber", icon: "numberShapes" },
        ],
    )
}
