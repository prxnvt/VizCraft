//! Tests for the container, lane and callout commands.

use serde_json::{Value, json};
use vizcraft_diagram::container::{self as ct, HeadingPosition, Role};

use crate::Session;

/// A blank document with shapes `a` (0,0 40x20), `b` (200,100 40x40), `c` (500,0 20x60).
fn session() -> Session {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    for (id, x, y, w, h) in [("a", 0, 0, 40, 20), ("b", 200, 100, 40, 40), ("c", 500, 0, 20, 60)] {
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "id": id, "x": x, "y": y, "w": w, "h": h})).unwrap();
    }
    s.run("edit.selectNone", &json!({})).unwrap();
    s
}

fn frame(s: &Session, id: &str) -> (f64, f64, f64, f64) {
    let sh = s.doc.page().unwrap().shape(id).unwrap_or_else(|| panic!("no {id}"));
    (sh.x, sh.y, sh.w, sh.h)
}

fn parent(s: &Session, id: &str) -> Option<String> {
    s.doc.page().unwrap().shape(id).unwrap().parent.clone()
}

fn settings(s: &Session, id: &str) -> ct::Container {
    s.doc.page().unwrap().shape(id).unwrap().container.clone().unwrap()
}

/// Run `id` with `params`, check it made one undo step, undo it and check the document is back,
/// then redo it.
fn one_step(s: &mut Session, id: &str, params: Value) -> Value {
    let before = s.doc.clone();
    let depth = s.transactions().len();
    let r = s.run(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"));
    assert_ne!(s.doc, before, "{id} changed nothing");
    assert_eq!(s.transactions().len(), depth + 1, "{id} is one undo step");
    s.run("edit.undo", &json!({})).unwrap();
    assert!(s.doc == before, "{id} undone");
    s.run("edit.redo", &json!({})).unwrap();
    r
}

#[test]
fn add_wraps_shapes_or_places_an_empty_container() {
    let mut s = session();
    let r = one_step(&mut s, "container.add", json!({"ids": ["a", "b"], "heading": "Team", "style": "sage", "id": "k"}));
    assert_eq!(r["members"], json!(["a", "b"]));
    assert_eq!(s.sel.items, vec!["k".to_string()]);
    assert_eq!(parent(&s, "a").as_deref(), Some("k"));
    // Margin 20 around 0,0..240,140, heading 28 above.
    assert_eq!(frame(&s, "k"), (-20.0, -48.0, 280.0, 208.0));
    let sh = s.doc.shape("k").unwrap();
    assert_eq!((sh.text.as_str(), settings(&s, "k").style.as_str()), ("Team", "sage"));
    // Empty, at a place.
    let r = one_step(&mut s, "container.add", json!({"x": 600, "y": 300, "w": 200, "h": 100}));
    let id = r["id"].as_str().unwrap().to_string();
    assert_eq!(frame(&s, &id), (600.0, 300.0, 200.0, 100.0));
    // With the selection and no place: around the selection.
    s.run("edit.select", &json!({"ids": ["c"]})).unwrap();
    let r = one_step(&mut s, "container.add", json!({}));
    assert_eq!(r["members"], json!(["c"]));
    assert!(s.run("container.add", &json!({"style": "neon"})).is_err());
    assert!(s.run("container.add", &json!({"ids": ["zzz"]})).is_err());
}

#[test]
fn add_and_remove_members_and_locking() {
    let mut s = session();
    s.run("container.add", &json!({"x": 0, "y": 0, "w": 600, "h": 300, "id": "k"})).unwrap();
    let r = one_step(&mut s, "container.addMember", json!({"container": "k", "ids": ["a", "b"]}));
    assert_eq!(r["added"], json!(["a", "b"]));
    let z: Vec<String> = s.doc.page().unwrap().shapes.iter().map(|x| x.id.clone()).collect();
    assert_eq!(z, ["c", "k", "a", "b"], "members draw above the container");
    // Moving the container moves its members.
    one_step(&mut s, "shape.move", json!({"ids": ["k"], "dx": 10, "dy": 5}));
    assert_eq!(frame(&s, "a"), (10.0, 5.0, 40.0, 20.0));
    let r = one_step(&mut s, "container.removeMember", json!({"ids": ["a"]}));
    assert_eq!((r["container"].clone(), r["removed"].clone()), (json!("k"), json!(["a"])));
    assert_eq!(parent(&s, "a"), None);
    one_step(&mut s, "container.set", json!({"container": "k", "locked": true}));
    assert!(s.run("container.addMember", &json!({"container": "k", "ids": ["a"]})).is_err());
    assert!(s.run("container.removeMember", &json!({"container": "k", "ids": ["b"]})).is_err());
    assert!(s.run("container.disband", &json!({"container": "k"})).is_err());
    assert!(s.run("container.addMember", &json!({"container": "a", "ids": ["c"]})).is_err());
    assert!(s.run("container.addMember", &json!({"container": "k"})).is_err());
}

#[test]
fn fit_set_and_auto_fit() {
    let mut s = session();
    s.run("container.add", &json!({"x": -100, "y": -100, "w": 900, "h": 500, "id": "k"})).unwrap();
    s.run("container.addMember", &json!({"container": "k", "ids": ["a", "b"]})).unwrap();
    let r = one_step(&mut s, "container.fit", json!({"container": "k"}));
    assert_eq!((r["x"].clone(), r["y"].clone(), r["w"].clone(), r["h"].clone()), (json!(-20.0), json!(-48.0), json!(280.0), json!(208.0)));
    one_step(
        &mut s,
        "container.set",
        json!({"container": "k", "margins": 10, "headingPosition": "left", "heading": "Side", "headingStyle": "line", "style": "slate"}),
    );
    let c = settings(&s, "k");
    assert_eq!((c.margin, c.heading_position, c.heading_style), (10.0, HeadingPosition::Left, ct::HeadingStyle::Line));
    assert_eq!(s.doc.shape("k").unwrap().text, "Side");
    one_step(&mut s, "container.set", json!({"container": "k", "autoFit": true}));
    // Auto-fit: margin 10, heading 28 on the left, around 0,0..240,140.
    assert_eq!(frame(&s, "k"), (-38.0, -10.0, 288.0, 160.0));
    s.run("shape.move", &json!({"ids": ["b"], "dx": 100, "dy": 0})).unwrap();
    assert_eq!(frame(&s, "k").2, 388.0, "auto-fit follows a member");
    assert!(s.run("container.set", &json!({"container": "k"})).is_err());
    assert!(s.run("container.set", &json!({"container": "k", "headingPosition": "middle"})).is_err());
    s.run("container.add", &json!({"x": 900, "y": 0, "id": "e"})).unwrap();
    assert!(s.run("container.fit", &json!({"container": "e"})).is_err(), "an empty container cannot fit");
}

#[test]
fn disband_and_select_contents() {
    let mut s = session();
    s.run("container.add", &json!({"ids": ["a", "b"], "id": "k"})).unwrap();
    // Selection only: no undo step.
    let r = s.run("container.selectContents", &json!({})).unwrap();
    assert_eq!(r["items"], json!(["a", "b"]));
    s.run("edit.select", &json!({"ids": ["k"]})).unwrap();
    let r = one_step(&mut s, "container.disband", json!({}));
    assert_eq!(r["members"], json!(["a", "b"]));
    assert!(s.doc.shape("k").is_none());
    assert_eq!(parent(&s, "a"), None);
    assert_eq!(s.sel.items, vec!["a".to_string(), "b".to_string()]);
    assert!(s.run("container.selectContents", &json!({"container": "a"})).is_err());
}

#[test]
fn drop_moves_shapes_in_and_out() {
    let mut s = session();
    s.run("container.add", &json!({"x": 0, "y": 200, "w": 400, "h": 300, "id": "k"})).unwrap();
    // `b` moved fully inside: one batch, one undo step, the way the canvas commits a drag.
    let batch = json!({"label": "Move", "commands": [
        {"command": "container.drop", "params": {"ids": ["b"], "dx": 0, "dy": 200}},
        {"command": "shape.move", "params": {"ids": ["b"], "dx": 0, "dy": 200}},
    ]});
    one_step(&mut s, "edit.batch", batch);
    assert_eq!(parent(&s, "b").as_deref(), Some("k"));
    // Partly out: no change, so no undo step.
    let depth = s.transactions().len();
    let r = s.run("container.drop", &json!({"ids": ["b"], "dx": 200, "dy": 0})).unwrap();
    assert_eq!(r["changes"], json!([]));
    assert_eq!(s.transactions().len(), depth);
    // Fully out: leaves.
    let r = one_step(&mut s, "container.drop", json!({"ids": ["b"], "dx": 1000, "dy": 0}));
    assert_eq!(r["changes"], json!([{"id": "b", "container": null}]));
    assert_eq!(parent(&s, "b"), None);
}

#[test]
fn lanes_add_remove_move_phase_and_flip() {
    let mut s = session();
    let r = one_step(&mut s, "lane.addSet", json!({"x": 0, "y": 300, "lanes": 3, "orientation": "horizontal", "id": "set"}));
    let lanes: Vec<String> = r["lanes"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect();
    assert_eq!(lanes.len(), 3);
    let (l1, l2, l3) = (lanes[0].clone(), lanes[1].clone(), lanes[2].clone());
    assert_eq!(frame(&s, &l2).1, 300.0 + ct::SET_TITLE + ct::LANE_THICKNESS);
    // Put `a` in lane 2 by dropping it there.
    let y = frame(&s, &l2).1 + 30.0;
    s.run("shape.move", &json!({"id": "a", "x": 100, "y": y})).unwrap();
    s.run("container.drop", &json!({"ids": ["a"]})).unwrap();
    assert_eq!(parent(&s, "a").as_deref(), Some(l2.as_str()));
    let inside = |s: &Session, m: &str, l: &str| s.doc.shape(l).unwrap().frame().contains_rect(&s.doc.shape(m).unwrap().bounds());
    // Reorder: lane 2 first, `a` with it.
    let r = one_step(&mut s, "lane.move", json!({"lane": l2, "index": 0}));
    assert_eq!(r["index"], json!(0));
    assert_eq!(frame(&s, &l2).1, 300.0 + ct::SET_TITLE);
    assert!(inside(&s, "a", &l2));
    // Resize lane 2 taller with the ordinary resize command: the others move down.
    let f = frame(&s, &l2);
    one_step(&mut s, "shape.resize", json!({"id": l2, "w": f.2, "h": f.3 + 50.0}));
    assert_eq!(frame(&s, &l1).1, 300.0 + ct::SET_TITLE + ct::LANE_THICKNESS + 50.0);
    assert!(inside(&s, "a", &l2));
    // Add a lane at the front, then remove lane 3.
    let r = one_step(&mut s, "lane.add", json!({"set": "set", "index": 0, "title": "Customer"}));
    let l4 = r["id"].as_str().unwrap().to_string();
    assert_eq!(s.doc.shape(&l4).unwrap().text, "Customer");
    assert!(inside(&s, "a", &l2));
    let r = one_step(&mut s, "lane.remove", json!({"lane": l3}));
    assert_eq!(r["deleted"], json!([l3]));
    assert_eq!(ct::lanes(s.doc.page().unwrap(), "set"), vec![l4.clone(), l2.clone(), l1.clone()]);
    // Phases and flipping.
    let r = one_step(&mut s, "lane.addPhase", json!({"set": "set", "at": 380, "title": "Build"}));
    assert_eq!(r["phases"], json!(1));
    assert!(inside(&s, "a", &l2));
    let r = one_step(&mut s, "lane.flip", json!({"set": "set"}));
    assert_eq!(r["orientation"], json!("vertical"));
    assert!(settings(&s, &l2).vertical);
    assert!(inside(&s, "a", &l2));
    // The set's lanes follow selection: a selected lane names its set.
    s.run("edit.select", &json!({"ids": [l1]})).unwrap();
    one_step(&mut s, "lane.flip", json!({}));
    assert!(!settings(&s, "set").vertical);
    assert!(s.run("lane.addSet", &json!({"x": 0, "y": 0, "lanes": 0})).is_err());
    assert!(s.run("lane.addSet", &json!({"x": 0, "y": 0, "orientation": "diagonal"})).is_err());
    assert!(s.run("lane.move", &json!({"lane": "a", "index": 0})).is_err());
    assert!(s.run("lane.add", &json!({"set": "a"})).is_err());
    // A locked set refuses lane changes.
    s.run("container.set", &json!({"container": "set", "locked": true})).unwrap();
    assert!(s.run("lane.add", &json!({"set": "set"})).is_err());
    assert!(s.run("lane.remove", &json!({"lane": l4})).is_err());
}

#[test]
fn callouts_add_and_set() {
    let mut s = session();
    s.run("edit.select", &json!({"ids": ["b"]})).unwrap();
    let r = one_step(&mut s, "callout.add", json!({"text": "Check this", "id": "co"}));
    assert_eq!(r["id"], json!("co"));
    let c = settings(&s, "co");
    assert_eq!((c.role, c.target.as_deref()), (Role::Callout, Some("b")));
    assert_eq!(frame(&s, "co").0, 300.0, "right of the target by default");
    let page = s.doc.page().unwrap();
    let (_, to) = ct::leader(page, page.shape("co").unwrap()).unwrap();
    // The leader follows the target.
    s.run("shape.move", &json!({"ids": ["b"], "dx": 0, "dy": 300})).unwrap();
    let page = s.doc.page().unwrap();
    let (_, moved) = ct::leader(page, page.shape("co").unwrap()).unwrap();
    assert_ne!(to, moved);
    one_step(&mut s, "callout.set", json!({"callout": "co", "target": "c", "leader": false, "style": "bubble", "side": "left"}));
    let c = settings(&s, "co");
    assert_eq!((c.target.as_deref(), c.leader, c.side), (Some("c"), false, ct::Side::Left));
    assert_eq!(s.doc.shape("co").unwrap().kind, vizcraft_diagram::ShapeKind::Ellipse);
    assert!(s.run("callout.set", &json!({"callout": "co", "target": "co"})).is_err());
    assert!(s.run("callout.set", &json!({"callout": "b", "leader": true})).is_err());
    assert!(s.run("callout.add", &json!({"target": "zzz"})).is_err());
    assert!(s.run("callout.add", &json!({"target": "a", "style": "neon"})).is_err());
}

#[test]
fn containers_lanes_and_callouts_survive_save_and_load() {
    let mut s = session();
    s.run("container.add", &json!({"ids": ["a"], "id": "k", "style": "amber"})).unwrap();
    s.run("container.set", &json!({"container": "k", "autoFit": true, "headingPosition": "bottom"})).unwrap();
    s.run("lane.addSet", &json!({"x": 0, "y": 400, "lanes": 2, "id": "set"})).unwrap();
    s.run("lane.addPhase", &json!({"set": "set", "at": 300, "title": "Two"})).unwrap();
    s.run("callout.add", &json!({"target": "b", "id": "co", "style": "note"})).unwrap();
    let bytes = crate::io::save_bytes("t.vizcraft", &s.doc).unwrap();
    let back = crate::io::load_bytes("t.vizcraft", &bytes).unwrap();
    assert_eq!(back.page().unwrap().shapes, s.doc.page().unwrap().shapes);
}
