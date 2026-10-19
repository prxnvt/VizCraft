//! Model > Compare: compare the model with the saved file or another file (`model.diff`), select
//! or clear what the last analysis found, and show its rows in the dock. Comparing with git HEAD
//! is not offered: the engine does not support it yet.

use serde_json::{Value, json};

use crate::CraftApp;
use crate::dock::DockTab;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size};

fn results_open(a: &CraftApp) -> bool {
    a.ui.dock.open && a.ui.dock.tab == DockTab::Analysis
}

/// Results shows the Analysis tab, or closes the dock when it already shows it.
fn results(a: &CraftApp) -> Value {
    if results_open(a) { json!({"open": false}) } else { json!({"tab": "analysis"}) }
}

/// The Model tab's Compare group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Compare",
        vec![
            Split {
                id: "model.diff",
                icon: "compare",
                big: true,
                menu: vec![
                    MenuEntry::new("With Saved File", "model.diff", json!({"against": "saved"})),
                    MenuEntry::new("With File...", "ui.compareFile", json!({})),
                ],
            },
            Stack(vec![
                Small { id: "analyze.select", icon: "selectAll" },
                Button { id: "analyze.clear", icon: "eraser", label: "Clear Highlights", size: Size::Small, params: |_| json!({}), checked: None },
                Button { id: "ui.dock", icon: "analysisResults", label: "Results", size: Size::Small, params: results, checked: Some(results_open) },
            ]),
        ],
    )
}
