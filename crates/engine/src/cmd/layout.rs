//! Auto-layout commands (`layout.*`): rearrange a page or a selection with the layered, tree,
//! radial or force-directed layout, place new shapes around ones that stay put, and route
//! connectors around shapes without moving any shape.

use serde_json::{Value, json};
use vizcraft_diagram::autolayout::{self, Algorithm, Direction, Options};
use vizcraft_diagram::{autoroute, decor};

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
                r#"; rearranges `ids`, else the selected shapes when two or more are selected, else the whole page; groups, containers and lane sets move whole, callouts follow their target, locked shapes stay"#
            ))
            .when(can_lay_out),
        CommandSpec::new("layout.incremental", "Place New Shapes", "Design > Layout", incremental)
            .params(concat!(
                r#"{"ids": [string], "algorithm"?: "layered"|"tree"|"radial"|"force", "direction"?: "TB"|"BT"|"LR"|"RL" (also "TD"), "nodeSpacing"?: n, "layerSpacing"?: n} -> {"ids", "notes"}"#,
                r#"; places only the shapes in `ids` (with everything inside them) next to the shapes they connect to; every other shape stays where it is; new groups, containers and lane sets are placed whole, new members of an existing container or lane stay inside it, callouts follow their target, locked shapes stay"#
            ))
            .when(can_lay_out),
        CommandSpec::new("layout.route", "Route Connectors", "Design > Layout", route)
            .params(r#"{"ids"?: [string]} -> {"ids"}; routes the connectors (default: the selected ones, else all) around shapes, orthogonal, and keeps them clear after every change; never moves a shape"#)
            .when(has_connectors),
    ]
}

/// Enabled when the active page has shapes and is not a generated decoration page (its shapes are
/// rebuilt from the decoration settings after every change, so a layout could not stick).
fn can_lay_out(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(decor::is_generated) {
        return Some("this decoration page is generated from the Design settings");
    }
    has_shapes(s)
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
        // The per-connector flag only decides on a page that does not route every connector.
        s.run("page.setRouting", &json!({"avoid": "never"})).unwrap();
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
    /// The sample flowchart plus a page built through commands with a group, a three-lane
    /// swimlane with two shapes per lane, a callout and connectors across all of them: what a
    /// person gets from the Insert tab, which Re-Layout must not take apart.
    fn parts() -> Session {
        let mut s = Session::new(crate::sample::sample_document());
        let run = |s: &mut Session, id: &str, v: serde_json::Value| s.run(id, &v).unwrap();
        run(&mut s, "page.add", json!({"name": "Parts"}));
        let add = |s: &mut Session, id: &str, x: f64, y: f64| {
            s.run("shape.add", &json!({"stencil": "flowchart.process", "x": x, "y": y, "id": id, "text": id})).unwrap();
        };
        add(&mut s, "ga", 60.0, 60.0);
        add(&mut s, "gb", 220.0, 60.0);
        run(&mut s, "arrange.group", json!({"ids": ["ga", "gb"], "id": "g"}));
        add(&mut s, "n1", 500.0, 60.0);
        add(&mut s, "n2", 700.0, 60.0);
        let lanes = run(&mut s, "lane.addSet", json!({"x": 60, "y": 300, "lanes": 3, "title": "Process", "id": "set"}))["lanes"].clone();
        for (i, lane) in lanes.as_array().unwrap().iter().enumerate() {
            for (j, x) in [160.0, 400.0].into_iter().enumerate() {
                let id = format!("l{i}{j}");
                add(&mut s, &id, x + 120.0 * i as f64, 360.0 + 140.0 * i as f64);
                run(&mut s, "container.addMember", json!({"container": lane, "ids": [id]}));
            }
        }
        run(&mut s, "callout.add", json!({"target": "n1", "text": "Note", "id": "co"}));
        for (a, b) in
            [("ga", "n1"), ("n1", "n2"), ("n2", "l00"), ("l00", "l01"), ("l01", "l10"), ("l10", "l11"), ("l11", "l20"), ("l20", "l21"), ("gb", "l10")]
        {
            run(&mut s, "connector.add", json!({"from": {"shape": a}, "to": {"shape": b}}));
        }
        run(&mut s, "edit.select", json!({"ids": []}));
        s
    }

    /// Top-left of `id` relative to `of`, to a thousandth.
    fn rel(page: &Page, id: &str, of: &str) -> (i64, i64) {
        let (a, b) = (page.shape(id).unwrap(), page.shape(of).unwrap());
        (((a.x - b.x) * 1000.0).round() as i64, ((a.y - b.y) * 1000.0).round() as i64)
    }

    #[test]
    fn relayout_keeps_groups_swimlanes_and_callouts_whole_in_every_mode() {
        let fresh = parts();
        let before = fresh.doc.page().unwrap().clone();
        let lanes: Vec<String> = vizcraft_diagram::container::lanes(&before, "set");
        assert_eq!(lanes.len(), 3);
        for algorithm in ["layered", "tree", "radial", "force"] {
            let mut s = parts();
            s.run("layout.full", &json!({"algorithm": algorithm})).unwrap();
            assert_eq!(s.undo_label(), Some("Re-Layout"), "{algorithm}");
            let page = s.doc.page().unwrap();
            assert_eq!(rel(page, "gb", "ga"), rel(&before, "gb", "ga"), "{algorithm}: the group stays whole");
            for (i, lane) in lanes.iter().enumerate() {
                assert_eq!(rel(page, lane, "set"), rel(&before, lane, "set"), "{algorithm}: {lane} stays in its set");
                for j in 0..2 {
                    let m = format!("l{i}{j}");
                    assert_eq!(page.shape(&m).unwrap().parent.as_ref(), Some(lane), "{algorithm}");
                    assert!(page.shape(lane).unwrap().bounds().contains_rect(&page.shape(&m).unwrap().bounds()), "{algorithm}: {m} left its lane");
                }
            }
            let co = page.shape("co").unwrap().bounds();
            for other in ["ga", "gb", "n1", "n2", "set"] {
                let b = page.shape(other).unwrap().bounds();
                assert!(!(co.x0 < b.x1 && b.x0 < co.x1 && co.y0 < b.y1 && b.y0 < co.y1), "{algorithm}: the callout covers {other}");
            }
            let units = ["g", "n1", "n2", "set"].map(|id| page.shape(id).unwrap().bounds());
            for (i, a) in units.iter().enumerate() {
                for b in units.iter().skip(i + 1) {
                    assert!(!(a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1), "{algorithm}: units overlap");
                }
            }
            // The sample page was not touched, and one undo puts the page back.
            assert_eq!(s.doc.pages[0], fresh.doc.pages[0]);
            s.run("edit.undo", &json!({})).unwrap();
            assert_eq!(s.doc.page().unwrap(), &before, "{algorithm}: one undo step");
        }
    }

    #[test]
    fn place_new_shapes_keeps_groups_lane_members_and_callouts_by_the_unit_rules_in_every_mode() {
        for algorithm in ["layered", "tree", "radial", "force"] {
            let mut s = parts();
            let run = |s: &mut Session, id: &str, v: serde_json::Value| s.run(id, &v).unwrap();
            let lane1 = vizcraft_diagram::container::lanes(s.doc.page().unwrap(), "set")[1].clone();
            // New: a group downstream of n2, a step added to the middle lane (still outside it),
            // and a shape with a callout, all far off to the right.
            for (id, x, y) in [("na", 2000.0, 60.0), ("nb", 2160.0, 60.0), ("nm", 2000.0, 400.0), ("nc", 2000.0, 800.0)] {
                run(&mut s, "shape.add", json!({"stencil": "flowchart.process", "x": x, "y": y, "id": id, "text": id}));
            }
            run(&mut s, "arrange.group", json!({"ids": ["na", "nb"], "id": "ng"}));
            run(&mut s, "container.addMember", json!({"container": lane1, "ids": ["nm"]}));
            run(&mut s, "callout.add", json!({"target": "nc", "text": "New note", "id": "nco"}));
            for (a, b) in [("n2", "na"), ("l11", "nm"), ("l01", "nm"), ("nb", "nc")] {
                run(&mut s, "connector.add", json!({"from": {"shape": a}, "to": {"shape": b}}));
            }
            let before = s.doc.page().unwrap().clone();
            let new = ["na", "nb", "ng", "nm", "nc", "nco"];
            s.run("layout.incremental", &json!({"ids": ["ng", "nm", "nc", "nco"], "algorithm": algorithm})).unwrap();
            assert_eq!(s.undo_label(), Some("Place New Shapes"), "{algorithm}");
            let page = s.doc.page().unwrap();
            for old in before.shapes.iter().filter(|sh| !new.contains(&sh.id.as_str())) {
                assert_eq!(page.shape(&old.id), Some(old), "{algorithm}: {} moved", old.id);
            }
            assert_eq!(rel(page, "nb", "na"), rel(&before, "nb", "na"), "{algorithm}: the new group stays whole");
            let nm = page.shape("nm").unwrap();
            assert_eq!(nm.parent.as_ref(), Some(&lane1), "{algorithm}");
            assert!(page.shape(&lane1).unwrap().bounds().contains_rect(&nm.bounds()), "{algorithm}: the new step left its lane");
            let overlap = |a: &str, b: &str| {
                let (x, y) = (page.shape(a).unwrap().bounds(), page.shape(b).unwrap().bounds());
                x.x0 < y.x1 && y.x0 < x.x1 && x.y0 < y.y1 && y.y0 < x.y1
            };
            for other in ["l10", "l11"] {
                assert!(!overlap("nm", other), "{algorithm}: the new step covers {other}");
            }
            let units = ["g", "n1", "n2", "set", "ng", "nc"];
            for (i, a) in units.iter().enumerate() {
                for b in units.iter().skip(i + 1) {
                    assert!(!overlap(a, b), "{algorithm}: {a} and {b} overlap");
                }
            }
            // The new callout was not placed: it followed its target and covers nothing.
            let gap = |p: &Page| {
                let (c, t) = (p.shape("nco").unwrap().bounds().center(), p.shape("nc").unwrap().bounds().center());
                (((c.x - t.x).abs() * 1000.0).round() as i64, ((c.y - t.y).abs() * 1000.0).round() as i64)
            };
            assert_eq!(gap(page), gap(&before), "{algorithm}");
            for other in page.shapes.iter().filter(|sh| !["nco", "g", "ng"].contains(&sh.id.as_str())) {
                assert!(!overlap("nco", &other.id), "{algorithm}: the new callout covers {}", other.id);
            }
            s.run("edit.undo", &json!({})).unwrap();
            assert_eq!(s.doc.page().unwrap(), &before, "{algorithm}: one undo step");
        }
    }

    #[test]
    fn relayout_of_the_sample_reglues_fixed_points_so_connectors_follow_the_new_arrangement() {
        for algorithm in ["layered", "tree", "radial", "force"] {
            let mut s = Session::new(crate::sample::sample_document());
            s.run("layout.full", &json!({"algorithm": algorithm, "direction": "LR"})).unwrap();
            let page = s.doc.page().unwrap();
            for c in &page.connectors {
                assert!(
                    matches!(c.start, End::Glued { point: None, .. }) && matches!(c.end, End::Glued { point: None, .. }),
                    "{algorithm}: {}",
                    c.id
                );
            }
        }
    }

    #[test]
    fn relayout_is_off_on_a_generated_decoration_page() {
        let mut s = Session::new(crate::sample::sample_document());
        s.run("design.setBorderTitle", &json!({"kind": "frame"})).unwrap();
        let generated = s.doc.pages.iter().position(vizcraft_diagram::decor::is_generated).unwrap();
        s.run("page.setActive", &json!({"page": generated})).unwrap();
        assert!(!s.doc.page().unwrap().shapes.is_empty(), "the frame is drawn with shapes");
        assert!(matches!(s.run("layout.full", &json!({})), Err(crate::EngineError::Disabled(why)) if why.contains("decoration")));
        assert!(matches!(s.run("layout.incremental", &json!({"ids": ["x"]})), Err(crate::EngineError::Disabled(_))));
    }
}
