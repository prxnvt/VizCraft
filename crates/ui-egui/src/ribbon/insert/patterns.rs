//! Insert > Patterns: the Patterns pane toggle and a gallery dropdown of the active pack's
//! patterns, each opening its form (`ui.patterns {pattern}`).

use serde_json::json;
use vizcraft_engine::cmd::pattern::patterns as vizcraft_patterns;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The pack the gallery shows: the active page's viewpoint pack, else the first pack the model
/// uses, else every pack with patterns.
fn pack(app: &CraftApp) -> Option<String> {
    let vp = app.session.doc.page().and_then(|p| p.view.as_ref()).map(|v| vizcraft_diagram::vizcraft_model::pack_of(&v.viewpoint).to_string()).unwrap_or_default();
    if !vp.is_empty() {
        return Some(vp);
    }
    app.session.doc.model.packs.first().map(|p| p.id.clone())
}

/// The patterns the gallery offers: the active pack's, else every one a loaded pack offers.
fn offered(app: &CraftApp) -> Vec<&'static vizcraft_patterns::Pattern> {
    let cat = vizcraft_notation::catalog();
    let list = vizcraft_patterns::list(cat, pack(app).as_deref(), None);
    if list.is_empty() { vizcraft_patterns::available(cat) } else { list }
}

/// The pane toggle above the gallery: a dropdown with one row per pattern, each opening its form.
/// A dropdown rather than a row of buttons keeps the Insert tab inside 1280 points.
fn gallery(app: &mut CraftApp, ui: &mut egui::Ui) {
    let open = app.ui.panes.is_open("patterns");
    ui.vertical(|ui| {
        crate::widgets::toggle(ui, app, "panelLeft", Some("Pane"), "Show the Patterns pane", "ui.pane.patterns", json!({}), open);
        crate::widgets::menu_button(ui, app, "pattern", "Insert", "Insert a pattern of the active notation", false, |ui, app| {
            for p in offered(app) {
                crate::widgets::menu_item(ui, app, &p.name, "ui.patterns", json!({"pattern": p.id}), None);
            }
        });
    });
}

/// The Insert tab's Patterns group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Patterns", vec![Custom(gallery)])
}
