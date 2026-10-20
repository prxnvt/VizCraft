//! Tests of `shape.transform`.

use serde_json::{Value, json};
use vizcraft_diagram::{End, Point, resolve_ends};

use crate::Session;
use crate::cmd::model::fixture::{blank, refused, run};

/// Run `id` and check that one undo takes the document back to where it was, then redo it.
fn one_step(s: &mut Session, id: &str, v: Value) -> Value {
    let before = s.doc.clone();
    let r = run(s, id, v.clone());
    assert_ne!(s.doc, before, "{id} {v} changed something");
    run(s, "edit.undo", json!({}));
    assert_eq!(s.doc, before, "{id} {v} is one undo step");
    run(s, "edit.redo", json!({}));
    r
}

/// Shapes `a` (0, 0, 40 by 20) and `b` (200, 100, 40 by 20), each with pins on its left and right
/// middle, and a wire `w` from `a`'s right pin to `b`'s left pin bent at (120, 10) and (120, 110).
fn circuit() -> Session {
    let mut s = blank();
    for (id, x, y) in [("a", 0, 0), ("b", 200, 100)] {
        run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "id": id, "x": x, "y": y, "w": 40, "h": 20}));
        s.doc.pages[0].shape_mut(id).unwrap().connection_points = vec![Point::new(0.0, 0.5), Point::new(1.0, 0.5)];
    }
    let w = run(
        &mut s,
        "wire.draw",
        json!({"from": {"shape": "a", "point": 1}, "to": {"shape": "b", "point": 0}, "points": [{"x": 120, "y": 10}, {"x": 120, "y": 110}]}),
    );
    let id = w["id"].as_str().unwrap().to_string();
    s.doc.pages[0].connector_mut(&id).unwrap().id = "w".into();
    run(&mut s, "edit.selectNone", json!({}));
    s
}

/// The wire still runs from `a`'s right pin to `b`'s left pin, with `bends`.
fn wire_between(s: &Session, bends: &[Point]) {
    let page = &s.doc.pages[0];
    let c = page.connector("w").unwrap();
    assert_eq!((c.start.shape(), c.end.shape()), (Some("a"), Some("b")), "both ends stay glued");
    let ends = resolve_ends(page, c);
    let (pa, pb) = (page.shape("a").unwrap().connection_point_world(1).unwrap(), page.shape("b").unwrap().connection_point_world(0).unwrap());
    assert!(ends.start.pos.distance(pa) < 1e-6 && ends.end.pos.distance(pb) < 1e-6, "{ends:?}");
    assert_eq!(c.waypoints.len(), bends.len(), "{:?}", c.waypoints);
    for (got, want) in c.waypoints.iter().zip(bends) {
        assert!(got.distance(*want) < 1e-6, "bends {:?}, wanted {bends:?}", c.waypoints);
    }
}

fn centre(s: &Session, id: &str) -> Point {
    s.doc.pages[0].shape(id).unwrap().center()
}

#[test]
fn a_group_moves_scales_and_turns_with_its_bent_wire() {
    let mut s = circuit();
    let ids = json!(["a", "b", "w"]);
    let r = one_step(&mut s, "shape.transform", json!({"ids": ids, "move": {"dx": 100, "dy": 50}}));
    assert_eq!(r["ids"], json!(["a", "b", "w"]));
    assert_eq!(centre(&s, "a"), Point::new(120.0, 60.0));
    wire_between(&s, &[Point::new(220.0, 60.0), Point::new(220.0, 160.0)]);

    // Twice the size about the top-left of the group.
    let r = one_step(&mut s, "shape.transform", json!({"ids": ids, "scale": {"sx": 2, "sy": 2, "about": {"x": 100, "y": 50}}}));
    let sh = s.doc.pages[0].shape("b").unwrap();
    assert_eq!((sh.w, sh.h), (80.0, 40.0));
    assert_eq!(centre(&s, "b"), Point::new(540.0, 270.0));
    wire_between(&s, &[Point::new(340.0, 70.0), Point::new(340.0, 270.0)]);
    let b = &r["bounds"];
    assert_eq!((b["x"].as_f64(), b["y"].as_f64(), b["w"].as_f64(), b["h"].as_f64()), (Some(100.0), Some(50.0), Some(480.0), Some(240.0)), "{r}");

    // A quarter turn about (340, 170): (x, y) goes to (340 - (y - 170), 170 + (x - 340)).
    one_step(&mut s, "shape.transform", json!({"ids": ids, "rotate": {"angle": 90, "about": {"x": 340, "y": 170}}}));
    let a = s.doc.pages[0].shape("a").unwrap();
    assert_eq!(a.angle_deg, 90.0);
    assert!(centre(&s, "a").distance(Point::new(440.0, -30.0)) < 1e-6, "{:?}", centre(&s, "a"));
    wire_between(&s, &[Point::new(440.0, 170.0), Point::new(240.0, 170.0)]);
}

#[test]
fn moving_the_shapes_alone_carries_the_wire_between_them_and_one_outside_reroutes() {
    let mut s = circuit();
    run(&mut s, "edit.select", json!({"ids": ["a", "b"]}));
    // No ids: the selection.
    let r = one_step(&mut s, "shape.transform", json!({"move": {"dx": 10, "dy": 0}}));
    assert_eq!(r["ids"], json!(["a", "b", "w"]), "the wire glued at both ends came along");
    wire_between(&s, &[Point::new(130.0, 10.0), Point::new(130.0, 110.0)]);
    // Only `a` and the wire: the wire keeps its end on `b` and drops its bends.
    one_step(&mut s, "shape.transform", json!({"ids": ["a", "w"], "move": {"dx": 0, "dy": -40}}));
    let page = &s.doc.pages[0];
    let c = page.connector("w").unwrap();
    assert_eq!(c.end.shape(), Some("b"));
    assert!(c.waypoints.iter().all(|p| p.distance(Point::new(130.0, 110.0)) > 1e-6), "the old bends are gone: {:?}", c.waypoints);
    assert_eq!(centre(&s, "b"), Point::new(230.0, 110.0), "b stays");
}

#[test]
fn a_symbol_keeps_its_size_and_turns_a_quarter() {
    let mut s = blank();
    let id = run(&mut s, "glyph.insert", json!({"builder": "contact", "x": 100, "y": 120}))["id"].as_str().unwrap().to_string();
    let other =
        run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 300, "y": 120, "w": 40, "h": 40}))["id"].as_str().unwrap().to_string();
    let sh = s.doc.pages[0].shape(&id).unwrap().clone();
    let ids = json!([id, other]);
    one_step(&mut s, "shape.transform", json!({"ids": ids, "scale": {"sx": 2, "sy": 3, "about": {"x": 0, "y": 0}}}));
    let now = s.doc.pages[0].shape(&id).unwrap();
    assert_eq!((now.w, now.h), (sh.w, sh.h), "the symbol never distorts");
    assert!(now.center().distance(Point::new(sh.center().x * 2.0, sh.center().y * 3.0)) < 1e-6);
    assert_eq!(s.doc.pages[0].shape(&other).unwrap().w, 80.0, "a plain shape scales");
    let before = s.doc.pages[0].shape(&id).unwrap().clone();
    one_step(&mut s, "shape.transform", json!({"ids": ids, "rotate": {"angle": 90, "about": {"x": 0, "y": 0}}}));
    let now = s.doc.pages[0].shape(&id).unwrap();
    assert_eq!((now.orient.turns, now.angle_deg), ((before.orient.turns + 1) % 4, before.angle_deg), "a quarter through the orientation");
    let c = before.center();
    assert!(now.center().distance(Point::new(-c.y, c.x)) < 1e-6, "{:?}", now.center());
    assert_eq!(s.doc.pages[0].shape(&other).unwrap().angle_deg, 90.0);
}

#[test]
fn on_a_module_page_first_pins_land_on_the_grid() {
    let mut s = blank();
    run(&mut s, "page.setModule", json!({"module": "iso2_5mm"}));
    let step = vizcraft_model::Module::ISO_STEP;
    let id = run(&mut s, "glyph.insert", json!({"builder": "contact", "x": 100, "y": 120}))["id"].as_str().unwrap().to_string();
    run(&mut s, "shape.transform", json!({"ids": [id], "move": {"dx": 3.3, "dy": 7.1}, "rotate": {"angle": 90, "about": {"x": 1.7, "y": 2.9}}}));
    let p = s.doc.pages[0].shape(&id).unwrap().connection_point_world(0).unwrap();
    let on = |v: f64| ((v / step) - (v / step).round()).abs() < 1e-9;
    assert!(on(p.x) && on(p.y), "{p:?}");
}

#[test]
fn locks_and_protection_refuse() {
    let mut s = circuit();
    let mv = json!({"ids": ["a", "b"], "move": {"dx": 10, "dy": 10}});
    let turn = json!({"ids": ["a", "b"], "rotate": {"angle": 30, "about": {"x": 0, "y": 0}}});
    let grow = json!({"ids": ["a", "b"], "scale": {"sx": 2, "sy": 1, "about": {"x": 0, "y": 0}}});
    s.doc.pages[0].layers = vec![vizcraft_diagram::Layer { id: "L".into(), locked: true, ..Default::default() }];
    s.doc.pages[0].shape_mut("b").unwrap().layers = vec!["L".into()];
    assert!(refused(&mut s, "shape.transform", mv.clone()).contains("locked layer"));
    s.doc.pages[0].layers[0].locked = false;
    s.doc.pages[0].shape_mut("b").unwrap().locks.position = true;
    assert!(refused(&mut s, "shape.transform", mv.clone()).contains("moving"));
    s.doc.pages[0].shape_mut("b").unwrap().locks.position = false;
    s.doc.pages[0].shape_mut("a").unwrap().locks.rotation = true;
    assert!(refused(&mut s, "shape.transform", turn.clone()).contains("rotation"));
    s.doc.pages[0].shape_mut("a").unwrap().locks.rotation = false;
    s.doc.pages[0].shape_mut("a").unwrap().locks.width = true;
    assert!(refused(&mut s, "shape.transform", grow.clone()).contains("resizing"));
    s.doc.pages[0].shape_mut("a").unwrap().locks.width = false;
    s.doc.protection.shapes = true;
    for v in [mv, turn, grow] {
        assert!(refused(&mut s, "shape.transform", v).contains("Protect Document"));
    }
}

#[test]
fn bad_params_are_refused_and_nothing_panics() {
    let mut s = circuit();
    let ids = json!(["a", "b", "w"]);
    for bad in [
        json!({"ids": ids}),
        json!({"ids": ids, "move": 5}),
        json!({"ids": ids, "move": {"dx": "NaN", "dy": 0}}),
        json!({"ids": ids, "scale": {"sx": 0, "sy": 1, "about": {"x": 0, "y": 0}}}),
        json!({"ids": ids, "scale": {"sx": -1, "sy": 1, "about": {"x": 0, "y": 0}}}),
        json!({"ids": ids, "scale": {"sx": 2, "sy": 2}}),
        json!({"ids": ids, "rotate": {"angle": 90, "about": {"x": "a", "y": 0}}}),
        json!({"ids": ids, "rotate": {"about": {"x": 0, "y": 0}}}),
        json!({"ids": ["nope"], "move": {"dx": 1, "dy": 1}}),
    ] {
        refused(&mut s, "shape.transform", bad);
    }
    // Huge numbers are clamped, not refused.
    run(
        &mut s,
        "shape.transform",
        json!({"ids": ids, "move": {"dx": 1e308, "dy": -1e308}, "scale": {"sx": 1e300, "sy": 1e-5, "about": {"x": 1e308, "y": 0}}, "rotate": {"angle": 1e300, "about": {"x": 0, "y": -1e308}}}),
    );
    let page = &s.doc.pages[0];
    assert!(page.shapes.iter().all(|sh| [sh.x, sh.y, sh.w, sh.h, sh.angle_deg].iter().all(|v| v.is_finite())));
    let c = page.connector("w").unwrap();
    assert!(c.waypoints.iter().all(|p| p.is_finite()));
    assert!(matches!(c.start, End::Glued { .. }));
}
