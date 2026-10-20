use serde_json::json;
use vizcraft_model::{Element, Model, PropValue, Relationship};

use super::*;
use crate::{NoTypes, analysis};

/// An element of `mechanical.<ty>` with number or text properties.
fn el(id: &str, ty: &str, props: &[(&str, PropValue)]) -> Element {
    let mut e = Element::new(id, format!("mechanical.{ty}"));
    e.name = id.to_string();
    for (k, v) in props {
        e.properties.insert((*k).to_string(), v.clone());
    }
    e
}

fn rel(id: &str, ty: &str, s: &str, t: &str) -> Relationship {
    Relationship::new(id, format!("mechanical.{ty}"), s, t)
}

fn n(x: f64) -> PropValue {
    PropValue::Number(x)
}

fn t(x: &str) -> PropValue {
    PropValue::Text(x.into())
}

/// A linkage: one ground, joints `j` (id, kind), and links between joints or to the ground `g`.
fn linkage(joints: &[(&str, &str)], links: &[(&str, &str)]) -> Model {
    let mut m = Model::new();
    m.elements.push(el("g", "Ground", &[]));
    for (id, kind) in joints {
        m.elements.push(el(id, "Joint", &[("kind", t(kind))]));
    }
    for (i, (a, b)) in links.iter().enumerate() {
        m.relationships.push(rel(&format!("l{i}"), "Link", a, b));
    }
    m
}

fn m_of(m: &Model) -> i64 {
    mobility(m, &NoTypes, &None, false).m
}

#[test]
fn mobility_of_the_classic_linkages() {
    let four = linkage(
        &[("A", "revolute"), ("B", "revolute"), ("C", "revolute"), ("D", "revolute")],
        &[("g", "A"), ("A", "B"), ("B", "C"), ("C", "D"), ("D", "g")],
    );
    let mob = mobility(&four, &NoTypes, &None, false);
    assert_eq!((mob.n, mob.j1, mob.j2, mob.m), (4, 4, 0, 1), "{}", mob.formula);
    assert_eq!(mob.formula, "M = 3(n - 1) - 2J1 - J2 = 3(4 - 1) - 2·4 - 0 = 1");
    let five = linkage(
        &[("A", "revolute"), ("B", "revolute"), ("C", "revolute"), ("D", "revolute"), ("E", "revolute")],
        &[("g", "A"), ("A", "B"), ("B", "C"), ("C", "D"), ("D", "E"), ("E", "g")],
    );
    assert_eq!(m_of(&five), 2);
    let triangle = linkage(&[("A", "revolute"), ("B", "revolute"), ("C", "revolute")], &[("g", "A"), ("A", "B"), ("B", "C"), ("C", "g")]);
    assert_eq!(m_of(&triangle), 0);
    let slider = linkage(
        &[("A", "revolute"), ("B", "revolute"), ("C", "revolute"), ("D", "prismatic")],
        &[("g", "A"), ("A", "B"), ("B", "C"), ("C", "D"), ("D", "g")],
    );
    assert_eq!(m_of(&slider), 1);
    // A pin in a slot is a half joint.
    let half = linkage(&[("A", "revolute"), ("B", "pinInSlot"), ("C", "revolute")], &[("g", "A"), ("A", "B"), ("B", "C"), ("C", "g")]);
    let mob = mobility(&half, &NoTypes, &None, false);
    assert_eq!((mob.j1, mob.j2, mob.m), (2, 1, 1));
    // A joint of three bodies counts twice, and a plate is one body.
    let mut plate = linkage(&[("A", "revolute"), ("B", "revolute"), ("C", "revolute")], &[("g", "A"), ("A", "P"), ("P", "B"), ("P", "C")]);
    plate.elements.push(el("P", "Plate", &[]));
    let mob = mobility(&plate, &NoTypes, &None, false);
    assert_eq!(mob.n, 2, "the frame and the plate: {:?}", mob.bodies);
    // Spatial counts six per body.
    assert_eq!(mobility(&four, &NoTypes, &None, true).m, 6 * 3 - 5 * 4);
    // The scope limits the count to what a page draws.
    let scope: Scope = Some(["g", "A", "B"].iter().map(|s| s.to_string()).collect());
    assert_eq!(mobility(&four, &NoTypes, &scope, false).joints.len(), 2);
}

#[test]
fn mobility_analysis_reports_the_formula_and_a_row_per_joint() {
    let four = linkage(
        &[("A", "revolute"), ("B", "revolute"), ("C", "revolute"), ("D", "revolute")],
        &[("g", "A"), ("A", "B"), ("B", "C"), ("C", "D"), ("D", "g")],
    );
    let r = analysis("mobility").unwrap().run(&Context { model: &four, types: &NoTypes }, &json!({})).unwrap();
    assert_eq!(r.detail["m"], json!(1));
    assert_eq!(r.table.as_ref().unwrap().rows.len(), 4);
    assert!(r.summary.starts_with("M = 1"), "{}", r.summary);
    assert!(analysis("mobility").unwrap().run(&Context { model: &four, types: &NoTypes }, &json!({"space": "warp"})).is_err());
}

/// Motor, a 20/40 pair, a 15/45 pair and a load, joined by shafts.
fn gearbox() -> Model {
    let mut m = Model::new();
    m.elements = vec![
        el("motor", "Motor", &[]),
        el("g1", "Gear", &[("teeth", n(20.0))]),
        el("g2", "Gear", &[("teeth", n(40.0))]),
        el("g3", "Gear", &[("teeth", n(15.0))]),
        el("g4", "Gear", &[("teeth", n(45.0))]),
        el("load", "Load", &[]),
    ];
    m.relationships = vec![
        rel("s1", "Shaft", "motor", "g1"),
        rel("m1", "Mesh", "g1", "g2"),
        rel("s2", "Shaft", "g2", "g3"),
        rel("m2", "Mesh", "g3", "g4"),
        rel("s3", "Shaft", "g4", "load"),
    ];
    m
}

#[test]
fn two_external_pairs_give_one_to_six_the_same_way_round() {
    let m = gearbox();
    let r = ratio(&m, &NoTypes, &None, "motor", "load").unwrap();
    assert_eq!((r.ratio.text(), r.ratio.sign(), r.ratio.exact), ("1:6".to_string(), 1, Some((1, 6))));
    assert_eq!(r.path, ["motor", "g1", "g2", "g3", "g4", "load"]);
    assert!(r.conflicts.is_empty());
    let back = ratio(&m, &NoTypes, &None, "load", "motor").unwrap();
    assert_eq!(back.ratio.text(), "6:1");
    let a = analysis("ratio").unwrap().run(&Context { model: &m, types: &NoTypes }, &json!({"from": "motor", "to": "load", "speed": 1500})).unwrap();
    assert_eq!((a.detail["ratio"].clone(), a.detail["sign"].clone(), a.detail["speed"].clone()), (json!("1:6"), json!("same"), json!(250.0)));
}

#[test]
fn a_loop_with_two_ratios_is_a_conflict_and_a_disengaged_clutch_cuts_the_path() {
    let mut m = gearbox();
    m.elements.push(el("g5", "Gear", &[("teeth", n(30.0))]));
    // g1 also drives g4 through g5 at a different ratio than the two pairs.
    m.relationships.push(rel("m3", "Mesh", "g1", "g5"));
    m.relationships.push(rel("m4", "Mesh", "g5", "g4"));
    let r = ratio(&m, &NoTypes, &None, "motor", "load").unwrap();
    assert_eq!(r.conflicts.len(), 1, "{:?}", r.conflicts);
    assert_eq!(ratio_conflicts(&m, &NoTypes, &None).len(), 1);
    assert!(ratio_conflicts(&gearbox(), &NoTypes, &None).is_empty());
    let mut c = gearbox();
    c.elements.push(el("cl", "Clutch", &[("engaged", PropValue::Bool(false))]));
    c.relationships.retain(|r| r.id != "s2");
    c.relationships.push(rel("s2a", "Shaft", "g2", "cl"));
    c.relationships.push(rel("s2b", "Shaft", "cl", "g3"));
    assert!(ratio(&c, &NoTypes, &None, "motor", "load").is_err());
    if let Some(e) = c.element_mut("cl") {
        e.properties.insert("engaged".into(), PropValue::Bool(true));
    }
    assert_eq!(ratio(&c, &NoTypes, &None, "motor", "load").unwrap().ratio.text(), "1:6");
}

#[test]
fn internal_mesh_worm_and_crossed_belt_signs() {
    let mut m = Model::new();
    m.elements = vec![
        el("a", "Gear", &[("teeth", n(20.0))]),
        el("b", "Gear", &[("teeth", n(60.0)), ("kind", t("internal"))]),
        el("w", "Gear", &[("kind", t("worm")), ("starts", n(2.0))]),
        el("wh", "Gear", &[("teeth", n(40.0))]),
        el("p1", "Pulley", &[("diameter", n(100.0))]),
        el("p2", "Pulley", &[("diameter", n(250.0))]),
    ];
    let mut belt = rel("bt", "Belt", "p1", "p2");
    belt.properties.insert("crossed".into(), PropValue::Bool(true));
    m.relationships = vec![rel("m1", "Mesh", "a", "b"), rel("m2", "Mesh", "w", "wh"), belt];
    let r = ratio(&m, &NoTypes, &None, "a", "b").unwrap();
    assert_eq!((r.ratio.text(), r.ratio.sign()), ("1:3".to_string(), 1));
    assert_eq!(ratio(&m, &NoTypes, &None, "w", "wh").unwrap().ratio.text(), "1:20");
    let b = ratio(&m, &NoTypes, &None, "p1", "p2").unwrap();
    assert_eq!((b.ratio.text(), b.ratio.sign()), ("2:5".to_string(), -1));
}

/// A unity feedback loop: R -> E(+) -> G -> B -> Y, B -> H -> E(-).
fn feedback(g: &str, h: &str) -> Model {
    let mut m = Model::new();
    m.elements = vec![
        el("r", "Input", &[]),
        el("e", "SummingJunction", &[]),
        el("gb", "TransferBlock", &[("gain", t(g))]),
        el("b", "BranchPoint", &[]),
        el("y", "Output", &[]),
        el("hb", "TransferBlock", &[("gain", t(h))]),
    ];
    let mut minus = rel("s5", "Signal", "hb", "e");
    minus.properties.insert("sign".into(), t("-"));
    m.relationships = vec![
        rel("s1", "Signal", "r", "e"),
        rel("s2", "Signal", "e", "gb"),
        rel("s3", "Signal", "gb", "b"),
        rel("s4", "Signal", "b", "y"),
        rel("s6", "Signal", "b", "hb"),
        minus,
    ];
    m
}

#[test]
fn mason_gives_g_over_one_plus_gh() {
    let m = feedback("G", "H");
    let r = mason(&m, &NoTypes, &None, None, None).unwrap();
    assert_eq!(r.formula, "G / (1 + G·H)");
    assert_eq!((r.paths.len(), r.loops.len(), r.value), (1, 1, None));
    let num = mason(&feedback("10", "0.5"), &NoTypes, &None, None, None).unwrap();
    assert_eq!(num.value, Some(10.0 / 6.0));
    // Integrators make the loop dynamic; plain gains make it algebraic.
    assert_eq!(algebraic_loops(&m, &NoTypes, &None).len(), 1);
    let mut d = feedback("G", "H");
    if let Some(e) = d.element_mut("gb") {
        e.properties.insert("dynamic".into(), PropValue::Bool(true));
    }
    assert!(algebraic_loops(&d, &NoTypes, &None).is_empty());
    let a = analysis("mason").unwrap().run(&Context { model: &m, types: &NoTypes }, &json!({})).unwrap();
    assert_eq!(a.detail["formula"], json!("G / (1 + G·H)"));
    assert!(mason(&Model::new(), &NoTypes, &None, None, None).is_err());
}

#[test]
fn mass_spring_damper_strokes_match_the_textbook() {
    let mut m = Model::new();
    m.elements = vec![el("se", "Se", &[]), el("j", "J1", &[]), el("mass", "I", &[]), el("spring", "C", &[]), el("damper", "R", &[])];
    m.relationships =
        vec![rel("b1", "Bond", "se", "j"), rel("b2", "Bond", "j", "mass"), rel("b3", "Bond", "j", "spring"), rel("b4", "Bond", "j", "damper")];
    let c = assign_causality(&m, &NoTypes, &None);
    assert!(c.conflicts.is_empty(), "{:?}", c.conflicts);
    let got: Vec<(&str, &str)> = c.assigned.iter().map(|(a, b)| (a.as_str(), *b)).collect();
    // The source sets the effort into the junction; the mass takes integral causality and sets
    // the junction's flow; spring and damper take the effort from the junction side.
    assert_eq!(got, [("b1", "end"), ("b2", "end"), ("b3", "start"), ("b4", "start")]);
    // A 0-junction with two effort sources cannot be assigned.
    let mut bad = Model::new();
    bad.elements = vec![el("s1", "Se", &[]), el("s2", "Se", &[]), el("z", "J0", &[])];
    bad.relationships = vec![rel("b1", "Bond", "s1", "z"), rel("b2", "Bond", "s2", "z")];
    assert!(!assign_causality(&bad, &NoTypes, &None).conflicts.is_empty());
}

#[test]
fn long_chains_do_not_overflow_and_results_repeat() {
    let n = 100_000;
    let mut m = Model::new();
    m.elements.push(el("g", "Ground", &[]));
    for i in 0..n {
        m.elements.push(el(&format!("j{i}"), "Joint", &[]));
    }
    for i in 0..n - 1 {
        m.relationships.push(rel(&format!("l{i}"), "Link", &format!("j{i}"), &format!("j{}", i + 1)));
    }
    let a = mobility(&m, &NoTypes, &None, false);
    assert_eq!(a, mobility(&m, &NoTypes, &None, false));
    let mut s = Model::new();
    for i in 0..n {
        s.elements.push(el(&format!("x{i}"), "Gear", &[("teeth", n_teeth(i))]));
    }
    for i in 0..n - 1 {
        s.relationships.push(rel(&format!("s{i}"), "Shaft", &format!("x{i}"), &format!("x{}", i + 1)));
    }
    let r = ratio(&s, &NoTypes, &None, "x0", &format!("x{}", n - 1)).unwrap();
    assert_eq!(r.ratio.text(), "1:1");
    let mut sig = Model::new();
    for i in 0..n {
        sig.elements.push(el(
            &format!("k{i}"),
            if i == 0 {
                "Input"
            } else if i == n - 1 {
                "Output"
            } else {
                "Gain"
            },
            &[("gain", t("2"))],
        ));
    }
    for i in 0..n - 1 {
        sig.relationships.push(rel(&format!("q{i}"), "Signal", &format!("k{i}"), &format!("k{}", i + 1)));
    }
    let r = mason(&sig, &NoTypes, &None, None, None).unwrap();
    assert_eq!(r.paths.len(), 1);
}

fn n_teeth(i: usize) -> PropValue {
    PropValue::Number(10.0 + (i % 7) as f64)
}
