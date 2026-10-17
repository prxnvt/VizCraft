//! Auto-layout commands (`layout.*`): rearrange a page or a selection with the layered, tree,
//! radial or force-directed layout, place new shapes around ones that stay put, and route
//! connectors around shapes without moving any shape.

use serde_json::{Value, json};
use vizcraft_diagram::autolayout::{self, Algorithm, Direction, Options};
use vizcraft_diagram::autoroute;

use super::connector::connector_targets;
use super::{Kinds, has_connectors, has_shapes, ids_param, num, opt_str, page_mut, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Largest spacing a caller may ask for, in world units.
const MAX_SPACING: f64 = 10_000.0;

/// The layout commands, in menu order.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("layout.full", "Re-Layout", "Design > Layout", full)
            .params(concat!(
                r#"{"ids"?: [string], "algorithm"?: "layered"|"tree"|"radial"|"force" (default layered), "direction"?: "TB"|"BT"|"LR"|"RL" (also "TD"), "nodeSpacing"?: n, "layerSpacing"?: n} -> {"ids", "notes"}"#,
                r#"; rearranges `ids`, else the selected shapes when two or more are selected, else the whole page"#
            ))
            .when(has_shapes),
        CommandSpec::new("layout.incremental", "Place New Shapes", "Design > Layout", incremental)
            .params(concat!(
                r#"{"ids": [string], "algorithm"?: "layered"|"tree"|"radial"|"force", "direction"?: "TB"|"BT"|"LR"|"RL" (also "TD"), "nodeSpacing"?: n, "layerSpacing"?: n} -> {"ids", "notes"}"#,
                r#"; places only the shapes in `ids` next to the shapes they connect to; every other shape stays where it is"#
            ))
            .when(has_shapes),
        CommandSpec::new("layout.route", "Route Connectors", "Design > Layout", route)
            .params(r#"{"ids"?: [string]} -> {"ids"}; routes the connectors (default: the selected ones, else all) around shapes, orthogonal, and keeps them clear after every change; never moves a shape"#)
            .when(has_connectors),
    ]
}

/// The layout options from the params: algorithm, direction and the two spacings.
fn options(v: &Value) -> Result<Options, EngineError> {
    let mut o = Options::default();
    if let Some(a) = opt_str(v, "algorithm")? {
        o.algorithm = match a.to_ascii_lowercase().as_str() {
            "layered" => Algorithm::Layered,
            "tree" => Algorithm::Tree,
            "radial" => Algorithm::Radial,
            "force" => Algorithm::Force,
            _ => return Err(EngineError::Params("`algorithm` must be layered, tree, radial or force".into())),
        };
    }
    if let Some(d) = opt_str(v, "direction")? {
        o.direction = match d.to_ascii_uppercase().as_str() {
            "TB" | "TD" => Direction::TopBottom,
            "BT" => Direction::BottomTop,
            "LR" => Direction::LeftRight,
            "RL" => Direction::RightLeft,
            _ => return Err(EngineError::Params("`direction` must be TB, BT, LR or RL".into())),
        };
    }
    for (key, slot) in [("nodeSpacing", &mut o.node_spacing), ("layerSpacing", &mut o.layer_spacing)] {
        if let Some(n) = num(v, key)? {
            if !(0.0..=MAX_SPACING).contains(&n) {
                return Err(EngineError::Params(format!("`{key}` must be between 0 and {MAX_SPACING}")));
            }
            *slot = n;
        }
    }
    Ok(o)
}

fn full(s: &mut Session, v: &Value) -> CmdResult {
    let opts = options(v)?;
    let ids: Option<Vec<String>> = if ids_param(v)?.is_some() {
        Some(targets(s, v, Kinds::Shapes)?)
    } else {
        let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        let selected: Vec<String> = s.sel.items.iter().filter(|id| page.shape(id).is_some()).cloned().collect();
        (selected.len() >= 2).then_some(selected)
    };
    let page = page_mut(s)?;
    let notes = autolayout::layout_items(page, ids.as_deref(), &opts).map_err(EngineError::Failed)?;
    let ids = ids.unwrap_or_else(|| page.shapes.iter().map(|sh| sh.id.clone()).collect());
    with_sel(s, json!({"ids": ids, "notes": notes}))
}

fn incremental(s: &mut Session, v: &Value) -> CmdResult {
    if ids_param(v)?.is_none() {
        return Err(EngineError::Params("`ids` (the new shapes to place) is required".into()));
    }
    let ids = targets(s, v, Kinds::Shapes)?;
    let opts = options(v)?;
    let notes = autolayout::layout_incremental(page_mut(s)?, &ids, &opts).map_err(EngineError::Failed)?;
    with_sel(s, json!({"ids": ids, "notes": notes}))
}

fn route(s: &mut Session, v: &Value) -> CmdResult {
    let ids = connector_targets(s, v)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
            c.avoid = true;
            c.route = vizcraft_diagram::Route::Orthogonal;
        }
    }
    autoroute::reroute(page, Some(&ids));
    with_sel(s, json!({"ids": ids}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::autoroute::clear_of_shapes;
    use vizcraft_diagram::{Connector, Document, End, Page, Point, Route, Shape, ShapeKind};

    use crate::Session;

    /// Two shapes with a wall between them and an orthogonal connector across the wall.
    fn walled() -> Session {
        let mut page = Page::new("p1", "Page-1");
        page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 0.0, 100.0, 60.0, 40.0));
        page.shapes.push(Shape::new("wall", ShapeKind::Rectangle, 150.0, 0.0, 40.0, 260.0));
        page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 300.0, 100.0, 60.0, 40.0));
        let mut c = Connector::new("c", End::dynamic("a"), End::dynamic("b"));
        c.route = Route::Orthogonal;
        page.connectors.push(c);
        let mut doc = Document::new();
        doc.pages = vec![page];
        Session::new(doc)
    }

    fn clear(s: &Session) -> bool {
        let page = s.doc.page().unwrap();
        clear_of_shapes(page, page.connector("c").unwrap())
    }

    #[test]
    fn layout_route_avoids_shapes_and_a_move_reroutes_in_one_undo_step() {
        let mut s = walled();
        assert!(!clear(&s));
        let out = s.run("layout.route", &json!({})).unwrap();
        assert_eq!(out["ids"], json!(["c"]));
        assert!(clear(&s));
        let c = s.doc.page().unwrap().connector("c").unwrap();
        assert!(c.avoid && !c.waypoints.is_empty());
        let bends = c.waypoints.clone();
        // Move the wall: the route follows in the same step.
        s.run("shape.move", &json!({"ids": ["wall"], "dx": 0, "dy": 80})).unwrap();
        assert_eq!(s.undo_label(), Some("Move"));
        assert!(clear(&s));
        assert_ne!(s.doc.page().unwrap().connector("c").unwrap().waypoints, bends);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().connector("c").unwrap().waypoints, bends, "undo restores the old bends");
        assert_eq!(s.doc.page().unwrap().shape("wall").unwrap().y, 0.0);
        assert_eq!(s.undo_label(), Some("Route Connectors"), "the move and its reroute were one step");
    }

    #[test]
    fn connector_reroute_and_jumps_toggle_per_connector() {
        let mut s = walled();
        s.run("connector.reroute", &json!({"ids": ["c"]})).unwrap();
        assert!(s.doc.page().unwrap().connector("c").unwrap().avoid);
        s.run("connector.reroute", &json!({"avoid": false})).unwrap();
        let c = s.doc.page().unwrap().connector("c").unwrap();
        assert!(!c.avoid && c.waypoints.is_empty());
        s.run("connector.setJumps", &json!({"on": true})).unwrap();
        assert!(s.doc.page().unwrap().connector("c").unwrap().jumps);
        assert!(s.run("connector.setJumps", &json!({})).is_err());
        assert!(s.run("connector.reroute", &json!({"ids": ["a"]})).is_err(), "a shape is not a connector");
    }

    #[test]
    fn full_layout_of_each_algorithm_leaves_no_overlaps_and_is_one_step() {
        for algorithm in ["layered", "tree", "radial", "force"] {
            let mut s = walled();
            let out = s.run("layout.full", &json!({"algorithm": algorithm, "direction": "LR", "nodeSpacing": 30})).unwrap();
            assert_eq!(out["ids"].as_array().unwrap().len(), 3);
            assert_eq!(s.undo_label(), Some("Re-Layout"));
            s.run("edit.undo", &json!({})).unwrap();
            assert!(!s.can_undo(), "one undo step");
            s.run("edit.redo", &json!({})).unwrap();
            let page = s.doc.page().unwrap();
            for (i, a) in page.shapes.iter().enumerate() {
                for b in page.shapes.iter().skip(i + 1) {
                    let (x, y) = (a.bounds(), b.bounds());
                    assert!(!(x.x0 < y.x1 && y.x0 < x.x1 && x.y0 < y.y1 && y.y0 < x.y1), "{algorithm}: {} and {} overlap", a.id, b.id);
                }
            }
        }
        let mut s = walled();
        assert!(s.run("layout.full", &json!({"algorithm": "spiral"})).is_err());
        assert!(s.run("layout.full", &json!({"direction": "up"})).is_err());
        assert!(s.run("layout.full", &json!({"nodeSpacing": -1})).is_err());
    }

    #[test]
    fn full_layout_uses_a_selection_of_two_or_more() {
        let mut s = walled();
        s.run("edit.select", &json!({"ids": ["a", "b"]})).unwrap();
        if s.sel.items.len() == 2 {
            let wall = s.doc.page().unwrap().shape("wall").unwrap().clone();
            let out = s.run("layout.full", &json!({})).unwrap();
            assert_eq!(out["ids"], json!(["a", "b"]));
            assert_eq!(s.doc.page().unwrap().shape("wall").unwrap(), &wall);
        }
    }

    #[test]
    fn incremental_places_new_shapes_and_pins_the_rest() {
        let mut s = walled();
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": 0, "y": 0, "id": "n"})).unwrap();
        s.run("connector.add", &json!({"from": {"shape": "b"}, "to": {"shape": "n"}})).unwrap();
        let before: Vec<Point> = s.doc.page().unwrap().shapes.iter().take(3).map(|sh| Point::new(sh.x, sh.y)).collect();
        s.run("layout.incremental", &json!({"ids": ["n"], "direction": "LR"})).unwrap();
        let page = s.doc.page().unwrap();
        let after: Vec<Point> = page.shapes.iter().take(3).map(|sh| Point::new(sh.x, sh.y)).collect();
        assert_eq!(before, after);
        assert!(page.shape("n").unwrap().bounds().x0 >= page.shape("b").unwrap().bounds().x1);
        assert!(s.run("layout.incremental", &json!({})).is_err(), "ids are required");
    }
}
