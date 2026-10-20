//! Circuit > Sequence: the displacement-step chart: make one for the page's actuators, add a step
//! after the last or remove the last from the selected chart.

use serde_json::{Value, json};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The selected shape (the chart the step buttons edit) and its number of state columns.
fn chart(app: &CraftApp) -> (Option<String>, usize) {
    let Some(id) = app.session.sel.primary.clone() else { return (None, 0) };
    let doc = &app.session.doc;
    let rows = doc
        .page()
        .and_then(|p| p.shape(&id))
        .and_then(|sh| sh.element.as_deref())
        .and_then(|e| doc.model.element(e))
        .and_then(|e| e.prop("rows"))
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    (Some(id), vizcraft_notation::fluid::steps::parse(&rows).states())
}

fn insert_params(app: &CraftApp) -> Value {
    json!({"chart": chart(app).0})
}

fn delete_params(app: &CraftApp) -> Value {
    let (id, n) = chart(app);
    json!({"chart": id, "at": n.saturating_sub(1)})
}

/// The Sequence group.
pub(super) fn group() -> Group {
    Group::new(
        "Sequence",
        vec![Stack(vec![
            Small { id: "fluid.stepChart", icon: "stepChart" },
            Button { id: "fluid.insertStep", icon: "stepInsert", label: "Insert Step", size: Size::Small, params: insert_params, checked: None },
            Button { id: "fluid.deleteStep", icon: "stepDelete", label: "Delete Step", size: Size::Small, params: delete_params, checked: None },
        ])],
    )
}
