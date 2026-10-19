//! Tests of the net commands, together with the net pass they rely on.

use serde_json::{Value, json};
use vizcraft_diagram::{Point, resolve_ends};

use crate::Session;
use crate::cmd::model::fixture::{blank, refused, run};

fn ids(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn net_of(s: &Session, c: &str) -> String {
    s.doc.connector(c).and_then(|c| c.net.clone()).unwrap_or_default()
}

/// Run `id` and check that one undo takes the document back to where it was.
fn one_step(s: &mut Session, id: &str, v: Value) -> Value {
    let before = s.doc.clone();
    let r = run(s, id, v.clone());
    let after = s.doc.clone();
    run(s, "edit.undo", json!({}));
    assert_eq!(s.doc, before, "{id} {v} is one undo step");
    run(s, "edit.redo", json!({}));
    assert_eq!(s.doc, after, "{id} {v} redoes");
    r
}

/// A horizontal wire from (0, 100) to (300, 100) with a branch drawn to its middle.
fn tee() -> (Session, String, String, String) {
    let mut s = blank();
    let w = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 100}, "to": {"x": 300, "y": 100}}));
    let main = w["id"].as_str().unwrap().to_string();
    let b = one_step(&mut s, "wire.draw", json!({"from": {"connector": main, "x": 150, "y": 103}, "to": {"x": 150, "y": 250}}));
    let branch = b["id"].as_str().unwrap().to_string();
    let j = ids(&b["junctions"]).pop().expect("one junction");
    (s, main, branch, j)
}

#[test]
fn a_wire_drawn_to_the_middle_of_a_wire_splits_it_at_one_junction_on_one_net() {
    let (mut s, main, branch, j) = tee();
    let page = &s.doc.pages[0];
    assert_eq!(page.connectors.len(), 3, "the wire split in two plus the branch");
    assert_eq!(page.shapes.len(), 1, "one junction");
    assert_eq!(page.shape(&j).unwrap().center(), Point::new(150.0, 100.0), "projected onto the wire");
    let nets: Vec<String> = page.connectors.iter().map(|c| c.net.clone().unwrap_or_default()).collect();
    assert!(nets.iter().all(|n| *n == nets[0]), "one net: {nets:?}");
    assert_eq!(s.doc.model.nets.len(), 1);
    // Moving the junction moves all three ends on it.
    run(&mut s, "shape.move", json!({"ids": [j], "dx": 0, "dy": 40}));
    let page = &s.doc.pages[0];
    let at = Point::new(150.0, 140.0);
    let on_j = page.connectors.iter().flat_map(|c| {
        let e = resolve_ends(page, c);
        [(c.start.shape() == Some(j.as_str())).then_some(e.start.pos), (c.end.shape() == Some(j.as_str())).then_some(e.end.pos)]
    });
    let ends: Vec<Point> = on_j.flatten().collect();
    assert_eq!(ends.len(), 3);
    assert!(ends.iter().all(|p| p.distance(at) < 1e-9), "{ends:?}");
    // The index and the read commands agree.
    let net = net_of(&s, &main);
    let m = run(&mut s, "net.members", json!({"net": net}));
    assert_eq!(ids(&m["segments"]).len(), 3);
    assert_eq!(ids(&m["junctions"]), std::slice::from_ref(&j));
    assert_eq!(run(&mut s, "net.of", json!({"junction": j}))["net"]["id"], json!(net));
    assert_eq!(run(&mut s, "net.of", json!({"connector": branch}))["net"]["id"], json!(net));
    assert_eq!(s.nets.net_of_connector(&branch), Some(net.as_str()), "Session.nets is rebuilt after the change");
    // Deleting the branch heals the dot away and leaves one connector.
    run(&mut s, "shape.move", json!({"ids": [j], "dx": 0, "dy": -40}));
    run(&mut s, "edit.select", json!({"ids": [branch]}));
    run(&mut s, "edit.delete", json!({}));
    let page = &s.doc.pages[0];
    assert!(page.shapes.is_empty(), "the junction is gone");
    assert_eq!(page.connectors.len(), 1, "the halves are one wire again");
    assert_eq!(page.connectors[0].id, main);
    assert!(s.sel.items.is_empty());
}

#[test]
fn crossing_wires_stay_two_nets() {
    let mut s = blank();
    let a = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 100}, "to": {"x": 300, "y": 100}}));
    let b = run(&mut s, "wire.draw", json!({"from": {"x": 150, "y": 0}, "to": {"x": 150, "y": 300}}));
    assert_ne!(a["net"], b["net"]);
    assert_eq!(run(&mut s, "net.list", json!({}))["nets"].as_array().unwrap().len(), 2);
    assert_eq!(run(&mut s, "net.list", json!({"kind": "pipe"}))["nets"], json!([]));
}

#[test]
fn two_named_nets_joined_by_a_wire_are_not_merged_and_raise_a_conflict() {
    let mut s = blank();
    let a = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 100}, "to": {"x": 300, "y": 100}}));
    let b = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 300}, "to": {"x": 300, "y": 300}}));
    let (na, nb) = (a["net"].as_str().unwrap().to_string(), b["net"].as_str().unwrap().to_string());
    one_step(&mut s, "net.set", json!({"net": na, "name": "L1"}));
    run(&mut s, "net.set", json!({"net": nb, "name": "L2", "properties": {"colour": "BK"}}));
    let e = refused(&mut s, "net.set", json!({"net": nb, "name": "L1"}));
    assert!(e.contains("already called"), "{e}");
    let (ca, cb) = (a["id"].as_str().unwrap(), b["id"].as_str().unwrap());
    run(&mut s, "wire.draw", json!({"from": {"connector": ca, "x": 100, "y": 100}, "to": {"connector": cb, "x": 100, "y": 300}}));
    assert_eq!(s.doc.model.nets.len(), 2, "named nets are never merged by the pass");
    let issue = s.issues.iter().find(|i| i.rule == "net.conflict").cloned().expect("a live conflict issue");
    assert_eq!(issue.subjects, [na.clone(), nb.clone()]);
    assert_eq!(issue.fixes[0].command, "net.merge");
    let e = refused(&mut s, "net.merge", json!({"nets": [na, nb]}));
    assert!(e.contains("force"), "{e}");
    let fix = issue.fixes[0].params.clone();
    let r = one_step(&mut s, "net.merge", fix);
    assert_eq!(r["net"], json!(na));
    assert_eq!(s.doc.model.nets.len(), 1);
    assert!(s.doc.model.net(&na).unwrap().properties.contains_key("colour"), "properties are unioned");
    assert!(s.issues.iter().all(|i| i.rule != "net.conflict"));
}

#[test]
fn deleting_a_middle_segment_of_an_unnamed_net_splits_it() {
    let mut s = blank();
    let w = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 100}, "to": {"x": 400, "y": 100}}));
    let main = w["id"].as_str().unwrap().to_string();
    run(&mut s, "wire.draw", json!({"from": {"connector": main, "x": 100, "y": 100}, "to": {"x": 100, "y": 300}}));
    // The far half of the main wire is the newest connector holding the right end.
    let far = s.doc.pages[0].connectors.iter().find(|c| resolve_ends(&s.doc.pages[0], c).end.pos == Point::new(400.0, 100.0)).unwrap().id.clone();
    run(&mut s, "wire.draw", json!({"from": {"connector": far, "x": 300, "y": 100}, "to": {"x": 300, "y": 300}}));
    assert_eq!(s.doc.model.nets.len(), 1);
    assert_eq!(s.doc.pages[0].connectors.len(), 5);
    // The run between the two junctions is the middle segment.
    let page = &s.doc.pages[0];
    let middle = page
        .connectors
        .iter()
        .find(|c| c.start.shape().is_some_and(|x| x.starts_with('j')) && c.end.shape().is_some_and(|x| x.starts_with('j')))
        .unwrap()
        .id
        .clone();
    run(&mut s, "edit.select", json!({"ids": [middle]}));
    run(&mut s, "edit.delete", json!({}));
    assert_eq!(s.doc.model.nets.len(), 2, "two unconnected halves, two nets");
    assert!(s.doc.pages[0].shapes.is_empty(), "both junctions healed away");
    assert_eq!(s.doc.pages[0].connectors.len(), 2);
}

#[test]
fn split_assign_renumber_and_list_by_page() {
    let mut s = blank();
    let a = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 20}, "to": {"x": 300, "y": 20}}));
    let b = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 250}, "to": {"x": 300, "y": 250}}));
    run(&mut s, "page.add", json!({}));
    let c = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 230}, "to": {"x": 300, "y": 230}}));
    // Page scheme: page number times 1000 plus the 100-unit band from the top.
    let r = one_step(&mut s, "net.renumber", json!({"scheme": "page"}));
    let numbers: Vec<&str> = r["numbered"].as_array().unwrap().iter().map(|x| x["number"].as_str().unwrap()).collect();
    assert_eq!(numbers, ["1001", "1003", "2003"]);
    assert_eq!(s.doc.model.net(c["net"].as_str().unwrap()).unwrap().number, "2003");
    let r = run(&mut s, "net.renumber", json!({"nets": [a["net"], b["net"]], "start": 10, "step": 5, "prefix": "W"}));
    assert_eq!(r["numbered"][1]["number"], "W15");
    run(&mut s, "net.set", json!({"net": a["net"], "name": "keep"}));
    let r = run(&mut s, "net.renumber", json!({"nets": [a["net"]]}));
    assert_eq!(r["numbered"], json!([]), "named nets are skipped unless forced");
    let page0 = s.doc.pages[0].id.clone();
    assert_eq!(run(&mut s, "net.list", json!({"page": page0}))["nets"].as_array().unwrap().len(), 2);
    // Assign: release a connector and make it a pipe net instead.
    let cid = c["id"].as_str().unwrap();
    one_step(&mut s, "net.assign", json!({"ids": [cid], "net": null}));
    assert_eq!(net_of(&s, cid), "");
    let r = one_step(&mut s, "net.assign", json!({"ids": [cid], "kind": "pipe"}));
    assert_eq!(s.doc.model.net(r["net"].as_str().unwrap()).unwrap().kind, vizcraft_model::NetKind::Pipe);
    // Split gives the wiring around a connector a new unnamed net.
    let bid = b["id"].as_str().unwrap();
    let old = net_of(&s, bid);
    let r = one_step(&mut s, "net.split", json!({"net": old, "at": bid}));
    assert_ne!(r["net"], json!(old));
    assert_eq!(net_of(&s, bid), r["net"].as_str().unwrap());
}

#[test]
fn net_commands_refuse_junk() {
    let (mut s, main, _, _) = tee();
    let net = net_of(&s, &main);
    for (id, v) in [
        ("net.members", json!({"net": "nope"})),
        ("net.set", json!({"net": net, "kind": "plasma"})),
        ("net.set", json!({"net": net, "label": "sometimes"})),
        ("net.set", json!({"net": net, "properties": 5})),
        ("net.set", json!({"net": net, "properties": {"": 1}})),
        ("net.merge", json!({"nets": [net]})),
        ("net.merge", json!({"nets": [net, "nope"]})),
        ("net.split", json!({"net": net, "at": "nope"})),
        ("net.renumber", json!({"scheme": "alphabetical"})),
        ("net.assign", json!({"ids": ["nope"]})),
        ("net.of", json!({})),
        ("net.of", json!({"shape": "s1"})),
        ("wire.draw", json!({"from": {"x": 0, "y": 0}})),
        ("wire.draw", json!({"from": {"shape": "nope", "point": 0}, "to": {"x": 0, "y": 0}})),
        ("wire.draw", json!({"from": {"junction": main}, "to": {"x": 0, "y": 0}})),
        ("wire.draw", json!({"from": {"connector": main, "x": 0, "y": 100}, "to": {"x": 0, "y": 0}})),
        ("wire.draw", json!({"from": {"x": 0, "y": 0}, "to": {"x": 1, "y": 1}, "kind": "plasma"})),
        ("wire.draw", json!({"from": {"x": 0, "y": 0}, "to": {"x": 1, "y": 1}, "net": "nope"})),
        ("wire.splice", json!({"shape": "nope"})),
        ("wire.heal", json!({"shape": "nope"})),
        ("page.setModule", json!({"module": "furlong"})),
        ("page.setModule", json!({"module": -4})),
        ("shape.mirror", json!({"axis": "z"})),
        ("shape.rotate", json!({"turns": 1.5})),
        ("shape.rotate", json!({})),
    ] {
        refused(&mut s, id, v);
    }
    // Reading an unknown pin or junction is no net, not an error.
    assert_eq!(run(&mut s, "net.of", json!({"shape": "nope", "point": 3}))["net"], Value::Null);
}
