//! Auto-layout commands (`layout.*`): rearrange a page or a selection with the layered, tree,
//! radial or force-directed layout (optionally in the model's creation order), place new shapes
//! around ones that stay put (which are pinned), lay shapes out with a fixed primitive such as a
//! pipeline, bands or a matrix, and route connectors around shapes without moving any shape.

use std::collections::HashMap;

use serde_json::{Value, json};
use vizcraft_diagram::autolayout::{self, Algorithm, Direction, Options, Primitive};
use vizcraft_diagram::{autoroute, decor};

use super::connector::connector_targets;
use super::{Kinds, has_connectors, has_shapes, ids_param, num, opt_bool, opt_str, page_mut, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Largest spacing a caller may ask for, in world units.
const MAX_SPACING: f64 = 10_000.0;

/// The layout commands, in menu order.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("layout.full", "Re-Layout", "Design > Layout", full)
            .params(concat!(
                r#"{"ids"?: [string], "algorithm"?: "layered"|"tree"|"radial"|"force" (default layered), "direction"?: "TB"|"BT"|"LR"|"RL" (also "TD"), "nodeSpacing"?: n, "layerSpacing"?: n, "keepOrder"?: bool} -> {"ids", "notes"}"#,
                r#"; rearranges `ids`, else the selected shapes when two or more are selected, else the whole page; groups, containers and lane sets move whole, callouts follow their target, locked shapes stay; `keepOrder` keeps siblings in the creation order of the model elements they draw (then page order) wherever crossing reduction would tie"#
            ))
            .when(can_lay_out),
        CommandSpec::new("layout.incremental", "Place New Shapes", "Design > Layout", incremental)
            .params(concat!(
                r#"{"ids": [string], "algorithm"?: "layered"|"tree"|"radial"|"force", "direction"?: "TB"|"BT"|"LR"|"RL" (also "TD"), "nodeSpacing"?: n, "layerSpacing"?: n} -> {"ids", "notes"}"#,
                r#"; places only the shapes in `ids` (with everything inside them) next to the shapes they connect to; every other shape stays where it is (the layout pins it); new groups, containers and lane sets are placed whole, new members of an existing container or lane stay inside it, callouts follow their target, locked shapes stay"#
            ))
            .when(can_lay_out),
        CommandSpec::new("layout.primitive", "Lay Out As", "Design > Layout", primitive)
            .params(concat!(
                r#"{"primitive": "pipeline"|"bands"|"chain"|"matrix"|"nested"|"hubSpoke"|"rings"|"timeline"|"voter"|"er", "ids"?: [string], "roles"?: {..}, "direction"?: "TB"|"BT"|"LR"|"RL", "nodeSpacing"?: n, "layerSpacing"?: n} -> {"ids", "notes", "bounds"}"#,
                r#"; roles by primitive: pipeline {"parallel"?: {"from", "to", "members": [id]}} (steps are the other shapes in page order), bands {"bands": [[id]]}, chain {"forward": [id], "compensation"?: [id|null]}, matrix {"rows": [[id|null]]} or {"cols": [[id|null]]}, nested {"containers"?: [{"id", "members": [id], "padding"?, "header"?}]} (default: the containers on the page and their members), hubSpoke {"hub"}, rings {"center", "rings": [[id]]}, timeline {"events"?: [id], "labels"?: [id|null]}, voter {"channels": [[id]], "voter"}, er {"satellites"?: [{"owner", "members": [id]}]} (default: the Chen attribute satellites on the page are put back on a ring around their entity, which carries them)"#,
                r#"; lays out `ids` and every shape the roles name (else the selection when two or more are selected, else the whole page) in one undo step with the unit rules of layout.full, refuses roles that are unknown, repeated or leave a shape without a role, and returns the bounds of what moved; direction defaults to LR, TB for voter and er"#
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
    let keep_order = opt_bool(v, "keepOrder")?.unwrap_or(false);
    let ids: Option<Vec<String>> = if ids_param(v)?.is_some() {
        Some(targets(s, v, Kinds::Shapes)?)
    } else {
        let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        let selected: Vec<String> = s.sel.items.iter().filter(|id| page.shape(id).is_some()).cloned().collect();
        (selected.len() >= 2).then_some(selected)
    };
    let order = keep_order.then(|| creation_order(s)).transpose()?;
    let page = page_mut(s)?;
    let ids = without_keys(page, ids);
    let notes = autolayout::layout_items_ordered(page, ids.as_deref(), &opts, order.as_ref()).map_err(EngineError::Failed)?;
    let ids = ids.unwrap_or_else(|| page.shapes.iter().map(|sh| sh.id.clone()).collect());
    with_sel(s, json!({"ids": ids, "notes": notes}))
}

/// The creation order of the active page's shapes: a shape drawing a model element takes that
/// element's place among the elements, every other shape comes after them in page order.
fn creation_order(s: &Session) -> Result<HashMap<String, u32>, EngineError> {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let elements: HashMap<&str, usize> = s.doc.model.elements.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let after = elements.len();
    let rank = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    Ok(page
        .shapes
        .iter()
        .enumerate()
        .map(|(i, sh)| {
            let at = sh.element.as_deref().and_then(|e| elements.get(e)).copied().unwrap_or_else(|| after.saturating_add(i));
            (sh.id.clone(), rank(at))
        })
        .collect())
}

/// The default direction of a primitive: along a row for chains, bands and timelines, down for
/// the voter (the direction does not matter to the others).
fn default_direction(primitive: &str) -> Direction {
    if primitive == "voter" || primitive == "er" { Direction::TopBottom } else { Direction::LeftRight }
}

/// The default roles of the er primitive: none, after every satellite on the page (a shape
/// marked `vizcraft.satelliteOf`, whose parent is its entity, so it moves with it) is put back on
/// the ring around its entity that the er primitive draws.
fn ring_satellites(page: &mut vizcraft_diagram::Page) {
    let key = vizcraft_notation::satellites::EXT_SATELLITE_OF;
    let mut by_owner: Vec<(String, Vec<String>)> = Vec::new();
    for s in page.shapes.iter().filter(|s| s.parent.is_some()) {
        let Some(owner) = s.ext.get(key).filter(|o| s.parent.as_ref() == Some(*o)) else { continue };
        match by_owner.iter_mut().find(|(o, _)| o == owner) {
            Some((_, list)) => list.push(s.id.clone()),
            None => by_owner.push((owner.clone(), vec![s.id.clone()])),
        }
    }
    for (owner, members) in by_owner {
        let Some(o) = page.shape(&owner).map(|o| (o.x, o.y, o.w, o.h)) else { continue };
        let sizes: Vec<(f64, f64)> = members.iter().filter_map(|m| page.shape(m)).map(|m| (m.w, m.h)).collect();
        let (offsets, _) = vizcraft_layout::er_ring((o.2, o.3), &sizes);
        for (m, p) in members.iter().zip(offsets) {
            if let Some(s) = page.shape_mut(m) {
                s.x = o.0 + p.x;
                s.y = o.1 + p.y;
                s.sanitize();
            }
        }
    }
}

/// The shapes to lay out with a generated key (`vizcraft_diagram::key`) left where it is: on a
/// page with a key, `None` (the whole page) becomes every other shape. A page without one keeps
/// `ids` as given.
fn without_keys(page: &vizcraft_diagram::Page, ids: Option<Vec<String>>) -> Option<Vec<String>> {
    if !page.shapes.iter().any(vizcraft_diagram::key::is_key) {
        return ids;
    }
    let keep = |id: &String| page.shape(id).is_none_or(|sh| !vizcraft_diagram::key::is_key(sh));
    match ids {
        Some(list) => Some(list.into_iter().filter(keep).collect()),
        None => Some(page.shapes.iter().filter(|sh| !vizcraft_diagram::key::is_key(sh)).map(|sh| sh.id.clone()).collect()),
    }
}

fn primitive(s: &mut Session, v: &Value) -> CmdResult {
    let name = opt_str(v, "primitive")?.ok_or_else(|| EngineError::Params("`primitive` is required".into()))?.to_string();
    let roles = match v.get("roles") {
        None | Some(Value::Null) => json!({}),
        Some(r @ Value::Object(_)) => r.clone(),
        Some(_) => return Err(EngineError::Params("`roles` must be an object".into())),
    };
    let parsed: Primitive<String> = serde_json::from_value(json!({"primitive": name, "roles": roles}))
        .map_err(|e| EngineError::Params(format!("`primitive` or `roles` is not valid: {e}")))?;
    let mut opts = options(v)?;
    if opt_str(v, "direction")?.is_none() {
        opts.direction = default_direction(parsed.name());
    }
    let ids: Option<Vec<String>> = if ids_param(v)?.is_some() {
        Some(targets(s, v, Kinds::Shapes)?)
    } else if !parsed.references().is_empty() {
        // The roles name the shapes.
        Some(Vec::new())
    } else {
        let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        let selected: Vec<String> = s.sel.items.iter().filter(|id| page.shape(id).is_some()).cloned().collect();
        (selected.len() >= 2).then_some(selected)
    };
    let page = page_mut(s)?;
    if matches!(&parsed, Primitive::Er { satellites } if satellites.is_empty()) {
        ring_satellites(page);
    }
    let ids = without_keys(page, ids);
    let out = autolayout::layout_primitive(page, ids.as_deref(), parsed, &opts).map_err(EngineError::Failed)?;
    let bounds = out.bounds.map(|b| json!({"x": b.x0, "y": b.y0, "width": b.width(), "height": b.height()}));
    with_sel(s, json!({"ids": out.ids, "notes": out.notes, "bounds": bounds}))
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

    fn overlap_free(page: &Page) {
        for (i, a) in page.shapes.iter().enumerate() {
            for b in page.shapes.iter().skip(i + 1) {
                let (x, y) = (a.bounds(), b.bounds());
                assert!(!(x.x0 < y.x1 && y.x0 < x.x1 && x.y0 < y.y1 && y.y0 < x.y1), "{} and {} overlap", a.id, b.id);
            }
        }
    }

    #[test]
    fn layout_primitive_on_the_sample_is_one_undo_step_and_reroutes() {
        let mut s = Session::new(crate::sample::sample_document());
        let before = s.doc.page().unwrap().clone();
        let out = s.run("layout.primitive", &json!({"primitive": "pipeline"})).unwrap();
        let page = s.doc.page().unwrap();
        assert_eq!(out["ids"].as_array().unwrap().len(), page.shapes.len());
        assert!(out["bounds"]["width"].as_f64().unwrap() > 0.0);
        // Steps in page order along one row.
        for pair in page.shapes.windows(2) {
            assert!(pair[0].bounds().x1 < pair[1].bounds().x0, "{} before {}", pair[0].id, pair[1].id);
            assert!((pair[0].bounds().center().y - pair[1].bounds().center().y).abs() < 1e-6);
        }
        overlap_free(page);
        // Connectors between shapes that moved apart lost their fixed points and stored bends.
        let rerouted = page.connectors.iter().filter(|c| matches!(&c.start, End::Glued { point: None, .. }) && c.waypoints.is_empty()).count();
        assert!(rerouted > 0, "connectors follow the new arrangement");
        assert_eq!(s.undo_label(), Some("Lay Out As"));
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap(), &before, "one undo puts the page back");
        assert!(!s.can_undo());
    }

    #[test]
    fn layout_primitive_follows_the_roles_and_refuses_bad_ones() {
        let mut s = Session::new(crate::sample::sample_document());
        let ids: Vec<String> = s.doc.page().unwrap().shapes.iter().map(|sh| sh.id.clone()).collect();
        let hub = ids[2].clone();
        s.run("layout.primitive", &json!({"primitive": "hubSpoke", "roles": {"hub": hub}, "ids": ids})).unwrap();
        let page = s.doc.page().unwrap();
        overlap_free(page);
        let c = page.shape(&hub).unwrap().bounds().center();
        let d: Vec<f64> =
            page.shapes.iter().filter(|sh| sh.id != hub).map(|sh| (sh.bounds().center().x - c.x).hypot(sh.bounds().center().y - c.y)).collect();
        assert!(d.iter().all(|r| (r - d[0]).abs() < 1e-6), "spokes on one circle");
        let bands = json!({"primitive": "bands", "roles": {"bands": [[ids[0], ids[1], ids[2]], [ids[3], ids[4], ids[5]]]}});
        s.run("layout.primitive", &bands).unwrap();
        let page = s.doc.page().unwrap();
        let bottom = ids[..3].iter().map(|id| page.shape(id).unwrap().bounds().y1).fold(f64::MIN, f64::max);
        assert!(ids[3..].iter().all(|id| page.shape(id).unwrap().bounds().y0 > bottom), "two bands, the second below the first");
        let snapshot = s.doc.page().unwrap().clone();
        for bad in [
            json!({"primitive": "spiral"}),
            json!({}),
            json!({"primitive": "hubSpoke", "roles": {"hub": "nope"}}),
            json!({"primitive": "hubSpoke", "roles": {"hub": 5}}),
            json!({"primitive": "rings", "roles": {"center": ids[0], "rings": [[ids[1]]]}, "ids": ids}),
            json!({"primitive": "bands", "roles": {"bands": [[ids[0], ids[0]]]}}),
            json!({"primitive": "matrix", "roles": {"rows": [[ids[0]]], "cols": [[ids[1]]]}}),
            json!({"primitive": "pipeline", "roles": "x"}),
            json!({"primitive": "chain", "roles": {"forward": [], "compensation": [null, 3, {}]}}),
            json!({"primitive": "voter", "roles": {"channels": [[]], "voter": ids[0]}, "nodeSpacing": -1}),
            json!({"primitive": "nested", "roles": {"containers": [{"id": ids[0], "members": [ids[0]]}]}}),
            json!({"primitive": "nested", "roles": {"containers": [{"id": ids[0], "members": [ids[1]], "padding": 1e300}]}}),
            json!({"primitive": "timeline", "roles": {"events": [ids[0]], "labels": [ids[1], ids[2]]}}),
        ] {
            assert!(s.run("layout.primitive", &bad).is_err(), "{bad}");
            assert_eq!(s.doc.page().unwrap(), &snapshot, "a refused layout moves nothing: {bad}");
        }
        let err =
            s.run("layout.primitive", &json!({"primitive": "rings", "roles": {"center": ids[0], "rings": [[ids[1]]]}, "ids": ids})).unwrap_err();
        assert!(err.to_string().contains("shape "), "errors name shapes, not node indices: {err}");
    }

    #[test]
    fn layout_primitive_nests_containers_around_their_members() {
        let mut s = Session::new(Document::new());
        let boxed = s.run("container.add", &json!({"x": 0, "y": 0, "w": 400, "h": 300})).unwrap()["id"].as_str().unwrap().to_string();
        for (id, x) in [("m1", 600.0), ("m2", 800.0)] {
            s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": x, "y": 500, "id": id})).unwrap();
        }
        // Membership comes from the roles, so the members land inside the box with its padding.
        let out =
            s.run("layout.primitive", &json!({"primitive": "nested", "roles": {"containers": [{"id": boxed, "members": ["m1", "m2"]}]}})).unwrap();
        assert_eq!(out["ids"].as_array().unwrap().len(), 3);
        let page = s.doc.page().unwrap();
        let frame = page.shape(&boxed).unwrap().frame();
        for m in ["m1", "m2"] {
            let b = page.shape(m).unwrap().bounds();
            assert!(b.x0 > frame.x0 && b.y0 > frame.y0 && b.x1 < frame.x1 && b.y1 < frame.y1, "{m} inside the container");
        }
    }

    #[test]
    fn keep_order_draws_siblings_in_creation_order() {
        let mut page = Page::new("p1", "Page-1");
        for id in ["root", "c", "a", "b"] {
            page.shapes.push(Shape::new(id, ShapeKind::Rectangle, 0.0, 0.0, 60.0, 40.0));
        }
        for (i, t) in ["a", "b", "c"].iter().enumerate() {
            page.connectors.push(Connector::new(format!("e{i}"), End::dynamic("root"), End::dynamic(*t)));
        }
        let mut doc = Document::new();
        doc.pages = vec![page];
        let mut s = Session::new(doc);
        let x = |s: &Session, id: &str| s.doc.page().unwrap().shape(id).unwrap().x;
        s.run("layout.full", &json!({})).unwrap();
        assert!(x(&s, "a") < x(&s, "b") && x(&s, "b") < x(&s, "c"), "edge order without keepOrder");
        s.run("layout.full", &json!({"keepOrder": true})).unwrap();
        assert!(x(&s, "c") < x(&s, "a") && x(&s, "a") < x(&s, "b"), "page (creation) order with keepOrder");
        assert!(s.run("layout.full", &json!({"keepOrder": "yes"})).is_err());
    }

    #[test]
    fn junk_roles_never_panic() {
        let junk = [
            json!({"primitive": "bands", "roles": {"bands": [[1e308, null, {}], "x", [[[]]]]}}),
            json!({"primitive": "matrix", "roles": {"rows": [[null, null], []], "cols": null}}),
            json!({"primitive": "matrix", "roles": {"rows": [[null]]}}),
            json!({"primitive": "rings", "roles": {"center": null, "rings": "x"}}),
            json!({"primitive": "pipeline", "roles": {"parallel": {"from": "s1", "to": "s1", "members": []}}}),
            json!({"primitive": "pipeline", "roles": {"parallel": {"from": "s1", "to": "s2", "members": ["s3", "s3"]}}}),
            json!({"primitive": "nested", "roles": {"containers": [{"id": "s1", "members": ["s2"]}, {"id": "s2", "members": ["s1"]}]}}),
            json!({"primitive": "nested", "roles": {"containers": [{"id": "s1", "members": ["s2"], "header": -5}]}}),
            json!({"primitive": "timeline", "roles": {"labels": [null, null, null, null, null, null, null, null, null]}}),
            json!({"primitive": "voter", "roles": {"channels": [["s1"], ["s1"]], "voter": "s1"}}),
            json!({"primitive": "hubSpoke", "roles": {"hub": "s1"}, "direction": "up", "layerSpacing": 1e308}),
            json!({"primitive": "chain", "roles": {"forward": ["s1", "s2"], "compensation": ["s2"]}}),
        ];
        for j in &junk {
            for select_all in [false, true] {
                let mut s = Session::new(crate::sample::sample_document());
                if select_all {
                    s.run("edit.selectAll", &json!({})).unwrap();
                }
                let _ = s.run("layout.primitive", j);
                for sh in &s.doc.page().unwrap().shapes {
                    assert!(sh.x.is_finite() && sh.y.is_finite() && sh.w.is_finite() && sh.h.is_finite(), "{j}");
                }
            }
        }
    }
}
