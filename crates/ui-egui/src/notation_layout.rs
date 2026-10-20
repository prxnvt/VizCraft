//! What the page's notation says about layout and flow: the direction a typed chain grows in, the
//! layouts the notation's document types and examples use, and whether the page is a sequence page (where the
//! sequence tools own the keyboard). The rules live here so keys, the picker and the ribbon agree.

use vizcraft_engine::suggest::Direction;

use crate::CraftApp;

/// Whether the active page is a sequence page (`Page.sequence` set). Tab and Enter do nothing
/// there: the sequence tools own the keyboard flow.
pub fn sequence_page(app: &CraftApp) -> bool {
    app.session.doc.page().is_some_and(|p| p.sequence.is_some())
}

/// Whether the active page is an electrical page (its viewpoint is the electrical pack's). Tab and
/// Enter do nothing there: devices are joined by wires drawn with W, not by a chain of
/// relationships.
pub fn wiring_page(app: &CraftApp) -> bool {
    app.session.doc.page().and_then(|p| p.view.as_ref()).is_some_and(|v| v.viewpoint.starts_with("electrical."))
}

/// Whether the active page is a process plant page (P&ID, PFD, loop, HVAC, plumbing, isometric).
/// Tab and Enter do nothing there: plant items are joined by the Pipe tool and signal lines, not
/// grown as a chain, and a line number is composed, never typed.
pub fn process_page(app: &CraftApp) -> bool {
    crate::palette::active_viewpoint(app).is_some_and(|v| v.starts_with("process."))
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

/// What `auto` resolves to: the flow direction of the first document type, then example, of the
/// page's viewpoint that sets one (TB is down, LR right, BT up, RL left), else right, which
/// `insert.next` defaults to. A document type's layout direction is where its chains grow.
pub fn auto_direction(app: &CraftApp) -> Direction {
    let vp = viewpoint(app);
    if vp.is_empty() {
        return Direction::Right;
    }
    let cat = vizcraft_notation::catalog();
    let pack = vizcraft_diagram::vizcraft_model::pack_of(&vp);
    cat.pack(pack)
        .and_then(|p| for_viewpoint(p, &vp).find_map(|t| t.layout.as_ref().and_then(|l| l.direction.as_deref()).and_then(from_flow)))
        .unwrap_or(Direction::Right)
}

/// The direction Tab, Enter and the picker grow a chain in: the user's `insert_direction` choice
/// (`right`, `down`, `left`, `up`), else on `auto` the notation's flow, else right.
pub fn flow_direction(app: &CraftApp) -> Direction {
    Direction::parse(&app.ui.insert_direction).unwrap_or_else(|| auto_direction(app))
}

/// One layout the page's notation offers: what the Re-Layout menu lists first.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutChoice {
    /// The menu text, such as "Layered, Left to Right (State machine)".
    pub label: String,
    /// The command it runs: `layout.full` or `layout.primitive`.
    pub command: &'static str,
    /// Its params.
    pub params: serde_json::Value,
}

/// The flow direction a template layout names, as words.
fn direction_words(direction: &str) -> Option<&'static str> {
    match direction {
        "TB" | "TD" => Some("Top to Bottom"),
        "LR" => Some("Left to Right"),
        "BT" => Some("Bottom to Top"),
        "RL" => Some("Right to Left"),
        _ => None,
    }
}

/// The words for a layout primitive.
fn primitive_words(name: &str) -> String {
    match name {
        "pipeline" => "Pipeline",
        "bands" => "Bands",
        "chain" => "Chain",
        "matrix" => "Matrix",
        "nested" => "Nested containers",
        "hubSpoke" => "Hub and spoke",
        "rings" => "Rings",
        "timeline" => "Timeline",
        "voter" => "Voter",
        "sequence" => "Sequence",
        "er" => "Entity-relationship",
        other => other,
    }
    .to_string()
}

/// The label of the page's viewpoint, shown as the title of the notation's layout section; `None`
/// on a free-drawing page.
pub fn viewpoint_label(app: &CraftApp) -> Option<String> {
    let vp = viewpoint(app);
    let pack = vizcraft_notation::catalog().pack(vizcraft_diagram::vizcraft_model::pack_of(&vp))?;
    pack.viewpoint(&vp).map(|v| v.label.clone())
}

/// The document types, then the examples, of `pack` that start on viewpoint `vp` (a type also
/// counts for the viewpoints it offers as choices).
fn for_viewpoint<'a>(pack: &'a vizcraft_notation::Pack, vp: &'a str) -> impl Iterator<Item = &'a vizcraft_notation::Template> {
    pack.templates.iter().filter(move |t| t.offers_viewpoint(vp)).chain(pack.examples.iter().filter(move |t| t.viewpoint == vp))
}

/// The layouts the page's notation uses: the distinct layouts of its viewpoint's document types
/// and examples, with their roles dropped so a primitive uses its page defaults. Empty on a
/// free-drawing page.
pub fn layout_choices(app: &CraftApp) -> Vec<LayoutChoice> {
    let vp = viewpoint(app);
    if vp.is_empty() {
        return Vec::new();
    }
    let Some(pack) = vizcraft_notation::catalog().pack(vizcraft_diagram::vizcraft_model::pack_of(&vp)) else { return Vec::new() };
    let mut out: Vec<LayoutChoice> = Vec::new();
    for t in for_viewpoint(pack, &vp) {
        let Some(l) = &t.layout else { continue };
        let dir = l.direction.as_deref().filter(|d| direction_words(d).is_some());
        let choice = match (l.primitive.as_deref(), l.algorithm.as_deref()) {
            (Some(prim), _) => {
                let mut params = serde_json::json!({"primitive": prim});
                if let (Some(o), Some(d)) = (params.as_object_mut(), dir) {
                    o.insert("direction".into(), serde_json::json!(d));
                }
                LayoutChoice { label: primitive_words(prim), command: "layout.primitive", params }
            }
            (None, Some(alg)) => {
                let name = match alg {
                    "layered" => "Layered",
                    "tree" => "Tree",
                    "radial" => "Radial",
                    _ => "Force-Directed",
                };
                let mut params = serde_json::json!({"algorithm": alg});
                let mut label = name.to_string();
                if let Some(d) = dir {
                    if let Some(o) = params.as_object_mut() {
                        o.insert("direction".into(), serde_json::json!(d));
                    }
                    label = format!("{name}, {}", direction_words(d).unwrap_or_default());
                }
                LayoutChoice { label: format!("{label} ({})", t.label), command: "layout.full", params }
            }
            (None, None) => continue,
        };
        if !out.iter().any(|c| c.command == choice.command && c.params == choice.params) {
            out.push(choice);
        }
    }
    out
}
