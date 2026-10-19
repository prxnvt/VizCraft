//! Model > Analyze: graph analyses over the model (`analyze.*`), run with the options the user
//! picks here and in the Analysis tab (depth, the relationship types followed, containment).
//! Each button sends the options it needs and leaves out the ones at their defaults; the result
//! shows on the canvas and in the dock's Analysis tab.

use serde_json::{Value, json};

use super::model::selected_elements;
use crate::CraftApp;
use crate::dock::analysis::with_options;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};
use crate::widgets::{menu_button, menu_item};

/// Depths the Depth field offers, besides typing one.
const DEPTHS: &[&str] = &["1", "2", "3", "5", "10", "All"];

fn upstream(a: &CraftApp) -> Value {
    with_options(a, "analyze.dependencies", json!({"direction": "upstream"}))
}

fn downstream(a: &CraftApp) -> Value {
    with_options(a, "analyze.dependencies", json!({"direction": "downstream"}))
}

fn both(a: &CraftApp) -> Value {
    with_options(a, "analyze.dependencies", json!({"direction": "both"}))
}

/// Path between the first two selected elements, in the order they were selected. With fewer
/// than two the engine's message reaches the status bar.
fn path(a: &CraftApp) -> Value {
    let els = selected_elements(a);
    let mut p = json!({});
    if let (Some(from), Some(to)) = (els.first(), els.get(1)) {
        p = json!({"from": from, "to": to});
    }
    with_options(a, "analyze.path", p)
}

fn metrics(a: &CraftApp) -> Value {
    with_options(a, "analyze.metrics", json!({}))
}

fn cycles(a: &CraftApp) -> Value {
    with_options(a, "analyze.cycles", json!({}))
}

fn unreachable(a: &CraftApp) -> Value {
    with_options(a, "analyze.unreachable", json!({}))
}

fn spof(a: &CraftApp) -> Value {
    with_options(a, "analyze.spof", json!({}))
}

fn depths(_: &CraftApp) -> Vec<String> {
    DEPTHS.iter().map(|d| d.to_string()).collect()
}

fn depth(a: &CraftApp) -> String {
    a.ui.dock.state.analysis.depth.map_or_else(|| "All".to_string(), |d| d.to_string())
}

/// Follow: which links the analyses walk. One checkable row per relationship type the model
/// uses (none checked is every link), then whether containment counts as a link.
fn follow_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "dependencies", "Follow", "Follow", false, |ui, app| {
        let st = app.ui.dock.state.analysis.clone();
        menu_item(ui, app, "All links", "ui.analysis", json!({"types": []}), Some(st.types.is_empty()));
        let cat = vizcraft_notation::catalog();
        let mut used: Vec<String> = Vec::new();
        for r in &app.session.doc.model.relationships {
            if !used.contains(&r.type_id) && !r.type_id.is_empty() {
                used.push(r.type_id.clone());
            }
            if used.len() >= 40 {
                break;
            }
        }
        if !used.is_empty() {
            ui.separator();
        }
        for ty in used {
            let label = cat.relationship_type(&ty).map(|(_, t)| t.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| ty.clone());
            let on = st.types.contains(&ty);
            let mut next = st.types.clone();
            if on {
                next.retain(|x| *x != ty);
            } else {
                next.push(ty.clone());
            }
            menu_item(ui, app, &label, "ui.analysis", json!({"types": next}), Some(on));
        }
        ui.separator();
        menu_item(ui, app, "Include containment", "ui.analysis", json!({"containment": !st.containment}), Some(st.containment));
    })
    .on_hover_text("The links the analyses follow, from source to target");
}

/// The Model tab's Analyze group.
pub(in crate::ribbon) fn group() -> Group {
    let deps = "analyze.dependencies";
    Group::new(
        "Analyze",
        vec![
            Stack(vec![
                Button { id: deps, icon: "upstream", label: "Upstream", size: Size::Small, params: upstream, checked: None },
                Button { id: deps, icon: "downstream", label: "Downstream", size: Size::Small, params: downstream, checked: None },
                Button { id: deps, icon: "dependencies", label: "Both", size: Size::Small, params: both, checked: None },
            ]),
            Stack(vec![
                Row(vec![
                    Label { icon: "dependencies", text: "Depth" },
                    Combo { id: "ui.analysis", param: "depth", name: "Depth", width: 52.0, items: depths, current: depth },
                ]),
                Button { id: "analyze.path", icon: "path", label: "Path", size: Size::Small, params: path, checked: None },
                Button { id: "analyze.metrics", icon: "metrics", label: "Metrics", size: Size::Small, params: metrics, checked: None },
            ]),
            Stack(vec![
                Button { id: "analyze.cycles", icon: "cycle", label: "Cycles", size: Size::Small, params: cycles, checked: None },
                Button { id: "analyze.unreachable", icon: "unreachable", label: "Unreachable", size: Size::Small, params: unreachable, checked: None },
                Button { id: "analyze.spof", icon: "spof", label: "Single Points of Failure", size: Size::Small, params: spof, checked: None },
            ]),
            Custom(follow_menu),
        ],
    )
}
