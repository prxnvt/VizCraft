//! What the page's notation says about layout and flow: the direction a typed chain grows in, the
//! layouts the notation's own templates use, and whether the page is a sequence page (where the
//! sequence tools own the keyboard). The rules live here so keys, the picker and the ribbon agree.

use vizcraft_engine::suggest::Direction;

use crate::CraftApp;

/// Whether the active page is a sequence page (`Page.sequence` set). Tab and Enter do nothing
/// there: the sequence tools own the keyboard flow.
pub fn sequence_page(app: &CraftApp) -> bool {
    app.session.doc.page().is_some_and(|p| p.sequence.is_some())
}

/// The insertion direction a template flow direction (`TB`, `LR`, ...) names.
fn from_flow(direction: &str) -> Option<Direction> {
    match direction {
        "TB" | "TD" => Some(Direction::Down),
        "LR" => Some(Direction::Right),
        "BT" => Some(Direction::Up),
        "RL" => Some(Direction::Left),
        _ => None,
    }
}

/// The page's viewpoint id, empty for a free-drawing page.
fn viewpoint(app: &CraftApp) -> String {
    app.session.doc.page().and_then(|p| p.view.as_ref()).map(|v| v.viewpoint.clone()).unwrap_or_default()
}

/// What `auto` resolves to: the flow direction of the first template of the page's viewpoint that
/// sets one (TB is down, LR right, BT up, RL left), else right, which `insert.next` defaults to.
pub fn auto_direction(app: &CraftApp) -> Direction {
    let vp = viewpoint(app);
    if vp.is_empty() {
        return Direction::Right;
    }
    let cat = vizcraft_notation::catalog();
    let pack = vizcraft_diagram::vizcraft_model::pack_of(&vp);
    cat.pack(pack)
        .and_then(|p| {
            p.templates.iter().filter(|t| t.viewpoint == vp).find_map(|t| t.layout.as_ref().and_then(|l| l.direction.as_deref()).and_then(from_flow))
        })
        .unwrap_or(Direction::Right)
}

/// The direction Tab, Enter and the picker grow a chain in: the user's `insert_direction` choice
/// (`right`, `down`, `left`, `up`), else on `auto` the notation's flow, else right.
pub fn flow_direction(app: &CraftApp) -> Direction {
    Direction::parse(&app.ui.insert_direction).unwrap_or_else(|| auto_direction(app))
}
