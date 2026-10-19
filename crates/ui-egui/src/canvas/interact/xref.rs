//! Cross-reference labels on the canvas: the location labels the sheet pass writes beside an
//! element's views (`ext["vizcraft.xref.for"]`, see `vizcraft_notation::sheet_pass`). A click on
//! one runs `xref.go`, which shows the partner's page and selects it; hovering one shows its
//! partners from `xref.list` in a tooltip. Shift-click still selects the label.

use egui::Response;
use serde_json::{Value, json};
use vizcraft_diagram::Hit;
use vizcraft_notation::sheet_pass::EXT_XREF_FOR;

use crate::CraftApp;

/// The cross-reference label under the pointer and the view it labels.
fn label_under_pointer(app: &CraftApp) -> Option<(String, String)> {
    let Some(Hit::Shape(id)) = &app.canvas.hover else { return None };
    let s = app.session.doc.page()?.shape(id)?;
    let view = s.ext.get(EXT_XREF_FOR)?;
    Some((s.id.clone(), view.clone()))
}

/// The tooltip for a label of view `view`: one line per partner location, from `xref.list`.
fn tip_text(app: &mut CraftApp, view: &str) -> String {
    let r = app.session.run("xref.list", &json!({"element": view})).unwrap_or(Value::Null);
    let lines: Vec<String> = r["xrefs"]
        .as_array()
        .map(|a| {
            a.iter()
                .take(20)
                .map(|x| {
                    let tag = x["to"]["tag"].as_str().unwrap_or_default();
                    let text = x["text"].as_str().unwrap_or_default();
                    if tag.is_empty() { text.to_string() } else { format!("{tag} at {text}") }
                })
                .collect()
        })
        .unwrap_or_default();
    if lines.is_empty() { "No cross references".to_string() } else { format!("Go to {}", lines.join(", ")) }
}

/// Tooltip and click handling for cross-reference labels; true when a click went to `xref.go`
/// (the canvas does nothing else with it).
pub fn pointer(app: &mut CraftApp, response: &Response) -> bool {
    let Some((label, view)) = label_under_pointer(app) else {
        app.canvas.xref_tip = None;
        return false;
    };
    if app.canvas.xref_tip.as_ref().is_none_or(|(id, _)| *id != label) {
        let text = tip_text(app, &view);
        app.canvas.xref_tip = Some((label.clone(), text));
    }
    if app.canvas.drag.is_none()
        && let Some((_, text)) = &app.canvas.xref_tip
    {
        response.clone().on_hover_text_at_pointer(text.as_str());
    }
    let shift = response.ctx.input(|i| i.modifiers.shift);
    if app.tool == crate::Tool::Pointer && response.clicked_by(egui::PointerButton::Primary) && !shift {
        let _ = app.run("xref.go", json!({"id": label}));
        return true;
    }
    false
}
