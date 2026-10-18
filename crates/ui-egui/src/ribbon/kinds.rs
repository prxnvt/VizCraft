//! The connector kind menu shared by Design > Connectors (Default Kind) and the Connector tab
//! (Kind): one row per kind of the catalog in `vizcraft_stencils`, grouped by category, each
//! running `connector.setKind` on explicit ids so it works with nothing selected too.

use egui::{RichText, ScrollArea, Ui};
use serde_json::json;
use vizcraft_stencils::connector_kinds;

use crate::CraftApp;
use crate::widgets::menu_item;

/// Tallest the kind list grows before it scrolls.
const MAX_H: f32 = 420.0;

/// The connectors a kind menu acts on: the selected ones, else every connector on the page
/// (`connector.setKind` alone refuses with nothing selected, so the ids go in the params).
pub(super) fn target_ids(app: &CraftApp) -> Vec<String> {
    let Some(page) = app.session.doc.page() else { return Vec::new() };
    let selected: Vec<String> = app.session.sel.items.iter().filter(|id| page.connector(id).is_some()).cloned().collect();
    if selected.is_empty() { page.connectors.iter().map(|c| c.id.clone()).collect() } else { selected }
}

/// The kind every target connector has, if they all share one; `None` when they differ or there
/// are no targets.
pub(super) fn shared_kind(app: &CraftApp, ids: &[String]) -> Option<String> {
    let page = app.session.doc.page()?;
    let mut kinds = ids.iter().filter_map(|id| page.connector(id)).map(|c| c.kind.as_str());
    let first = kinds.next()?;
    kinds.all(|k| k == first).then(|| first.to_string())
}

/// Draw the kind rows into an open menu: the catalog by category, the kind in `current` checked.
pub(super) fn rows(ui: &mut Ui, app: &mut CraftApp, ids: &[String], current: Option<&str>) {
    ScrollArea::vertical().max_height(MAX_H).show(ui, |ui| {
        let mut category = "";
        for k in connector_kinds() {
            if k.category != category {
                category = k.category;
                ui.label(RichText::new(category).small().weak());
            }
            menu_item(ui, app, k.label, "connector.setKind", json!({"ids": ids, "kind": k.id}), Some(current == Some(k.id)));
        }
    });
}
