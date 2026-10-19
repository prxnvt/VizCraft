//! Tests of the wire commands, orientation commands and the symbol grid module.

use serde_json::{Value, json};
use vizcraft_diagram::{Point, route_polyline};

use crate::Session;
use crate::cmd::model::fixture::{blank, refused, run};

/// Run `id` and check that one undo takes the document back to where it was.
fn one_step(s: &mut Session, id: &str, v: Value) -> Value {
    let before = s.doc.clone();
    let r = run(s, id, v.clone());
    run(s, "edit.undo", json!({}));
    assert_eq!(s.doc, before, "{id} {v} is one undo step");
    run(s, "edit.redo", json!({}));
    r
}

/// A wire from (0, 100) to (400, 100) and a 40 by 20 device with pins on its left and right,
/// sitting on the wire between x 180 and 220.
fn wire_and_device() -> (Session, String, String) {
    let mut s = blank();
    let w = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 100}, "to": {"x": 400, "y": 100}}));
    let d = run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 180, "y": 90, "w": 40, "h": 20}));
    let id = d["id"].as_str().unwrap().to_string();
    // Pins on the left and right middle, whatever the master's own points are.
    s.doc.pages[0].shape_mut(&id).unwrap().connection_points = vec![Point::new(0.0, 0.5), Point::new(1.0, 0.5)];
    (s, w["id"].as_str().unwrap().to_string(), id)
}

#[test]
fn splice_then_heal_gives_back_the_original_route() {
    let (mut s, w, dev) = wire_and_device();
    let before = route_polyline(&s.doc.pages[0], s.doc.pages[0].connector(&w).unwrap());
    let r = one_step(&mut s, "wire.splice", json!({"shape": dev}));
    let halves: Vec<String> = r["connectors"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
    let page = &s.doc.pages[0];
    let (a, b) = (page.connector(&halves[0]).unwrap(), page.connector(&halves[1]).unwrap());
    assert_eq!((a.end.shape(), b.start.shape()), (Some(dev.as_str()), Some(dev.as_str())));
    assert_eq!(route_polyline(page, a).last().copied(), Some(Point::new(180.0, 100.0)), "the first half ends on the left pin");
    assert_eq!(route_polyline(page, b).first().copied(), Some(Point::new(220.0, 100.0)));
    assert!(a.net.is_some() && b.net.is_some(), "both halves stay segments");
    let r = one_step(&mut s, "wire.heal", json!({"shape": dev}));
    assert_eq!(r["connector"], json!(w));
    let page = &s.doc.pages[0];
    assert!(page.shape(&dev).is_none(), "the device is gone");
    assert_eq!(page.connectors.len(), 1);
    assert_eq!(route_polyline(page, page.connector(&w).unwrap()), before, "the route is the one before the splice");
    assert_eq!(s.doc.model.nets.len(), 1, "the half's net went with it");
}

#[test]
fn deleting_a_device_on_a_wire_heals_it_unless_told_not_to() {
    let (mut s, w, dev) = wire_and_device();
    run(&mut s, "wire.splice", json!({"shape": dev, "connector": w}));
    let r = one_step(&mut s, "shape.delete", json!({"ids": [dev]}));
    assert_eq!(r["healed"], json!([w]));
    assert_eq!(s.doc.pages[0].connectors.len(), 1);
    run(&mut s, "edit.undo", json!({}));
    let r = run(&mut s, "shape.delete", json!({"ids": [dev], "heal": false}));
    assert!(r.get("healed").is_none());
    assert_eq!(s.doc.pages[0].connectors.len(), 2, "both halves stay, their ends freed");
}

#[test]
fn splice_refuses_pins_on_different_runs_and_heal_refuses_two_named_nets() {
    let mut s = blank();
    let w = run(
        &mut s,
        "wire.draw",
        json!({"from": {"x": 0, "y": 100}, "to": {"x": 200, "y": 300}, "points": [{"x": 100, "y": 100}, {"x": 100, "y": 300}]}),
    );
    let d = run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 80, "y": 80, "w": 40, "h": 40}));
    let dev = d["id"].as_str().unwrap().to_string();
    s.doc.pages[0].shape_mut(&dev).unwrap().connection_points = vec![Point::new(0.0, 0.5), Point::new(0.5, 1.0)];
    let e = refused(&mut s, "wire.splice", json!({"shape": dev, "connector": w["id"]}));
    assert!(e.contains("different runs"), "{e}");
    // Two named nets on the device's pins: no healing.
    let (mut s, w, dev) = wire_and_device();
    run(&mut s, "wire.splice", json!({"shape": dev, "connector": w}));
    let nets: Vec<String> = s.doc.pages[0].connectors.iter().filter_map(|c| c.net.clone()).collect();
    run(&mut s, "net.set", json!({"net": nets[0], "name": "A"}));
    run(&mut s, "net.set", json!({"net": nets[1], "name": "B"}));
    let e = refused(&mut s, "wire.heal", json!({"shape": dev}));
    assert!(e.contains("named"), "{e}");
    let r = run(&mut s, "shape.delete", json!({"ids": [dev]}));
    assert!(r.get("healed").is_none(), "the default delete does not heal two named nets either");
}

#[test]
fn rotate_by_turns_and_mirror_change_the_orientation_in_one_step() {
    let (mut s, _, dev) = wire_and_device();
    let r = one_step(&mut s, "shape.rotate", json!({"ids": [dev], "turns": 1}));
    assert_eq!(r["orient"][0]["turns"], 1);
    let sh = s.doc.pages[0].shape(&dev).unwrap();
    assert_eq!((sh.orient.turns, sh.angle_deg), (1, 0.0));
    let p0 = sh.connection_point_world(0).unwrap();
    assert!(p0.distance(Point::new(200.0, 80.0)) < 1e-9, "the left pin turned to the top: {p0:?}");
    one_step(&mut s, "shape.mirror", json!({"ids": [dev], "axis": "y"}));
    let sh = s.doc.pages[0].shape(&dev).unwrap();
    assert_eq!((sh.orient.turns, sh.orient.mirror), (1, true), "R F R^... stays canonical");
    run(&mut s, "shape.mirror", json!({"ids": [dev], "axis": "y"}));
    run(&mut s, "shape.rotate", json!({"ids": [dev], "turns": -1}));
    assert!(s.doc.pages[0].shape(&dev).unwrap().orient.is_default());
    // The absolute angle still works, alone or with turns.
    run(&mut s, "shape.rotate", json!({"ids": [dev], "angle": 30, "turns": 2}));
    let sh = s.doc.pages[0].shape(&dev).unwrap();
    assert_eq!((sh.angle_deg, sh.orient.turns), (30.0, 2));
    // A rotation lock refuses both.
    s.doc.pages[0].shape_mut(&dev).unwrap().locks.rotation = true;
    assert!(s.run("shape.rotate", &json!({"ids": [dev], "turns": 1})).is_err());
    assert!(s.run("shape.mirror", &json!({"ids": [dev], "axis": "x"})).is_err());
}

#[test]
fn the_module_snaps_wires_and_placed_ports() {
    let mut s = blank();
    let r = one_step(&mut s, "page.setModule", json!({"module": "iso2_5mm"}));
    assert_eq!(r["module"], "iso2_5mm");
    let step = vizcraft_model::Module::ISO_STEP;
    let w = run(&mut s, "wire.draw", json!({"from": {"x": 31, "y": 48}, "to": {"x": 101, "y": 48}, "points": [{"x": 60, "y": 48}]}));
    let c = s.doc.pages[0].connector(w["id"].as_str().unwrap()).unwrap();
    let on = |v: f64| ((v / step) - (v / step).round()).abs() < 1e-9;
    let ends = vizcraft_diagram::resolve_ends(&s.doc.pages[0], c);
    assert!([ends.start.pos.x, ends.start.pos.y, ends.end.pos.x, c.waypoints[0].x].into_iter().all(on), "{ends:?}");
    // A typed glyph placed on the page puts its first port on the module.
    let e = run(&mut s, "element.create", json!({"type": "flowchart.Process", "name": "P", "x": 13, "y": 17}));
    let sh = s.doc.pages[0].shape(e["shape"].as_str().unwrap()).unwrap();
    let p = sh.connection_point_world(0).unwrap();
    assert!(on(p.x) && on(p.y), "{p:?}");
    // A custom step, then none: the view goes when it holds nothing else.
    run(&mut s, "page.setModule", json!({"module": 12.5}));
    assert_eq!(vizcraft_diagram::snap::module_step(&s.doc.pages[0]), Some(12.5));
    run(&mut s, "page.setModule", json!({"module": "none"}));
    assert_eq!(vizcraft_diagram::snap::module_step(&s.doc.pages[0]), None);
}

#[test]
fn a_variant_with_its_own_module_sets_the_page_module() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let pack = json!({
            "id": "modvar", "version": "1.0.0", "name": "Module variant test", "module": "iso2_5mm",
            "glyphs": [{"id": "modvar.box", "label": "Box", "category": "Test", "outline": "rectangle", "w": 39.37007874015748, "h": 19.68503937007874, "fill": "#ffffff", "line": "#000000", "elementType": "modvar.Node"},
                       {"id": "modvar.ansi", "label": "Box", "category": "Test", "outline": "rectangle", "w": 40, "h": 20, "fill": "#ffffff", "line": "#000000"}],
            "elementTypes": [{"id": "modvar.Node", "glyph": "modvar.box"}],
            "viewpoints": [{"id": "modvar.Main", "label": "Main", "types": ["*"]}],
            "variants": [{"id": "modvar.ansi", "label": "ANSI", "module": "inch0_1", "glyphs": {"modvar.Node": "modvar.ansi"}}]
        });
        let p = vizcraft_notation::loader::load_json(&pack.to_string()).unwrap();
        vizcraft_notation::catalog::add_pack(p).unwrap();
    });
    let mut s = blank();
    run(&mut s, "model.setViewpoint", json!({"viewpoint": "modvar.Main"}));
    let module = |s: &Session| s.doc.pages[0].view.as_ref().map(|v| v.module);
    assert_eq!(module(&s), Some(vizcraft_model::Module::Iso2_5mm), "the viewpoint brings the pack's module");
    run(&mut s, "element.create", json!({"type": "modvar.Node", "name": "N", "x": 0, "y": 0}));
    one_step(&mut s, "variant.set", json!({"variant": "modvar.ansi"}));
    assert_eq!(module(&s), Some(vizcraft_model::Module::Inch0_1), "the variant brings its own");
}

#[test]
fn hostile_wire_params_never_panic() {
    let junk = [
        json!({"from": {"connector": "c1", "x": 1e308, "y": "a"}, "to": {"junction": 5}}),
        json!({"from": {"x": f64::MAX, "y": f64::MIN}, "to": {"x": 0, "y": 0}, "points": [{"x": 1e308, "y": -1e308}, {}, 5]}),
        json!({"from": {"shape": "s1", "point": 1e20}, "to": {"connector": "c1", "x": 0, "y": 0}, "kind": [], "net": {}}),
        json!({"shape": "s1", "connector": 7, "heal": "yes", "turns": 1e300, "axis": [], "module": {"units": 5}}),
        json!({"nets": [1, null], "into": 5, "force": "no", "at": [], "scheme": 3, "start": -1e308, "step": 1e308, "band": f64::NAN, "prefix": "p".repeat(100_000)}),
        json!({"ids": ["c1", "c1"], "net": null, "kind": "pipe", "page": 99}),
    ];
    let commands: Vec<&str> = crate::cmd::registry()
        .all()
        .iter()
        .map(|c| c.id)
        .filter(|id| id.starts_with("net.") || id.starts_with("wire.") || *id == "page.setModule" || *id == "shape.rotate" || *id == "shape.mirror")
        .collect();
    assert_eq!(commands.len(), 14);
    for id in commands {
        for j in &junk {
            let mut s = blank();
            let w = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 100}, "to": {"x": 300, "y": 100}}));
            run(&mut s, "wire.draw", json!({"from": {"connector": w["id"], "x": 150, "y": 100}, "to": {"x": 150, "y": 300}}));
            run(&mut s, "edit.selectAll", json!({}));
            let _ = s.run(id, j);
            for sh in &s.doc.pages[0].shapes {
                assert!(sh.x.is_finite() && sh.y.is_finite() && sh.orient.turns < 4, "{id}: {j}");
            }
            let mut m = s.doc.model.clone();
            m.sanitize();
            assert_eq!(m, s.doc.model, "{id}: {j}");
        }
    }
}
