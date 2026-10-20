use serde_json::json;
use vizcraft_model::{Element, Model, PropValue, Relationship};

use super::*;
use crate::{NoTypes, analysis};

fn el(id: &str, ty: &str, props: &[(&str, f64)]) -> Element {
    let mut e = Element::new(id, format!("reliability.{ty}"));
    e.name = id.to_string();
    for (k, v) in props {
        e.properties.insert((*k).to_string(), PropValue::Number(*v));
    }
    e
}

fn rel(id: &str, ty: &str, s: &str, t: &str) -> Relationship {
    Relationship::new(id, format!("reliability.{ty}"), s, t)
}

/// Inputs `(child, parent)` numbered in order.
fn inputs(m: &mut Model, pairs: &[(&str, &str)]) {
    for (i, (c, p)) in pairs.iter().enumerate() {
        m.relationships.push(rel(&format!("i{i}"), "Input", c, p));
    }
}

fn sets(c: &CutSets) -> Vec<Vec<String>> {
    c.sets.iter().map(|s| s.events.clone()).collect()
}

#[test]
fn a_or_b_and_c() {
    let mut m = Model::new();
    m.elements = vec![
        el("top", "TopEvent", &[]),
        el("or", "Or", &[]),
        el("and", "And", &[]),
        el("A", "BasicEvent", &[("probability", 0.01)]),
        el("B", "BasicEvent", &[("probability", 0.1)]),
        el("C", "BasicEvent", &[("probability", 0.2)]),
    ];
    inputs(&mut m, &[("or", "top"), ("A", "or"), ("and", "or"), ("B", "and"), ("C", "and")]);
    let c = cut_sets(&m, &NoTypes, &None, None, 8, 10_000).unwrap();
    assert_eq!(sets(&c), [vec!["A".to_string()], vec!["B".into(), "C".into()]]);
    assert!((c.rare_event - 0.03).abs() < 1e-12);
    assert!((c.upper_bound - (1.0 - 0.99 * 0.98)).abs() < 1e-12);
    let fv: Vec<(String, f64)> = c.importance.clone();
    assert_eq!(fv.len(), 3);
    assert!((fv[0].1 - 0.01 / 0.03).abs() < 1e-12);
    // A house event that is off prunes its branch; one that is on drops out of its sets.
    let mut h = m.clone();
    h.elements.push(el("H", "HouseEvent", &[]));
    h.relationships.push(rel("ih", "Input", "H", "and"));
    assert_eq!(sets(&cut_sets(&h, &NoTypes, &None, None, 8, 10_000).unwrap()), [vec!["A".to_string()]]);
    if let Some(e) = h.element_mut("H") {
        e.properties.insert("state".into(), PropValue::Bool(true));
    }
    assert_eq!(sets(&cut_sets(&h, &NoTypes, &None, None, 8, 10_000).unwrap()).len(), 2);
    // The order cap drops the pair.
    assert_eq!(sets(&cut_sets(&m, &NoTypes, &None, None, 1, 10_000).unwrap()), [vec!["A".to_string()]]);
}

#[test]
fn two_out_of_three_gives_three_pairs_and_transfers_are_followed() {
    let mut m = Model::new();
    m.elements = vec![
        el("top", "TopEvent", &[]),
        el("v", "VotingGate", &[("k", 2.0)]),
        el("A", "BasicEvent", &[("probability", 0.1)]),
        el("B", "BasicEvent", &[("probability", 0.1)]),
        el("tin", "TransferIn", &[]),
        el("tout", "TransferOut", &[]),
        el("C", "BasicEvent", &[("probability", 0.1)]),
    ];
    for id in ["tin", "tout"] {
        if let Some(e) = m.element_mut(id) {
            e.tag = "T1".into();
        }
    }
    inputs(&mut m, &[("v", "top"), ("A", "v"), ("B", "v"), ("tin", "v"), ("C", "tout")]);
    let c = cut_sets(&m, &NoTypes, &None, None, 8, 10_000).unwrap();
    assert_eq!(sets(&c), [vec!["A".to_string(), "B".into()], vec!["A".to_string(), "C".into()], vec!["B".to_string(), "C".into()]]);
    let r = analysis("cutsets").unwrap().run(&Context { model: &m, types: &NoTypes }, &json!({})).unwrap();
    assert_eq!(r.groups.len(), 3);
    assert_eq!(r.table.unwrap().rows.len(), 3);
}

#[test]
fn junk_fault_trees_never_panic() {
    let mut seed = 7u64;
    let mut next = || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as usize
    };
    let kinds = [
        "TopEvent",
        "IntermediateEvent",
        "BasicEvent",
        "HouseEvent",
        "And",
        "Or",
        "Xor",
        "PriorityAnd",
        "Inhibit",
        "VotingGate",
        "TransferIn",
        "TransferOut",
    ];
    for _ in 0..2000 {
        let n = 1 + next() % 12;
        let mut m = Model::new();
        for i in 0..n {
            let mut e =
                el(&format!("e{i}"), kinds[next() % kinds.len()], &[("probability", (next() % 100) as f64 / 50.0 - 0.5), ("k", (next() % 5) as f64)]);
            e.tag = format!("T{}", next() % 3);
            m.elements.push(e);
        }
        for j in 0..next() % 30 {
            m.relationships.push(rel(&format!("r{j}"), "Input", &format!("e{}", next() % n), &format!("e{}", next() % n)));
        }
        let top = format!("e{}", next() % n);
        let c = cut_sets(&m, &NoTypes, &None, Some(&top), 1 + next() % 4, 1 + next() % 50).unwrap();
        assert!(c.rare_event.is_finite() && c.upper_bound.is_finite());
        let _ = availability(&m, &NoTypes, &None, None, None, Some(10.0));
        let _ = outcomes(&m, &NoTypes, &None, None);
    }
}

/// A start, the blocks with availability `p` and paths, and an end.
fn rbd(blocks: &[(&str, f64)], paths: &[(&str, &str)]) -> Model {
    let mut m = Model::new();
    m.elements.push(el("S", "Start", &[]));
    m.elements.push(el("T", "End", &[]));
    for (id, p) in blocks {
        m.elements.push(el(id, "Block", &[("availability", *p)]));
    }
    for (i, (a, b)) in paths.iter().enumerate() {
        m.relationships.push(rel(&format!("p{i}"), "Path", a, b));
    }
    m
}

fn avail(m: &Model) -> f64 {
    availability(m, &NoTypes, &None, None, None, None).unwrap().availability
}

#[test]
fn series_parallel_and_bridge() {
    let series = rbd(&[("a", 0.9), ("b", 0.9)], &[("S", "a"), ("a", "b"), ("b", "T")]);
    assert!((avail(&series) - 0.81).abs() < 1e-12);
    let parallel = rbd(&[("a", 0.9), ("b", 0.9)], &[("S", "a"), ("S", "b"), ("a", "T"), ("b", "T")]);
    assert!((avail(&parallel) - 0.99).abs() < 1e-12);
    let bridge = rbd(
        &[("b1", 0.9), ("b2", 0.9), ("b3", 0.9), ("b4", 0.9), ("b5", 0.9)],
        &[("S", "b1"), ("S", "b2"), ("b1", "b4"), ("b2", "b5"), ("b1", "b3"), ("b2", "b3"), ("b3", "b4"), ("b3", "b5"), ("b4", "T"), ("b5", "T")],
    );
    assert!((avail(&bridge) - 0.97848).abs() < 1e-9, "{}", avail(&bridge));
    // Two out of three.
    let mut v = rbd(&[("a", 0.9), ("b", 0.9), ("c", 0.9)], &[("S", "a"), ("S", "b"), ("S", "c"), ("a", "V"), ("b", "V"), ("c", "V"), ("V", "T")]);
    v.elements.push(el("V", "Voting", &[("k", 2.0)]));
    assert!((avail(&v) - (3.0 * 0.81 * 0.1 + 0.729)).abs() < 1e-12);
    // MTBF and MTTR, and reliability from failure rates.
    let mut d = rbd(&[], &[("S", "x"), ("x", "T")]);
    d.elements.push(el("x", "Block", &[("mtbf", 900.0), ("mttr", 100.0), ("failureRate", 0.001)]));
    let a = availability(&d, &NoTypes, &None, None, None, Some(100.0)).unwrap();
    assert!((a.availability - 0.9).abs() < 1e-12 && (a.reliability.unwrap() - (-0.1f64).exp()).abs() < 1e-12);
    let looped = rbd(&[("a", 0.9), ("b", 0.9)], &[("S", "a"), ("a", "b"), ("b", "a"), ("b", "T")]);
    assert!(availability(&looped, &NoTypes, &None, None, None, None).is_err());
    let r = analysis("availability").unwrap().run(&Context { model: &bridge, types: &NoTypes }, &json!({})).unwrap();
    assert_eq!(r.table.unwrap().rows.len(), 5);
}

#[test]
fn event_tree_outcomes_add_up_to_the_initiator() {
    let mut m = Model::new();
    m.elements = vec![
        el("ie", "Initiator", &[("frequency", 0.1)]),
        el("bp1", "BranchPoint", &[("failureProbability", 0.01)]),
        el("bp2", "BranchPoint", &[]),
        el("bp3", "BranchPoint", &[]),
        el("ok", "Outcome", &[]),
        el("minor", "Outcome", &[]),
        el("major", "Outcome", &[]),
        el("fail", "Outcome", &[]),
    ];
    let mut sf = el("sf", "SafetyFunction", &[("failureProbability", 0.05)]);
    sf.name = "Spray".into();
    m.elements.push(sf);
    for id in ["bp2", "bp3"] {
        if let Some(e) = m.element_mut(id) {
            e.properties.insert("function".into(), PropValue::Text("Spray".into()));
        }
    }
    m.relationships = vec![
        rel("r0", "Success", "ie", "bp1"),
        rel("r1", "Success", "bp1", "bp2"),
        rel("r2", "Failure", "bp1", "bp3"),
        rel("r3", "Success", "bp2", "ok"),
        rel("r4", "Failure", "bp2", "minor"),
        rel("r5", "Success", "bp3", "major"),
        rel("r6", "Failure", "bp3", "fail"),
    ];
    let o = outcomes(&m, &NoTypes, &None, None).unwrap();
    let total: f64 = o.outcomes.iter().map(|(_, f)| f).sum();
    assert!((total - 0.1).abs() < 1e-15, "{total}");
    assert!((o.outcomes[0].1 - 0.1 * 0.99 * 0.95).abs() < 1e-15);
    assert_eq!(o.outcomes.len(), 4);
}

#[test]
fn a_long_series_chain_reduces_without_overflow_and_repeats() {
    let n = 100_000;
    let mut m = Model::new();
    m.elements.push(el("S", "Start", &[]));
    m.elements.push(el("T", "End", &[]));
    for i in 0..n {
        m.elements.push(el(&format!("b{i}"), "Block", &[("availability", 1.0)]));
    }
    m.relationships.push(rel("p", "Path", "S", "b0"));
    for i in 0..n - 1 {
        m.relationships.push(rel(&format!("p{i}"), "Path", &format!("b{i}"), &format!("b{}", i + 1)));
    }
    m.relationships.push(rel("q", "Path", &format!("b{}", n - 1), "T"));
    let a = availability(&m, &NoTypes, &None, None, None, None).unwrap();
    assert!((a.availability - 1.0).abs() < 1e-9);
    let mut ft = Model::new();
    ft.elements.push(el("top", "TopEvent", &[]));
    for i in 0..n {
        ft.elements.push(el(&format!("g{i}"), "Or", &[]));
    }
    ft.elements.push(el("x", "BasicEvent", &[("probability", 0.5)]));
    ft.relationships.push(rel("t", "Input", "g0", "top"));
    for i in 0..n - 1 {
        ft.relationships.push(rel(&format!("i{i}"), "Input", &format!("g{}", i + 1), &format!("g{i}")));
    }
    ft.relationships.push(rel("leaf", "Input", "x", &format!("g{}", n - 1)));
    let c = cut_sets(&ft, &NoTypes, &None, None, 8, 10_000).unwrap();
    assert_eq!(sets(&c), [vec!["x".to_string()]]);
    assert_eq!(c, cut_sets(&ft, &NoTypes, &None, None, 8, 10_000).unwrap());
}
