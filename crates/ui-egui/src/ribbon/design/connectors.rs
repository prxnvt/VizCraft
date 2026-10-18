//! Design > Connectors: the page-wide connector style, the page's routing policy (Avoid Shapes,
//! Straight Lines), line jumps for the page or the selection, and the connector kind.

use serde_json::json;
use vizcraft_diagram::{Avoid, Connector, Route, StraightFallback};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size, kinds};
use crate::widgets::menu_button;

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

/// The page's Avoid Shapes policy: every connector routes around shapes, or only those that ask.
fn avoid_menu() -> Vec<MenuEntry> {
    vec![
        MenuEntry::new("Always", "page.setRouting", json!({"avoid": "always"}))
            .checked(|a| a.session.doc.page().is_some_and(|p| p.routing.avoid == Avoid::Always)),
        MenuEntry::new("Never", "page.setRouting", json!({"avoid": "never"}))
            .checked(|a| a.session.doc.page().is_some_and(|p| p.routing.avoid == Avoid::Never)),
    ]
}

/// What a straight connector does when a shape is in the way: draw the detour, or stay straight.
fn straight_menu() -> Vec<MenuEntry> {
    vec![
        MenuEntry::new("Detour", "page.setRouting", json!({"straightFallback": "detour"}))
            .checked(|a| a.session.doc.page().is_some_and(|p| p.routing.straight_fallback == StraightFallback::Detour)),
        MenuEntry::new("Keep Straight", "page.setRouting", json!({"straightFallback": "keep"}))
            .checked(|a| a.session.doc.page().is_some_and(|p| p.routing.straight_fallback == StraightFallback::Keep)),
    ]
}

/// Default Kind: draw the selected connectors, or every connector of the page when none is
/// selected, as a kind of the catalog.
fn kind_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "connectorKind", "Default Kind", "Default Kind", false, |ui, app| {
        let ids = kinds::target_ids(app);
        let current = kinds::shared_kind(app, &ids);
        kinds::rows(ui, app, &ids, current.as_deref());
    });
}

/// The Design tab's Connectors group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Connectors",
        vec![
            Stack(vec![
                Menu { label: "Connector Style", icon: "routeOrthogonal", big: false, menu: style_menu() },
                Menu { label: "Avoid Shapes", icon: "routeAround", big: false, menu: avoid_menu() },
                Menu { label: "Straight Lines", icon: "routeStraight", big: false, menu: straight_menu() },
            ]),
            Stack(vec![
                Button {
                    id: "connector.setJumps",
                    icon: "lineJumps",
                    label: "Line Jumps",
                    size: Size::Small,
                    params: |a| json!({"on": !all_connectors(a, |c| c.jumps)}),
                    checked: Some(|a| all_connectors(a, |c| c.jumps)),
                },
                Custom(kind_menu),
            ]),
        ],
    )
}
