//! Hyperlinks and screen tips on the canvas: resting the pointer on a shape shows its screen tip
//! (or, with none, what its first link points at) after the usual tooltip delay, and Mod+click
//! follows the shape's first link. Following runs `link.follow`, so a script does the same.

use egui::{Response, Ui};
use serde_json::json;
use vizcraft_diagram::{self as dg, Hit};

use super::{HIT, Viewport2D, point};
use crate::CraftApp;

/// The link line a tooltip shows when the shape has links but no screen tip.
fn link_hint(sh: &dg::Shape) -> Option<String> {
    let l = sh.links.first()?;
    let what = if l.label.is_empty() { l.target.as_str() } else { l.label.as_str() };
    let more = if sh.links.len() > 1 { format!(" (+{} more)", sh.links.len() - 1) } else { String::new() };
    Some(format!("{what}{more}\nCtrl+click to follow"))
}

/// Show the tooltip for the shape under the pointer and follow its link on Mod+click. Returns
/// true when the click was used, so the caller skips its own click handling.
pub fn pointer(app: &mut CraftApp, ui: &Ui, response: &Response, vp: &Viewport2D) -> bool {
    let Some(hp) = response.hover_pos() else { return false };
    let Some(page) = app.session.doc.page() else { return false };
    let Some(Hit::Shape(id)) = dg::hit(page, point(vp.screen_to_world(hp)), f64::from(HIT / vp.zoom)) else { return false };
    let Some(sh) = page.shape(&id) else { return false };
    if app.canvas.drag.is_none() {
        let tip = if sh.screen_tip.trim().is_empty() { link_hint(sh) } else { Some(sh.screen_tip.clone()) };
        if let Some(tip) = tip {
            response.clone().on_hover_text(tip);
        }
    }
    let follow = !sh.links.is_empty() && response.clicked_by(egui::PointerButton::Primary) && ui.input(|i| i.modifiers.command);
    if follow {
        let _ = app.run("link.follow", json!({"id": id}));
    }
    follow
}
