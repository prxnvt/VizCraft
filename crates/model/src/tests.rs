//! Unit tests for sanitize, containment and the identity registry.

use std::collections::BTreeMap;

use proptest::prelude::*;
use serde_json::json;

use crate::identity::MAX_FORMAT;
use crate::*;

fn el(id: &str, ty: &str) -> Element {
    Element::new(id, ty)
}

fn owned(id: &str, owner: &str) -> Element {
    Element { owner: Some(owner.into()), ..el(id, "t.T") }
}

fn rel(id: &str, s: &str, t: &str) -> Relationship {
    Relationship::new(id, "t.Flow", s, t)
}

fn model(elements: Vec<Element>, relationships: Vec<Relationship>) -> Model {
    Model { elements, relationships, ..Model::new() }
}

fn load(v: serde_json::Value) -> Model {
    let mut m: Model = serde_json::from_value(v).unwrap();
    m.sanitize();
    m
}

fn ids(m: &Model) -> Vec<&str> {
    m.elements.iter().map(|e| e.id.as_str()).collect()
}

fn rel_ids(m: &Model) -> Vec<&str> {
    m.relationships.iter().map(|r| r.id.as_str()).collect()
}

fn sanitized_twice_is_stable(m: &Model) {
    let mut again = m.clone();
    again.sanitize();
    assert_eq!(&again, m);
}

// ---- tolerant loading and sanitize ----

#[test]
fn junk_field_types_fall_back_to_defaults() {
    let m = load(json!({
        "schemaVersion": "one",
        "packs": [{"id": "flowchart", "version": 3}, 7, {"id": ["x"]}],
        "elements": [
            {"id": "e1", "type": 5, "name": {"a": 1}, "tag": null, "owner": 12, "properties": [1, 2]},
            "not an element",
            {"id": "e2", "type": "flowchart.Process", "properties": {"a": true, "b": 2.5, "c": "x", "d": [1], "e": null, "f": {"g": 1}}},
            {"id": 42},
            {}
        ],
        "relationships": {"not": "a list"}
    }));
    assert_eq!(m.schema_version, SCHEMA_VERSION);
    assert_eq!(m.packs, vec![PackRef { id: "flowchart".into(), version: String::new() }]);
    assert_eq!(ids(&m), vec!["e1", "e2"]);
    let e1 = m.element("e1").unwrap();
    assert_eq!(e1.type_id, "");
    assert_eq!(e1.owner, None);
    assert!(e1.properties.is_empty());
    let e2 = m.element("e2").unwrap();
    assert_eq!(e2.properties.keys().collect::<Vec<_>>(), vec!["a", "b", "c"]);
    assert!(m.relationships.is_empty());
    sanitized_twice_is_stable(&m);
}

#[test]
fn missing_fields_and_empty_object_load() {
    let m = load(json!({}));
    assert_eq!(m, Model::new());
    let m = load(json!({"elements": [{"id": "a"}]}));
    assert_eq!(m.elements, vec![el("a", "")]);
}

#[test]
fn non_object_top_level_is_an_error_not_a_panic() {
    for text in ["5", "null", "[]", "\"x\"", "{", "{\"elements\": [", ""] {
        let _ = serde_json::from_str::<Model>(text);
    }
}

#[test]
fn out_of_range_numbers_do_not_panic() {
    let text = r#"{"elements":[{"id":"a","properties":{"big":1e999,"ok":1}}]}"#;
    if let Ok(mut m) = serde_json::from_str::<Model>(text) {
        m.sanitize();
        assert!(m.elements.iter().flat_map(|e| e.properties.values()).all(|v| v.as_f64().is_none_or(f64::is_finite)));
    }
}

#[test]
fn schema_version_zero_becomes_current_and_others_are_kept() {
    let m = load(json!({"schemaVersion": 0}));
    assert_eq!(m.schema_version, SCHEMA_VERSION);
    let m = load(json!({"schemaVersion": 7}));
    assert_eq!(m.schema_version, 7);
}

#[test]
fn empty_and_duplicate_ids_are_dropped_keeping_the_first() {
    let mut m = model(
        vec![el("", "t.A"), el("a", "t.First"), el("a", "t.Second"), el("b", "t.B")],
        vec![rel("", "a", "b"), rel("r", "a", "b"), rel("r", "b", "a"), rel("a", "a", "b"), rel("b2", "b", "b")],
    );
    m.sanitize();
    assert_eq!(ids(&m), vec!["a", "b"]);
    assert_eq!(m.element("a").unwrap().type_id, "t.First");
    // `a` is an element id, so the relationship with that id is the duplicate.
    assert_eq!(rel_ids(&m), vec!["r", "b2"]);
    assert_eq!(m.relationship("r").unwrap().source, "a");
    sanitized_twice_is_stable(&m);
}

#[test]
fn dangling_relationship_ends_are_dropped() {
    let m = load(json!({
        "elements": [{"id": "a"}, {"id": "b"}],
        "relationships": [
            {"id": "r1", "source": "a", "target": "b"},
            {"id": "r2", "source": "a", "target": "ghost"},
            {"id": "r3", "source": "", "target": "b"},
            {"id": "r4"}
        ]
    }));
    assert_eq!(rel_ids(&m), vec!["r1"]);
}

#[test]
fn missing_and_self_owners_are_cleared() {
    let mut m = model(vec![owned("a", "ghost"), owned("b", "b"), owned("c", "a"), Element { owner: Some(String::new()), ..el("d", "t.T") }], vec![]);
    m.sanitize();
    let owners: Vec<_> = m.elements.iter().map(|e| e.owner.as_deref()).collect();
    assert_eq!(owners, vec![None, None, Some("a"), None]);
}

#[test]
fn owner_of_a_dropped_duplicate_still_resolves_to_the_kept_element() {
    let mut m = model(vec![el("a", "t.A"), owned("a", "zzz"), owned("c", "a")], vec![]);
    m.sanitize();
    assert_eq!(ids(&m), vec!["a", "c"]);
    assert_eq!(m.element("c").unwrap().owner.as_deref(), Some("a"));
}

#[test]
fn ownership_cycles_are_broken() {
    // a -> b -> c -> a, plus d hanging off the cycle.
    let mut m = model(vec![owned("a", "b"), owned("b", "c"), owned("c", "a"), owned("d", "b")], vec![]);
    m.sanitize();
    // The walk starts at `a` and climbs b, c; `c` closes the loop and moves to the root.
    assert_eq!(m.element("c").unwrap().owner, None);
    assert_eq!(m.element("b").unwrap().owner.as_deref(), Some("c"));
    assert_eq!(m.element("a").unwrap().owner.as_deref(), Some("b"));
    assert_eq!(m.element("d").unwrap().owner.as_deref(), Some("b"));
    assert_eq!(m.ancestors("a"), vec!["b", "c"]);
    sanitized_twice_is_stable(&m);
}

#[test]
fn two_element_cycle_and_separate_cycles() {
    let mut m = model(vec![owned("a", "b"), owned("b", "a"), owned("x", "y"), owned("y", "x"), el("z", "t.T")], vec![]);
    m.sanitize();
    for e in &m.elements {
        assert!(m.ancestors(&e.id).len() <= 1);
        assert!(!m.ancestors(&e.id).contains(&e.id));
    }
    assert_eq!(m.elements.iter().filter(|e| e.owner.is_none()).count(), 3);
}

#[test]
fn chains_deeper_than_max_depth_are_cut() {
    let n = MAX_DEPTH * 2 + 10;
    let chain: Vec<Element> = (0..n).map(|i| if i == 0 { el("e0", "t.T") } else { owned(&format!("e{i}"), &format!("e{}", i - 1)) }).collect();
    // Root first settles one element per walk; deepest first climbs the whole chain in one walk.
    // Both must cut at the same places.
    let mut deepest_first = chain.clone();
    deepest_first.reverse();
    for elements in [chain, deepest_first] {
        let mut m = model(elements, vec![]);
        m.sanitize();
        for e in &m.elements {
            assert!(m.ancestors(&e.id).len() <= MAX_DEPTH, "{} too deep", e.id);
        }
        let mut roots: Vec<&str> = m.elements.iter().filter(|e| e.owner.is_none()).map(|e| e.id.as_str()).collect();
        roots.sort();
        let mut want = vec!["e0".to_string(), format!("e{}", MAX_DEPTH + 1), format!("e{}", 2 * MAX_DEPTH + 2)];
        want.sort();
        assert_eq!(roots, want);
        assert_eq!(m.ancestors(&format!("e{MAX_DEPTH}")).len(), MAX_DEPTH);
        sanitized_twice_is_stable(&m);
    }
}

#[test]
fn lengths_are_capped_at_char_boundaries() {
    let long_id = "é".repeat(MAX_ID); // two bytes each
    let long_text = "ü".repeat(MAX_TEXT);
    let mut props = BTreeMap::new();
    props.insert("k".repeat(MAX_KEY + 5), PropValue::Text(long_text.clone()));
    props.insert(String::new(), PropValue::Bool(true));
    props.insert("nan".into(), PropValue::Number(f64::NAN));
    props.insert("inf".into(), PropValue::Number(f64::INFINITY));
    props.insert("ok".into(), PropValue::Number(1.5));
    let e = Element {
        id: long_id.clone(),
        type_id: "t".repeat(MAX_KEY * 2),
        name: long_text.clone(),
        tag: "€".repeat(MAX_TAG),
        owner: None,
        properties: props,
    };
    let mut m = model(vec![e], vec![]);
    m.sanitize();
    let e = &m.elements[0];
    assert_eq!(e.id.len(), MAX_ID);
    assert_eq!(e.type_id.len(), MAX_KEY);
    assert!(e.name.len() <= MAX_TEXT && e.name.len() > MAX_TEXT - 4);
    assert!(e.tag.len() <= MAX_TAG && e.tag.len() > MAX_TAG - 4);
    assert_eq!(e.properties.len(), 2);
    assert_eq!(e.properties.get("ok"), Some(&PropValue::Number(1.5)));
    let (k, v) = e.properties.iter().find(|(k, _)| k.starts_with('k')).unwrap();
    assert_eq!(k.len(), MAX_KEY);
    assert!(v.as_str().unwrap().len() <= MAX_TEXT);
    sanitized_twice_is_stable(&m);
}

#[test]
fn property_keys_colliding_after_cut_keep_the_first() {
    let base = "p".repeat(MAX_KEY);
    let mut props = BTreeMap::new();
    props.insert(format!("{base}a"), PropValue::Number(1.0));
    props.insert(format!("{base}b"), PropValue::Number(2.0));
    let mut m = model(vec![Element { properties: props, ..el("a", "t.T") }], vec![]);
    m.sanitize();
    assert_eq!(m.elements[0].properties.get(&base), Some(&PropValue::Number(1.0)));
    assert_eq!(m.elements[0].properties.len(), 1);
}

#[test]
fn property_count_is_capped() {
    let props: BTreeMap<String, PropValue> = (0..MAX_PROPERTIES + 50).map(|i| (format!("k{i:05}"), PropValue::Bool(true))).collect();
    let mut m = model(vec![Element { properties: props.clone(), ..el("a", "t.T") }], vec![Relationship { properties: props, ..rel("r", "a", "a") }]);
    m.sanitize();
    assert_eq!(m.elements[0].properties.len(), MAX_PROPERTIES);
    assert_eq!(m.relationships[0].properties.len(), MAX_PROPERTIES);
    assert!(m.elements[0].properties.contains_key("k00000"));
}

#[test]
fn relationship_strings_and_ports_are_cleaned() {
    let mut m = model(
        vec![el("a", "t.T")],
        vec![Relationship {
            source_port: Some(String::new()),
            target_port: Some("x".repeat(MAX_KEY + 1)),
            name: "n".repeat(MAX_TEXT + 1),
            ..rel("r", "a", "a")
        }],
    );
    m.sanitize();
    let r = &m.relationships[0];
    assert_eq!(r.source_port, None);
    assert_eq!(r.target_port.as_ref().unwrap().len(), MAX_KEY);
    assert_eq!(r.name.len(), MAX_TEXT);
}

#[test]
fn long_ids_that_collide_after_cut_are_deduplicated_and_ends_follow() {
    let base = "a".repeat(MAX_ID);
    let mut m = model(
        vec![el(&format!("{base}1"), "t.First"), el(&format!("{base}2"), "t.Second")],
        vec![rel("r", &format!("{base}2"), &format!("{base}1"))],
    );
    m.sanitize();
    assert_eq!(ids(&m), vec![base.as_str()]);
    assert_eq!(m.elements[0].type_id, "t.First");
    // Both ends cut to the kept id, so the relationship stays as a self loop.
    assert_eq!(m.relationships[0].source, base);
}

#[test]
fn packs_are_capped_and_deduplicated() {
    let mut packs: Vec<serde_json::Value> = (0..Model::MAX_PACKS + 10).map(|i| json!({"id": format!("p{i}"), "version": "1"})).collect();
    packs.insert(1, json!({"id": "p0", "version": "2"}));
    packs.insert(2, json!({"id": "", "version": "2"}));
    packs.insert(3, json!({"id": "x".repeat(MAX_KEY * 2), "version": "v".repeat(MAX_KEY * 2)}));
    let m = load(json!({ "packs": packs }));
    assert!(m.packs.len() <= Model::MAX_PACKS);
    assert_eq!(m.packs[0], PackRef { id: "p0".into(), version: "1".into() });
    assert_eq!(m.packs[1].id.len(), MAX_KEY);
    assert_eq!(m.packs[1].version.len(), MAX_KEY);
    assert_eq!(m.packs.iter().filter(|p| p.id == "p0").count(), 1);
    sanitized_twice_is_stable(&m);
}

#[test]
fn huge_counts_keep_the_first_ones() {
    let elements: Vec<Element> = (0..MAX_ELEMENTS + 5).map(|i| el(&format!("e{i}"), "t.T")).collect();
    let relationships: Vec<Relationship> = (0..MAX_RELATIONSHIPS + 5).map(|i| rel(&format!("r{i}"), "e0", "e1")).collect();
    let mut m = model(elements, relationships);
    m.sanitize();
    assert_eq!(m.elements.len(), MAX_ELEMENTS);
    assert_eq!(m.relationships.len(), MAX_RELATIONSHIPS);
    assert_eq!(m.elements.last().unwrap().id, format!("e{}", MAX_ELEMENTS - 1));
}

#[test]
fn large_model_containment_stays_fast() {
    // A wide and deep tree at the element cap; quadratic code would take minutes here.
    let elements: Vec<Element> =
        (0..MAX_ELEMENTS).map(|i| if i == 0 { el("e0", "t.T") } else { owned(&format!("e{i}"), &format!("e{}", (i - 1) / 2)) }).collect();
    let relationships: Vec<Relationship> = (1..MAX_ELEMENTS).map(|i| rel(&format!("r{i}"), "e0", &format!("e{i}"))).collect();
    let mut m = model(elements, relationships);
    m.sanitize();
    assert_eq!(m.elements.len(), MAX_ELEMENTS);
    assert_eq!(m.descendants("e0").len(), MAX_ELEMENTS - 1);
    let removed = m.remove(&["e1".to_string()]);
    assert!(removed.elements.len() > MAX_ELEMENTS / 3);
    assert_eq!(removed.relationships.len(), removed.elements.len());
    let left: std::collections::HashSet<&str> = m.elements.iter().map(|e| e.id.as_str()).collect();
    assert!(m.elements.iter().all(|e| e.owner.as_deref().is_none_or(|o| left.contains(o))));
}

#[test]
fn sanitize_is_idempotent_on_a_clean_model() {
    let mut m = model(vec![el("a", "t.T"), owned("b", "a")], vec![rel("r", "a", "b")]);
    m.use_pack(PackRef { id: "t".into(), version: "1".into() });
    let before = m.clone();
    m.sanitize();
    assert_eq!(m, before);
}

#[test]
fn round_trip_through_json() {
    let mut m = model(vec![el("a", "t.T"), owned("b", "a")], vec![rel("r", "a", "b")]);
    m.elements[0].properties.insert("n".into(), PropValue::Number(2.0));
    m.elements[0].tag = "S1".into();
    let text = serde_json::to_string(&m).unwrap();
    let back: Model = serde_json::from_str(&text).unwrap();
    assert_eq!(back, m);
}

// ---- containment ----

fn tree() -> Model {
    // root1 { a { a1, a2 { a21 } }, b }, root2
    model(
        vec![
            el("root1", "t.T"),
            owned("a", "root1"),
            owned("a1", "a"),
            owned("a2", "a"),
            owned("a21", "a2"),
            owned("b", "root1"),
            el("root2", "t.T"),
        ],
        vec![rel("r1", "a21", "root2"), rel("r2", "root2", "b"), rel("r3", "b", "root2"), rel("r4", "root1", "root1")],
    )
}

#[test]
fn descendants_are_parents_first() {
    let m = tree();
    assert_eq!(m.descendants("root1"), vec!["a", "b", "a1", "a2", "a21"]);
    assert_eq!(m.descendants("a2"), vec!["a21"]);
    assert!(m.descendants("a21").is_empty());
    assert!(m.descendants("ghost").is_empty());
}

#[test]
fn descendants_and_ancestors_survive_cycles() {
    let m = model(vec![owned("a", "b"), owned("b", "c"), owned("c", "a"), owned("self", "self")], vec![]);
    assert_eq!(m.descendants("a"), vec!["c", "b"]);
    assert_eq!(m.ancestors("a"), vec!["b", "c"]);
    assert!(m.descendants("self").is_empty());
    assert!(m.ancestors("self").is_empty());
}

#[test]
fn walks_are_bounded_by_max_depth() {
    let n = MAX_DEPTH + 20;
    let elements: Vec<Element> = (0..n).map(|i| if i == 0 { el("e0", "t.T") } else { owned(&format!("e{i}"), &format!("e{}", i - 1)) }).collect();
    let m = model(elements, vec![]);
    assert_eq!(m.descendants("e0").len(), MAX_DEPTH);
    assert_eq!(m.ancestors(&format!("e{}", n - 1)).len(), MAX_DEPTH);
}

#[test]
fn ancestors_run_to_the_root() {
    let m = tree();
    assert_eq!(m.ancestors("a21"), vec!["a2", "a", "root1"]);
    assert!(m.ancestors("root1").is_empty());
    assert!(m.ancestors("ghost").is_empty());
    // A dangling owner stops the walk.
    let m = model(vec![owned("a", "ghost")], vec![]);
    assert!(m.ancestors("a").is_empty());
}

#[test]
fn set_owner_moves_and_refuses_cycles() {
    let mut m = tree();
    assert_eq!(m.set_owner("a", Some("a")), Err(ModelError::Cycle("a".into())));
    assert_eq!(m.set_owner("a", Some("a21")), Err(ModelError::Cycle("a".into())));
    assert_eq!(m.set_owner("root1", Some("a1")), Err(ModelError::Cycle("root1".into())));
    assert_eq!(m.set_owner("a", Some("ghost")), Err(ModelError::Unknown("ghost".into())));
    assert_eq!(m.set_owner("ghost", None), Err(ModelError::Unknown("ghost".into())));
    assert_eq!(m.set_owner("ghost", Some("a")), Err(ModelError::Unknown("ghost".into())));
    assert_eq!(m, tree());
    m.set_owner("a2", Some("root2")).unwrap();
    assert_eq!(m.ancestors("a21"), vec!["a2", "root2"]);
    m.set_owner("a2", None).unwrap();
    assert!(m.ancestors("a2").is_empty());
    m.set_owner("root1", Some("b")).unwrap_err();
    m.set_owner("b", Some("a1")).unwrap();
    assert_eq!(m.ancestors("b"), vec!["a1", "a", "root1"]);
}

#[test]
fn set_owner_on_a_cyclic_model_still_terminates() {
    let mut m = model(vec![owned("a", "b"), owned("b", "a"), el("c", "t.T")], vec![]);
    assert_eq!(m.set_owner("c", Some("a")), Ok(()));
    assert_eq!(m.set_owner("a", Some("c")), Err(ModelError::Cycle("a".into())));
}

#[test]
fn remove_cascades_to_descendants_and_relationships() {
    let mut m = tree();
    let removed = m.remove(&["a".to_string()]);
    assert_eq!(removed.elements, vec!["a", "a1", "a2", "a21"]);
    assert_eq!(removed.relationships, vec!["r1"]);
    assert_eq!(ids(&m), vec!["root1", "b", "root2"]);
    assert_eq!(rel_ids(&m), vec!["r2", "r3", "r4"]);
}

#[test]
fn remove_named_relationships_and_skips_unknown_ids() {
    let mut m = tree();
    let removed = m.remove(&["r2".to_string(), "ghost".to_string(), "r4".to_string()]);
    assert!(removed.elements.is_empty());
    assert_eq!(removed.relationships, vec!["r2", "r4"]);
    assert_eq!(m.elements.len(), 7);
}

#[test]
fn remove_overlapping_ids_reports_each_once_in_order() {
    let mut m = tree();
    let removed = m.remove(&["a2".to_string(), "root1".to_string(), "a21".to_string(), "r2".to_string()]);
    assert_eq!(removed.elements, vec!["a2", "a21", "root1", "a", "b", "a1"]);
    assert_eq!(removed.relationships, vec!["r1", "r2", "r3", "r4"]);
    assert_eq!(ids(&m), vec!["root2"]);
    assert!(m.relationships.is_empty());
}

#[test]
fn remove_on_cyclic_and_deep_models_terminates_and_leaves_no_orphans() {
    let mut m = model(vec![owned("a", "b"), owned("b", "a"), owned("c", "b"), el("d", "t.T")], vec![rel("r", "c", "d")]);
    let removed = m.remove(&["a".to_string()]);
    assert_eq!(removed.elements, vec!["a", "b", "c"]);
    assert_eq!(removed.relationships, vec!["r"]);
    let n = MAX_DEPTH + 50;
    let elements: Vec<Element> = (0..n).map(|i| if i == 0 { el("e0", "t.T") } else { owned(&format!("e{i}"), &format!("e{}", i - 1)) }).collect();
    let mut m = model(elements, vec![]);
    assert_eq!(m.remove(&["e0".to_string()]).elements.len(), n);
    assert!(m.elements.is_empty());
}

// ---- identity: formats ----

fn with_props(ty: &str, props: &[(&str, PropValue)]) -> Element {
    Element { properties: props.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(), ..el("", ty) }
}

#[test]
fn format_parse_errors() {
    for bad in ["{", "S{n", "}", "a}b", "{}", "{n:}", "{n:x}", "{n:13}", "{n:-1}", "{n}{n}", "{n}-{n:2}", "{a b}", "{a-b}", "{é}"] {
        assert!(matches!(IdFormat::parse(bad), Err(ModelError::Format(_))), "{bad} should fail");
    }
    assert!(IdFormat::parse(&"x".repeat(MAX_FORMAT + 1)).is_err());
    assert!(IdFormat::parse(&"x".repeat(MAX_FORMAT)).is_ok());
    assert!(IdFormat::parse("{n:99999999999999999999999}").is_err());
}

#[test]
fn format_parse_accepts_the_grammar() {
    for good in ["", "S{n}", "{n:3}", "{n:0}", "{n:12}", "{letters}-{n:3}", "{{{n}}}", "{a_1}{B}", "plain"] {
        assert!(IdFormat::parse(good).is_ok(), "{good} should parse");
    }
    let f = IdFormat::parse("S{n}").unwrap();
    assert_eq!(f.source(), "S{n}");
    assert!(f.has_number());
    assert!(!IdFormat::parse("{a}").unwrap().has_number());
}

#[test]
fn compose_fills_parts() {
    let e = with_props("t.T", &[("letters", PropValue::Text("FIC".into())), ("flag", PropValue::Bool(true)), ("x", PropValue::Number(2.5))]);
    assert_eq!(IdFormat::parse("S{n}").unwrap().compose(&e, 7), "S7");
    assert_eq!(IdFormat::parse("{letters}-{n:3}").unwrap().compose(&e, 101), "FIC-101");
    assert_eq!(IdFormat::parse("{letters}-{n:3}").unwrap().compose(&e, 7), "FIC-007");
    assert_eq!(IdFormat::parse("{{{n}}}").unwrap().compose(&e, 1), "{1}");
    assert_eq!(IdFormat::parse("{missing}{flag}{x}").unwrap().compose(&e, 1), "true2.5");
}

#[test]
fn number_in_matches_literals_props_and_digits() {
    let e = with_props("t.T", &[("letters", PropValue::Text("FIC".into()))]);
    let s = IdFormat::parse("S{n}").unwrap();
    assert_eq!(s.number_in("S1", &e), Some(1));
    assert_eq!(s.number_in("S0042", &e), Some(42));
    assert_eq!(s.number_in("S", &e), None);
    assert_eq!(s.number_in("S1a", &e), None);
    assert_eq!(s.number_in("T1", &e), None);
    assert_eq!(s.number_in("S-1", &e), None);
    assert_eq!(s.number_in("S١", &e), None); // a non-ASCII digit
    assert_eq!(s.number_in("", &e), None);
    let isa = IdFormat::parse("{letters}-{n:3}").unwrap();
    assert_eq!(isa.number_in("FIC-101", &e), Some(101));
    assert_eq!(isa.number_in("FIC-1001", &e), Some(1001));
    assert_eq!(isa.number_in("FIC-01", &e), None);
    assert_eq!(isa.number_in("TIC-101", &e), None);
    let suffix = IdFormat::parse("{n}{letters}").unwrap();
    assert_eq!(suffix.number_in("12FIC", &e), Some(12));
    assert_eq!(suffix.number_in("FIC", &e), None);
    let digits_around = IdFormat::parse("1{n}1").unwrap();
    assert_eq!(digits_around.number_in("151", &e), Some(5));
    assert_eq!(digits_around.number_in("11", &e), None);
    assert_eq!(IdFormat::parse("S").unwrap().number_in("S", &e), None);
}

#[test]
fn number_in_saturates() {
    let e = el("", "t.T");
    let s = IdFormat::parse("S{n}").unwrap();
    assert_eq!(s.number_in("S99999999999999999999", &e), Some(u64::MAX));
    assert_eq!(s.number_in(&format!("S{}", "9".repeat(10_000)), &e), Some(u64::MAX));
}

// ---- identity: registry ----

fn scope(id: &str, types: &[&str], format: &str) -> IdScope {
    IdScope {
        id: id.into(),
        types: types.iter().map(|t| t.to_string()).collect(),
        format: format.into(),
        unique: true,
        paired: false,
        auto_number: true,
        per_owner: false,
        number_by: String::new(),
    }
}

fn tagged(id: &str, ty: &str, tag: &str) -> Element {
    Element { tag: tag.into(), ..el(id, ty) }
}

#[test]
fn next_tag_numbers_after_the_highest() {
    let scopes = vec![scope("f.step", &["f.Process", "f.Decision"], "S{n}")];
    let m = model(vec![], vec![]);
    let r = Registry::new(&m, &scopes);
    assert_eq!(r.next_tag(&el("", "f.Process")).unwrap(), Some("S1".into()));
    let m = model(
        vec![tagged("a", "f.Process", "S1"), tagged("b", "f.Decision", "S7"), tagged("c", "f.Process", "custom"), tagged("d", "f.Other", "S99")],
        vec![],
    );
    let r = Registry::new(&m, &scopes);
    assert_eq!(r.next_tag(&el("new", "f.Process")).unwrap(), Some("S8".into()));
    assert_eq!(r.next_tag(&el("new", "f.Other")).unwrap(), None);
}

#[test]
fn next_tag_ignores_the_element_itself() {
    let scopes = vec![scope("f.step", &["f.Process"], "S{n}")];
    let m = model(vec![tagged("a", "f.Process", "S3"), tagged("b", "f.Process", "S5")], vec![]);
    let r = Registry::new(&m, &scopes);
    assert_eq!(r.next_tag(m.element("b").unwrap()).unwrap(), Some("S4".into()));
    assert_eq!(r.next_tag(m.element("a").unwrap()).unwrap(), Some("S6".into()));
}

#[test]
fn next_tag_skips_taken_composed_tags() {
    // Each member's tag is read with its own properties: a ("X1", tag X12) reads as 2 and
    // b ("X", tag X3) as 3, so the highest number is 3.
    let scopes = vec![scope("s", &["t.A"], "{letters}{n}")];
    let a = Element { properties: [("letters".to_string(), PropValue::Text("X1".into()))].into(), ..tagged("a", "t.A", "X12") };
    let b = Element { properties: [("letters".to_string(), PropValue::Text("X".into()))].into(), ..tagged("b", "t.A", "X3") };
    let m = model(vec![a, b], vec![]);
    let r = Registry::new(&m, &scopes);
    let new = Element { properties: [("letters".to_string(), PropValue::Text("X1".into()))].into(), ..el("n", "t.A") };
    assert_eq!(r.next_tag(&new).unwrap(), Some("X14".into()));
    // An element with letters "X" would get X4.
    let new = Element { properties: [("letters".to_string(), PropValue::Text("X".into()))].into(), ..el("n", "t.A") };
    assert_eq!(r.next_tag(&new).unwrap(), Some("X4".into()));
    let c = tagged("c", "t.A", "X4");
    let m = model(vec![m.elements[0].clone(), m.elements[1].clone(), c], vec![]);
    let r = Registry::new(&m, &scopes);
    // c has no letters property, so its tag "X4" reads with empty letters as no match; X4 is
    // taken, so X5 follows.
    assert_eq!(r.next_tag(&new).unwrap(), Some("X5".into()));
}

#[test]
fn next_tag_shares_one_counter_across_composed_prefixes() {
    let scopes = vec![scope("isa", &["p.Instrument"], "{letters}-{n:3}")];
    let fic = Element { properties: [("letters".to_string(), PropValue::Text("FIC".into()))].into(), ..tagged("a", "p.Instrument", "FIC-101") };
    let m = model(vec![fic], vec![]);
    let r = Registry::new(&m, &scopes);
    let tic = Element { properties: [("letters".to_string(), PropValue::Text("TIC".into()))].into(), ..el("b", "p.Instrument") };
    assert_eq!(r.next_tag(&tic).unwrap(), Some("TIC-102".into()));
    let bare = el("c", "p.Instrument");
    assert_eq!(r.next_tag(&bare).unwrap(), Some("-102".into()));
}

#[test]
fn next_tag_none_without_auto_number_and_errors_on_bad_formats() {
    let mut s = scope("s", &["t.A"], "S{n}");
    s.auto_number = false;
    let scopes = vec![s];
    let m = model(vec![], vec![]);
    assert_eq!(Registry::new(&m, &scopes).next_tag(&el("", "t.A")).unwrap(), None);
    for bad in ["S{n", "{n}{n}", "S{bad part}"] {
        let scopes = vec![scope("s", &["t.A"], bad)];
        assert!(matches!(Registry::new(&m, &scopes).next_tag(&el("", "t.A")), Err(ModelError::Format(_))));
    }
    let scopes = vec![scope("s", &["t.A"], "FIXED")];
    assert!(matches!(Registry::new(&m, &scopes).next_tag(&el("", "t.A")), Err(ModelError::Format(_))));
}

#[test]
fn next_tag_does_not_overflow() {
    let scopes = vec![scope("s", &["t.A"], "S{n}")];
    let m = model(vec![tagged("a", "t.A", "S99999999999999999999")], vec![]);
    let r = Registry::new(&m, &scopes);
    assert!(matches!(r.next_tag(&el("b", "t.A")), Err(ModelError::TooMany(_))));
    let m = model(vec![tagged("a", "t.A", &format!("S{}", u64::MAX - 1))], vec![]);
    let r = Registry::new(&m, &scopes);
    assert_eq!(r.next_tag(&el("b", "t.A")).unwrap(), Some(format!("S{}", u64::MAX)));
}

#[test]
fn next_tag_uses_the_first_scope_listing_the_type() {
    let scopes = vec![scope("one", &["t.A"], "A{n}"), scope("two", &["t.A"], "B{n}")];
    let m = model(vec![], vec![]);
    assert_eq!(Registry::new(&m, &scopes).next_tag(&el("", "t.A")).unwrap(), Some("A1".into()));
}

#[test]
fn is_free_in_unique_and_paired_scopes() {
    let mut paired = scope("p", &["t.Ref"], "{n}");
    paired.paired = true;
    let mut loose = scope("l", &["t.Loose"], "{n}");
    loose.unique = false;
    let scopes = vec![scope("u", &["t.Step"], "S{n}"), paired, loose];
    let m = model(
        vec![
            tagged("a", "t.Step", "S1"),
            tagged("b", "t.Ref", "1"),
            tagged("c", "t.Ref", "2"),
            tagged("d", "t.Ref", "2"),
            tagged("e", "t.Loose", "x"),
        ],
        vec![],
    );
    let r = Registry::new(&m, &scopes);
    assert!(!r.is_free("t.Step", "S1", None));
    assert!(r.is_free("t.Step", "S1", Some("a")));
    assert!(r.is_free("t.Step", "S2", None));
    assert!(r.is_free("t.Step", "", None));
    assert!(r.is_free("t.Ref", "1", None));
    assert!(!r.is_free("t.Ref", "2", None));
    assert!(r.is_free("t.Ref", "2", Some("c")));
    assert!(r.is_free("t.Loose", "x", None));
    assert!(r.is_free("t.Unscoped", "S1", None));
}

#[test]
fn duplicates_and_unmatched_are_sorted_and_ignore_empty_tags() {
    let mut paired = scope("a.refs", &["t.Ref"], "{n}");
    paired.paired = true;
    let scopes = vec![scope("b.steps", &["t.Step"], "S{n}"), paired];
    let m = model(
        vec![
            tagged("s1", "t.Step", "S2"),
            tagged("s2", "t.Step", "S1"),
            tagged("s3", "t.Step", "S2"),
            tagged("s4", "t.Step", "S1"),
            tagged("s5", "t.Step", ""),
            tagged("s6", "t.Step", ""),
            tagged("s7", "t.Step", "S2"),
            tagged("r1", "t.Ref", "B"),
            tagged("r2", "t.Ref", "A"),
            tagged("r3", "t.Ref", "A"),
            tagged("r4", "t.Ref", "C"),
            tagged("r5", "t.Ref", "C"),
            tagged("r6", "t.Ref", "C"),
            tagged("r7", "t.Ref", ""),
        ],
        vec![],
    );
    let r = Registry::new(&m, &scopes);
    let d = r.duplicates();
    let summary: Vec<(&str, &str, Vec<&str>)> =
        d.iter().map(|x| (x.scope.as_str(), x.tag.as_str(), x.elements.iter().map(String::as_str).collect())).collect();
    assert_eq!(
        summary,
        vec![("a.refs", "C", vec!["r4", "r5", "r6"]), ("b.steps", "S1", vec!["s2", "s4"]), ("b.steps", "S2", vec!["s1", "s3", "s7"])]
    );
    let u = r.unmatched();
    assert_eq!(u, vec![Unmatched { scope: "a.refs".into(), tag: "B".into(), element: "r1".into() }]);
}

#[test]
fn registry_on_an_empty_model_and_no_scopes() {
    let m = model(vec![tagged("a", "t.A", "S1"), tagged("b", "t.A", "S1")], vec![]);
    let r = Registry::new(&m, &[]);
    assert!(r.duplicates().is_empty());
    assert!(r.unmatched().is_empty());
    assert!(r.is_free("t.A", "S1", None));
    assert_eq!(r.next_tag(&el("c", "t.A")).unwrap(), None);
}

#[test]
fn scope_loads_tolerantly_with_defaults() {
    let s: IdScope = serde_json::from_value(json!({"id": "x.s", "types": "nope", "unique": "yes"})).unwrap();
    assert_eq!(s.format, "{n}");
    assert!(s.types.is_empty());
    assert!(!s.unique);
    let s: IdScope = serde_json::from_value(json!({})).unwrap();
    assert_eq!(s, IdScope::default());
    assert!(s.unique);
}

// ---- property tests ----

fn small_id() -> impl Strategy<Value = String> {
    prop_oneof![Just(String::new()), "[a-e]", "[a-e][0-9]", Just("é".repeat(40))]
}

fn prop_value() -> impl Strategy<Value = PropValue> {
    prop_oneof![
        any::<bool>().prop_map(PropValue::Bool),
        any::<f64>().prop_map(PropValue::Number),
        Just(PropValue::Number(f64::NAN)),
        Just(PropValue::Number(f64::NEG_INFINITY)),
        ".{0,8}".prop_map(PropValue::Text),
    ]
}

fn element() -> impl Strategy<Value = Element> {
    (small_id(), ".{0,6}", ".{0,4}", proptest::option::of(small_id()), proptest::collection::btree_map(".{0,6}", prop_value(), 0..4))
        .prop_map(|(id, type_id, tag, owner, properties)| Element { id, type_id, name: String::new(), tag, owner, properties })
}

fn relationship() -> impl Strategy<Value = Relationship> {
    (small_id(), small_id(), small_id(), proptest::option::of(".{0,3}"))
        .prop_map(|(id, source, target, source_port)| Relationship { source_port, ..Relationship::new(id, "t.Flow", source, target) })
}

fn any_model() -> impl Strategy<Value = Model> {
    (
        0u32..3,
        proptest::collection::vec((small_id(), ".{0,3}").prop_map(|(id, version)| PackRef { id, version }), 0..4),
        proptest::collection::vec(element(), 0..24),
        proptest::collection::vec(relationship(), 0..24),
    )
        .prop_map(|(schema_version, packs, elements, relationships)| Model {
            schema_version,
            packs,
            elements,
            relationships,
            patterns: Vec::new(),
            nets: Vec::new(),
        })
}

proptest! {
    #[test]
    fn sanitize_is_idempotent_and_leaves_a_valid_model(mut m in any_model()) {
        m.sanitize();
        let mut again = m.clone();
        again.sanitize();
        prop_assert_eq!(&again, &m);

        let mut seen = std::collections::HashSet::new();
        for e in &m.elements {
            prop_assert!(!e.id.is_empty() && e.id.len() <= MAX_ID);
            prop_assert!(seen.insert(e.id.clone()));
            if let Some(o) = &e.owner {
                prop_assert!(m.element(o).is_some());
            }
            prop_assert!(!m.ancestors(&e.id).contains(&e.id));
            prop_assert!(e.properties.values().all(|v| v.as_f64().is_none_or(f64::is_finite)));
        }
        for r in &m.relationships {
            prop_assert!(seen.insert(r.id.clone()));
            prop_assert!(m.element(&r.source).is_some() && m.element(&r.target).is_some());
        }
        for e in &m.elements {
            // On a sanitized model the walks are exact: an element is never among its own
            // descendants and set_owner refuses exactly those.
            let d = m.descendants(&e.id);
            prop_assert!(!d.contains(&e.id));
            for x in &d {
                let mut probe = m.clone();
                prop_assert_eq!(probe.set_owner(&e.id, Some(x)), Err(ModelError::Cycle(e.id.clone())));
            }
        }
    }

    #[test]
    fn remove_leaves_a_sanitized_model(mut m in any_model(), pick in proptest::collection::vec(small_id(), 0..4)) {
        m.sanitize();
        let removed = m.remove(&pick);
        let mut again = m.clone();
        again.sanitize();
        prop_assert_eq!(&again, &m);
        for id in &removed.elements {
            prop_assert!(!m.contains_id(id));
        }
    }

    #[test]
    fn junk_json_never_panics(text in ".{0,64}") {
        if let Ok(mut m) = serde_json::from_str::<Model>(&text) {
            m.sanitize();
        }
    }

    #[test]
    fn formats_never_panic(fmt in "[{}n:0-9a-zS_-]{0,12}", tag in "[{}nS0-9a-z-]{0,12}") {
        if let Ok(f) = IdFormat::parse(&fmt) {
            let e = with_props("t.T", &[("a", PropValue::Text("x".into()))]);
            let _ = f.number_in(&tag, &e);
            let composed = f.compose(&e, 42);
            if f.has_number() {
                prop_assert_eq!(f.number_in(&composed, &e), Some(42));
            }
        }
    }
}

#[test]
fn a_model_with_only_nets_or_patterns_is_not_empty() {
    // Regression: `is_empty` once looked at packs, elements and relationships only, so saving a
    // document that held nothing but wiring (or pattern instances) dropped its model section.
    let mut m = Model::new();
    assert!(m.is_empty());
    m.nets.push(crate::Net::new("n1", crate::NetKind::Wire));
    assert!(!m.is_empty());
    let mut m = Model::new();
    m.patterns.push(crate::PatternInstance { id: "p1".into(), ..crate::PatternInstance::default() });
    assert!(!m.is_empty());
}

#[test]
fn nets_share_the_id_space_and_sanitise() {
    let mut m = Model::new();
    m.add_element(Element::new("e1", "flowchart.Process")).unwrap();
    for id in ["n1", "n1", "", "e1", &"x".repeat(200)] {
        m.nets.push(crate::Net { name: "y".repeat(1000), ..crate::Net::new(id, crate::NetKind::Pipe) });
    }
    m.sanitize();
    let ids: Vec<&str> = m.nets.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["n1", &"x".repeat(crate::MAX_ID)]);
    assert!(m.nets.iter().all(|n| n.name.len() <= crate::MAX_TAG));
    assert!(m.contains_id("n1") && m.net("n1").is_some());
    let again = m.clone();
    m.sanitize();
    assert_eq!(m, again, "sanitize is idempotent");
}

#[test]
fn number_by_counts_and_checks_each_property_value_separately() {
    let mut s = scope("e.device", &["e.Coil", "e.Fuse"], "{code}{n}");
    s.number_by = "code".into();
    let scopes = vec![s];
    let coded = |id: &str, ty: &str, code: &str, tag: &str| Element {
        properties: [("code".to_string(), PropValue::Text(code.into()))].into(),
        ..tagged(id, ty, tag)
    };
    let m = model(vec![coded("a", "e.Coil", "K", "K1"), coded("b", "e.Coil", "K", "K2"), coded("c", "e.Fuse", "F", "F1")], vec![]);
    let r = Registry::new(&m, &scopes);
    assert_eq!(r.next_tag(&coded("", "e.Coil", "K", "")).unwrap(), Some("K3".into()));
    assert_eq!(r.next_tag(&coded("", "e.Fuse", "F", "")).unwrap(), Some("F2".into()));
    assert_eq!(r.next_tag(&coded("", "e.Fuse", "Q", "")).unwrap(), Some("Q1".into()));
    assert!(r.duplicates().is_empty());
    assert!(!r.is_free("e.Coil", "K1", Some("b")));
    // The same tag under two values is two groups (a property edit away from a clash, not one).
    let m = model(vec![coded("a", "e.Coil", "K", "X1"), coded("b", "e.Coil", "F", "X1"), coded("c", "e.Coil", "K", "X1")], vec![]);
    let d = Registry::new(&m, &scopes).duplicates();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].elements, vec!["a".to_string(), "c".to_string()]);
    // The key is camelCase on disk and left out when empty.
    let v = serde_json::to_value(&scopes[0]).unwrap();
    assert_eq!(v["numberBy"], "code");
    assert!(serde_json::to_value(IdScope::default()).unwrap().get("numberBy").is_none());
}
