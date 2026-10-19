use proptest::prelude::*;
use serde_json::{Value, json};
use vizcraft_model::{Element, Model, PropValue, Relationship};

use crate::graph::{CONTAINMENT, Direction, EdgeFilter, Graph, Link};
use crate::{Context, Mark, NoTypes, Types, analyses, analysis, cut, diff, metrics, paths, reach, scc};

/// A model of elements `ids` and relationships `(id, source, target)`, all of one type each,
/// pushed directly so huge test models build in linear time.
fn model(ids: &[&str], rels: &[(&str, &str, &str)]) -> Model {
    let mut m = Model::new();
    m.elements = ids.iter().map(|i| Element::new(*i, "t.Node")).collect();
    m.relationships = rels.iter().map(|(i, s, t)| Relationship::new(*i, "t.Link", *s, *t)).collect();
    m
}

fn graph(m: &Model) -> Graph {
    Graph::from_model(m, &EdgeFilter::default())
}

fn ids(g: &Graph, nodes: &[usize]) -> Vec<String> {
    g.node_ids(nodes.iter().copied())
}

fn run(m: &Model, kind: &str, v: Value) -> crate::AnalysisResult {
    analysis(kind).unwrap().run(&Context { model: m, types: &NoTypes }, &v).unwrap()
}

#[test]
fn cycles_finds_two_linked_cycles_and_a_self_loop() {
    // a <-> b, b -> c, c -> d -> c, e -> e, f alone.
    let m = model(
        &["a", "b", "c", "d", "e", "f"],
        &[("r1", "a", "b"), ("r2", "b", "a"), ("r3", "b", "c"), ("r4", "c", "d"), ("r5", "d", "c"), ("r6", "e", "e")],
    );
    let g = graph(&m);
    let c = scc::cycles(&g);
    let sets: Vec<(Vec<String>, Vec<String>)> = c.iter().map(|x| (x.elements.clone(), x.relationships.clone())).collect();
    assert_eq!(
        sets,
        [
            (vec!["a".to_string(), "b".into()], vec!["r1".to_string(), "r2".into()]),
            (vec!["c".to_string(), "d".into()], vec!["r4".to_string(), "r5".into()]),
            (vec!["e".to_string()], vec!["r6".to_string()]),
        ]
    );
    let r = run(&m, "cycles", json!({}));
    assert_eq!(r.elements, ["a", "b", "c", "d", "e"]);
    assert_eq!(r.relationships, ["r1", "r2", "r4", "r5", "r6"]);
    assert_eq!(r.marks.get("r3"), None, "the link between the cycles is on none");
    assert_eq!(r.groups.len(), 3);
}

#[test]
fn reach_depth_one_versus_unlimited_on_a_chain() {
    let m = model(&["a", "b", "c", "d"], &[("r1", "a", "b"), ("r2", "b", "c"), ("r3", "c", "d")]);
    let g = graph(&m);
    assert_eq!(reach::reach(&g, &[0], Direction::Downstream, Some(1)), [(0, 0), (1, 1)]);
    assert_eq!(reach::reach(&g, &[0], Direction::Downstream, None), [(0, 0), (1, 1), (2, 2), (3, 3)]);
    let r = run(&m, "dependencies", json!({"id": "a", "depth": 1}));
    assert_eq!((r.elements.clone(), r.relationships.clone()), (vec!["a".to_string(), "b".into()], vec!["r1".to_string()]));
    assert_eq!(r.detail["levels"], json!([["a"], ["b"]]));
    assert_eq!(r.marks.get("a"), Some(&Mark::Focus));
    assert_eq!(r.marks.get("b"), Some(&Mark::Downstream));
}

#[test]
fn reach_in_both_directions() {
    let m = model(&["a", "b", "c", "x"], &[("r1", "a", "b"), ("r2", "b", "c")]);
    let g = graph(&m);
    assert_eq!(reach::reach(&g, &[1], Direction::Upstream, None), [(0, 1), (1, 0)]);
    assert_eq!(reach::reach(&g, &[1], Direction::Both, None), [(0, 1), (1, 0), (2, 1)]);
    let r = run(&m, "dependencies", json!({"id": "b", "direction": "both"}));
    assert_eq!(r.elements, ["a", "b", "c"]);
    assert_eq!((r.marks.get("a"), r.marks.get("c")), (Some(&Mark::Upstream), Some(&Mark::Downstream)));
    assert_eq!(r.detail["levels"], json!([["b"], ["a", "c"]]));
}

#[test]
fn unreachable_reports_a_cycle_nothing_feeds() {
    // s -> a; b <-> c has no source feeding it.
    let m = model(&["s", "a", "b", "c"], &[("r1", "s", "a"), ("r2", "b", "c"), ("r3", "c", "b")]);
    let g = graph(&m);
    assert_eq!(ids(&g, &reach::unreachable(&g, &[])), ["b", "c"]);
    assert_eq!(ids(&g, &reach::unreachable(&g, &[2])), ["s", "a"]);
    let r = run(&m, "unreachable", json!({}));
    assert_eq!(r.elements, ["b", "c"]);
    assert_eq!(r.detail["roots"], json!(["s"]));
    let r = run(&m, "unreachable", json!({"from": ["s", "b"]}));
    assert!(r.elements.is_empty());
}

#[test]
fn a_bow_tie_has_one_cut_point_and_a_doubled_link_is_no_bridge() {
    // Two triangles sharing m.
    let m = model(
        &["a", "b", "m", "c", "d"],
        &[("r1", "a", "b"), ("r2", "b", "m"), ("r3", "m", "a"), ("r4", "m", "c"), ("r5", "c", "d"), ("r6", "d", "m")],
    );
    let g = graph(&m);
    let c = cut::cuts(&g);
    assert_eq!(ids(&g, &c.points), ["m"]);
    assert!(c.bridges.is_empty());
    // x - y doubled, y - z single: only y - z is a bridge, and y is a cut point.
    let m = model(&["x", "y", "z"], &[("d1", "x", "y"), ("d2", "y", "x"), ("s1", "y", "z")]);
    let g = graph(&m);
    assert_eq!(g.relationship_ids(cut::bridges(&g)), ["s1"]);
    assert_eq!(ids(&g, &cut::articulation_points(&g)), ["y"]);
    let r = run(&m, "spof", json!({}));
    assert_eq!((r.elements, r.relationships), (vec!["y".to_string()], vec!["s1".to_string()]));
}

#[test]
fn shortest_path_and_none() {
    let m = model(&["a", "b", "c", "d"], &[("r1", "a", "b"), ("r2", "b", "c"), ("r3", "a", "c"), ("r4", "c", "d")]);
    let g = graph(&m);
    let p = paths::shortest_path(&g, 0, 3, Direction::Downstream).unwrap();
    assert_eq!(
        (ids(&g, &p.nodes), g.relationship_ids(p.edges)),
        (vec!["a".to_string(), "c".into(), "d".into()], vec!["r3".to_string(), "r4".into()])
    );
    assert!(paths::shortest_path(&g, 3, 0, Direction::Downstream).is_none());
    assert!(paths::shortest_path(&g, 3, 0, Direction::Upstream).is_some());
    let r = run(&m, "path", json!({"from": "a", "to": "d"}));
    assert_eq!(r.detail["length"], 2);
    let e = analysis("path").unwrap().run(&Context { model: &m, types: &NoTypes }, &json!({"from": "d", "to": "a"}));
    assert!(matches!(e, Err(crate::AnalysisError::Params(_))));
}

#[test]
fn longest_path_in_a_dag_and_marked_cyclic_otherwise() {
    let m = model(&["a", "b", "c", "d"], &[("r1", "a", "b"), ("r2", "b", "c"), ("r3", "a", "c"), ("r4", "c", "d")]);
    let g = graph(&m);
    let l = paths::longest_path(&g);
    assert!(!l.cyclic);
    assert_eq!(g.relationship_ids(l.path.edges), ["r1", "r2", "r4"]);
    assert_eq!(ids(&g, &l.path.nodes), ["a", "b", "c", "d"]);
    let m = model(&["a", "b", "c", "d"], &[("r1", "a", "b"), ("r2", "b", "c"), ("r3", "c", "b"), ("r4", "c", "d")]);
    let g = graph(&m);
    let l = paths::longest_path(&g);
    assert!(l.cyclic);
    // The b-c cycle is one step of the condensation: a -> {b, c} -> d.
    assert_eq!(g.relationship_ids(l.path.edges), ["r1", "r4"]);
    assert_eq!(paths::layers(&g), [0, 1, 1, 2]);
}

#[test]
fn fan_in_and_fan_out() {
    let m = model(&["a", "b", "c"], &[("r1", "a", "c"), ("r2", "b", "c"), ("r3", "c", "a")]);
    let g = graph(&m);
    let x = metrics::metrics(&m, &g);
    let fans: Vec<(usize, usize)> = x.rows.iter().map(|r| (r.fan_in, r.fan_out)).collect();
    assert_eq!(fans, [(1, 1), (0, 1), (2, 1)]);
    assert_eq!((x.totals.elements, x.totals.relationships, x.totals.components, x.totals.cycles), (3, 3, 1, 1));
    let r = run(&m, "metrics", json!({"sort": "fanIn", "limit": 1}));
    assert_eq!(r.table.unwrap().rows[0].id, "c");
    assert_eq!(r.detail["rows"][0]["fanIn"], 2);
}

#[test]
fn containment_depth_is_linear_on_a_256_deep_chain() {
    let mut m = Model::new();
    for i in 0..256 {
        let mut e = Element::new(format!("e{i}"), "t.Node");
        if i > 0 {
            e.owner = Some(format!("e{}", i - 1));
        }
        m.elements.push(e);
    }
    // Deepest first, so each walk meets known depths only after the first.
    m.elements.reverse();
    let d = metrics::containment_depths(&m);
    assert_eq!((d["e0"], d["e1"], d["e255"]), (0, 1, 255));
    let g = Graph::from_model(&m, &EdgeFilter { containment: true, ..EdgeFilter::default() });
    assert_eq!(g.edges.len(), 255);
    assert!(g.edges.iter().all(|e| e.rel == CONTAINMENT && e.id.is_empty()));
    let x = metrics::metrics(&m, &g);
    assert_eq!(x.totals.relationships, 0, "containment edges are not relationships");
    assert_eq!(x.totals.longest_path, 255);
}

#[test]
fn diff_reports_added_removed_and_changed_fields_by_id() {
    let mut before = model(&["a", "b", "c", "gone"], &[("r1", "a", "b"), ("rx", "b", "c")]);
    before.elements[0].name = "Alpha".into();
    before.elements[2].properties.insert("size".into(), PropValue::Number(2.0));
    let mut after = before.clone();
    after.elements.retain(|e| e.id != "gone");
    after.elements[0].name = "Alpha 2".into();
    after.elements[1].type_id = "t.Other".into();
    after.elements[2].owner = Some("a".into());
    after.elements[2].properties.insert("size".into(), PropValue::Number(3.0));
    after.elements[2].properties.insert("color".into(), PropValue::Text("red".into()));
    after.elements.push(Element::new("new", "t.Node"));
    after.relationships.retain(|r| r.id != "rx");
    after.relationships.push(Relationship::new("r2", "t.Link", "a", "new"));
    let d = diff(&before, &after);
    assert_eq!((d.elements.added.clone(), d.elements.removed.clone()), (vec!["new".to_string()], vec!["gone".to_string()]));
    assert_eq!((d.relationships.added.clone(), d.relationships.removed.clone()), (vec!["r2".to_string()], vec!["rx".to_string()]));
    let changed: Vec<(String, Vec<String>)> =
        d.elements.changed.iter().map(|c| (c.id.clone(), c.fields.iter().map(|f| f.field.clone()).collect())).collect();
    assert_eq!(
        changed,
        [
            ("a".to_string(), vec!["name".to_string()]),
            ("b".to_string(), vec!["type".to_string()]),
            ("c".to_string(), vec!["owner".to_string(), "properties.color".into(), "properties.size".into()]),
        ]
    );
    let f = &d.elements.changed[2].fields[1];
    assert_eq!((f.before.clone(), f.after.clone()), (None, Some(json!("red"))));
    assert!(diff(&after, &after).is_empty());
    let r = crate::diff_result(&d);
    assert_eq!((r.marks.get("new"), r.marks.get("a"), r.marks.get("gone")), (Some(&Mark::Added), Some(&Mark::Changed), None));
    assert_eq!(r.groups[0].elements, ["gone"]);
}

#[test]
fn a_100000_chain_and_a_200000_edge_star_do_not_overflow_the_stack() {
    let n = 100_000;
    let nodes: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
    let links = (1..n).map(|i| Link { id: format!("l{i}"), from: format!("n{}", i - 1), to: format!("n{i}"), rel: "t".into() });
    let g = Graph::from_edges(nodes.clone(), links);
    assert_eq!(scc::strongly_connected(&g).groups.len(), n);
    assert_eq!(paths::longest_path(&g).path.edges.len(), n - 1);
    assert_eq!(cut::cuts(&g).bridges.len(), n - 1);
    assert_eq!(reach::reach(&g, &[0], Direction::Downstream, None).len(), n);
    assert!(paths::shortest_path(&g, 0, n - 1, Direction::Downstream).is_some());
    // Star: a hub with two links to each of 100,000 leaves.
    let mut nodes = vec!["hub".to_string()];
    nodes.extend((0..n).map(|i| format!("s{i}")));
    let links = (0..n).flat_map(|i| {
        [
            Link { id: format!("a{i}"), from: "hub".into(), to: format!("s{i}"), rel: "t".into() },
            Link { id: format!("b{i}"), from: format!("s{i}"), to: "hub".into(), rel: "t".into() },
        ]
    });
    let g = Graph::from_edges(nodes, links);
    assert_eq!(g.edges.len(), 2 * n);
    let c = cut::cuts(&g);
    assert_eq!((c.points.len(), c.bridges.len()), (1, 0));
    assert_eq!(scc::cycles(&g).len(), 1);
    assert_eq!(paths::longest_path(&g).path.edges.len(), 0);
}

/// Inverse and subtype knowledge for one test pack.
struct TestTypes;

impl Types for TestTypes {
    fn is_a(&self, ty: &str, ancestor: &str) -> bool {
        ty == ancestor || (ty == "t.Sub" && ancestor == "t.Node")
    }
    fn inverses(&self, rel: &str) -> Vec<String> {
        if rel == "t.dependsOn" { vec!["t.usedBy".into()] } else { Vec::new() }
    }
}

#[test]
fn types_follow_subtypes_and_inverses_backwards() {
    let mut m = model(&["a", "b", "c"], &[]);
    m.elements[1].type_id = "t.Sub".into();
    m.elements.push(Element::new("other", "t.Thing"));
    m.relationships.push(Relationship::new("r1", "t.dependsOn", "a", "b"));
    // c usedBy b means b dependsOn c.
    m.relationships.push(Relationship::new("r2", "t.usedBy", "c", "b"));
    m.relationships.push(Relationship::new("r3", "t.dependsOn", "a", "other"));
    let cx = Context { model: &m, types: &TestTypes };
    let v = json!({"id": "a", "types": ["t.dependsOn"], "elements": ["t.Node"]});
    let r = analysis("dependencies").unwrap().run(&cx, &v).unwrap();
    assert_eq!(r.elements, ["a", "b", "c"]);
    assert_eq!(r.relationships, ["r1", "r2"]);
}

#[test]
fn hostile_params_are_errors_not_panics() {
    let m = model(&["a", "b"], &[("r1", "a", "b")]);
    let cx = Context { model: &m, types: &NoTypes };
    for v in [
        json!(null),
        json!([]),
        json!({"id": 5, "depth": -1}),
        json!({"id": "a", "depth": 0}),
        json!({"id": "a", "depth": 1e300}),
        json!({"id": "a", "direction": "sideways"}),
        json!({"id": "nope"}),
        json!({"types": "x"}),
        json!({"types": [""]}),
        json!({"types": vec!["x"; 100]}),
        json!({"from": [1, 2]}),
        json!({"from": "a"}),
        json!({"sort": "zz", "limit": 0}),
        json!({"containment": "yes"}),
        json!({"from": "a", "to": "x".repeat(10_000)}),
    ] {
        for a in analyses() {
            if let Ok(r) = a.run(&cx, &v) {
                for id in r.ids() {
                    assert!(m.contains_id(&id), "{} {v}: {id}", a.id());
                }
            }
        }
    }
}

fn small_id() -> impl Strategy<Value = String> {
    "[a-e]{1,2}"
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_models_never_panic_and_every_output_id_exists(
        els in proptest::collection::vec((small_id(), proptest::option::of(small_id()), "t\\.(Node|Sub)"), 0..24),
        rels in proptest::collection::vec((small_id(), small_id(), small_id(), "t\\.(Link|dependsOn|usedBy)"), 0..40),
        depth in 1u32..5,
        containment in any::<bool>(),
    ) {
        let mut m = Model::new();
        for (id, owner, ty) in &els {
            let mut e = Element::new(id.clone(), ty.clone());
            e.owner = owner.clone();
            m.elements.push(e);
        }
        for (i, (id, s, t, ty)) in rels.iter().enumerate() {
            m.relationships.push(Relationship::new(format!("r{i}{id}"), ty.clone(), s.clone(), t.clone()));
        }
        let first = m.elements.first().map(|e| e.id.clone()).unwrap_or_default();
        let last = m.elements.last().map(|e| e.id.clone()).unwrap_or_default();
        let cx = Context { model: &m, types: &TestTypes };
        let params = [
            json!({"id": first, "direction": "both", "depth": depth, "containment": containment}),
            json!({"from": first, "to": last, "direction": "upstream", "types": ["t.dependsOn"]}),
            json!({"containment": containment, "elements": ["t.Node"], "sort": "layer"}),
            json!({"from": [last]}),
        ];
        for a in analyses() {
            for v in &params {
                if let Ok(r) = a.run(&cx, v) {
                    for id in r.ids() {
                        prop_assert!(m.contains_id(&id), "{} {}: {}", a.id(), v, id);
                    }
                }
            }
        }
        let mut other = m.clone();
        other.elements.truncate(other.elements.len() / 2);
        let d = diff(&m, &other);
        let r = crate::diff_result(&d);
        for id in r.elements.iter().chain(&r.relationships) {
            prop_assert!(other.contains_id(id));
        }
        for g in &r.groups {
            for id in g.elements.iter().chain(&g.relationships) {
                prop_assert!(m.contains_id(id));
            }
        }
    }
}
