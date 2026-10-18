//! The contextual Connector tab, shown while a connector is selected: kind, route, arrows and style.

use super::Tab;
use crate::CraftApp;
use crate::ribbon::selected_connector;

fn has_connector_selected(app: &CraftApp) -> bool {
    selected_connector(app).is_some()
}

groups! {
    kind,
    route,
    arrows,
    style,
}

/// The Connector tab.
pub(super) fn tab() -> Tab {
    Tab::contextual("Connector", has_connector_selected, groups())
}
