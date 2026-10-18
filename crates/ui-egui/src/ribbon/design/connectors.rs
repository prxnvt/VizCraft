//! Design > Connectors: the page-wide connector style, and avoid shapes and line jumps for the
//! page or the selection.

use serde_json::json;
use vizcraft_diagram::{Connector, Route};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size};

/// Whether every connector the Design tab's toggles act on has `flag`: the selected connectors,
/// else all on the page (the same fallback as `connector.reroute` and `connector.setJumps`).
/// False when there are none.
fn all_connectors(app: &CraftApp, flag: fn(&Connector) -> bool) -> bool {
    let Some(page) = app.session.doc.page() else { return false };
    let selected: Vec<&Connector> = app.session.sel.items.iter().filter_map(|id| page.connector(id)).collect();
    if selected.is_empty() { !page.connectors.is_empty() && page.connectors.iter().all(flag) } else { selected.into_iter().all(flag) }
}

/// Whether every connector on the page has `route`. The style command re-routes the whole page
/// whatever is selected, so the mark follows the page, not the selection. False with no connectors.
fn page_route_is(app: &CraftApp, route: Route) -> bool {
    app.session.doc.page().is_some_and(|p| !p.connectors.is_empty() && p.connectors.iter().all(|c| c.route == route))
}

/// The page connector style menu: every connector on the page re-routed in one style.
fn style_menu() -> Vec<MenuEntry> {
    vec![
        MenuEntry::new("Right Angle", "design.setConnectorStyle", json!({"style": "rightAngle"})).checked(|a| page_route_is(a, Route::Orthogonal)),
        MenuEntry::new("Straight", "design.setConnectorStyle", json!({"style": "straight"})).checked(|a| page_route_is(a, Route::Straight)),
        MenuEntry::new("Curved", "design.setConnectorStyle", json!({"style": "curved"})).checked(|a| page_route_is(a, Route::Curved)),
    ]
}

/// The Design tab's Connectors group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Connectors",
        vec![Stack(vec![
            Menu { label: "Connector Style", icon: "routeOrthogonal", big: false, menu: style_menu() },
            Button {
                id: "connector.reroute",
                icon: "routeAround",
                label: "Avoid Shapes",
                size: Size::Small,
                params: |a| json!({"avoid": !all_connectors(a, |c| c.avoid)}),
                checked: Some(|a| all_connectors(a, |c| c.avoid)),
            },
            Button {
                id: "connector.setJumps",
                icon: "lineJumps",
                label: "Line Jumps",
                size: Size::Small,
                params: |a| json!({"on": !all_connectors(a, |c| c.jumps)}),
                checked: Some(|a| all_connectors(a, |c| c.jumps)),
            },
        ])],
    )
}
