//! The Steps tool: numbering connectors in the order of a request. A click on a connector runs
//! `connector.setStep` with the next number on the page (one more than the highest); a Shift-click
//! clears its step. Escape returns to the pointer (`keys`). The badges themselves are drawn with
//! the connectors.

use egui::{Response, Ui};
use serde_json::json;
use vizcraft_diagram::{self as dg, Hit};

use super::{HIT, run};
use crate::canvas::Viewport2D;
use crate::canvas::draw::point;
use crate::{CraftApp, Tool};

/// The highest step on the active page (0 when none is numbered).
pub fn highest(app: &CraftApp) -> u16 {
    app.session.doc.page().map_or(0, |p| p.connectors.iter().map(|c| c.step).max().unwrap_or(0))
}

/// The status bar text while the tool is armed: the number the next click gives.
pub fn status(app: &CraftApp) -> Option<String> {
    (app.tool == Tool::Steps).then(|| format!("Step {}: click a connector", highest(app).saturating_add(1)))
}

/// Handle a click while the Steps tool is armed. Returns true when the tool took the frame's
/// pointer input.
pub fn pointer(app: &mut CraftApp, ui: &Ui, response: &Response, vp: &Viewport2D) -> bool {
    if app.tool != Tool::Steps {
        return false;
    }
    if response.clicked_by(egui::PointerButton::Primary)
        && let Some(p) = response.interact_pointer_pos()
    {
        let shift = ui.input(|i| i.modifiers.shift);
        let hit = app.session.doc.page().and_then(|page| dg::hit(page, point(vp.screen_to_world(p)), f64::from(HIT / vp.zoom)));
        if let Some(Hit::Connector(id)) = hit {
            let step = if shift { None } else { Some(highest(app).saturating_add(1)) };
            run(app, "connector.setStep", json!({"id": id, "step": step}));
        }
    }
    // Every other press does nothing with this tool (no marquee, no moves).
    true
}
