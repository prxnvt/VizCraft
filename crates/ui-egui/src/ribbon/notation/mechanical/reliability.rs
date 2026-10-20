//! Reliability > Reliability Analysis: Cut Sets (`rel.cutsets`), Availability
//! (`rel.availability`) and Outcomes (`rel.outcomes`), each filling the Analysis dock; and FMEA:
//! Add Failure Mode (an `element.create` owned by the selected worksheet, so it appears as a row)
//! and Recompute RPN (`rel.rpn`).

use serde_json::{Value, json};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Reliability Analysis group.
pub(super) fn analyze() -> Group {
    Group::new(
        "Reliability Analysis",
        vec![Stack(vec![
            Small { id: "rel.cutsets", icon: "cutSets" },
            Small { id: "rel.availability", icon: "availability" },
            Small { id: "rel.outcomes", icon: "outcomes" },
        ])],
    )
}

/// The worksheet a new row goes in: the selected one, else the page's first.
fn worksheet(app: &CraftApp) -> Option<String> {
    let doc = &app.session.doc;
    let is_sheet = |id: &String| doc.model.element(id).is_some_and(|e| e.type_id == "reliability.FmeaWorksheet");
    crate::ribbon::selected_shape(app)
        .and_then(|s| s.element.clone())
        .filter(is_sheet)
        .or_else(|| doc.page()?.shapes.iter().filter_map(|s| s.element.clone()).find(is_sheet))
}

fn add_row(app: &CraftApp) -> Value {
    // With no worksheet the owner names nothing, so the command says why it cannot add the row.
    let owner = worksheet(app).unwrap_or_else(|| "(no FMEA worksheet)".into());
    json!({"type": "reliability.FailureMode", "name": "New failure mode", "owner": owner})
}

/// The FMEA group.
pub(super) fn fmea() -> Group {
    Group::new(
        "FMEA",
        vec![Stack(vec![
            Button { id: "element.create", icon: "fmeaRow", label: "Add Failure Mode", size: Size::Small, params: add_row, checked: None },
            Small { id: "rel.rpn", icon: "rpn" },
        ])],
    )
}
