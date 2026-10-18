//! Connector > Kind: draw the selected connector as a kind of the catalog (`connector.setKind`),
//! the connector's current kind checked.

use crate::ribbon::Control::*;
use crate::ribbon::{Group, kinds, selected_connector};
use crate::widgets::{big_label, menu_button};
use crate::CraftApp;

/// The Kind menu. Drawn by hand because its rows are the catalog and its mark follows the
/// selected connector.
fn kind_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "connectorKind", &big_label("Kind"), "Kind", true, |ui, app| {
        let ids: Vec<String> = selected_connector(app).map(|c| c.id.clone()).into_iter().collect();
        let current = selected_connector(app).map(|c| c.kind.clone());
        kinds::rows(ui, app, &ids, current.as_deref());
    });
}

/// The Connector tab's Kind group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Kind", vec![Custom(kind_menu)])
}
