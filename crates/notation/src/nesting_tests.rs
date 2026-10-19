//! Tests of the nesting data checks, sync step 9 and the nesting, label and port checks.

use serde_json::{Value, json};
use vizcraft_diagram::container::Container;
use vizcraft_diagram::{Document, Shape, ShapeKind};
use vizcraft_model::{Element, Relationship, Severity};

use super::*;
use crate::loader::{NotationError, load_json};

/// A small nesting pack: a Site and a Rack (containers), a Zone (container), a Device that sits
/// in a Rack, a Site or at the top, and a Cable relationship between device ports.
fn pack_json() -> Value {
    json!({
        "id": "nt", "version": "1.0.0", "name": "Nesting test",
        "glyphs": [
            {"id": "nt.site", "outline": "rectangle", "w": 400, "h": 300, "fill": "#ffffff", "line": "#000000", "elementType": "nt.Site",
             "container": {"dash": "dashed", "corner": 8, "heading": "line", "band": "#eeeeee", "margin": 12}},
            {"id": "nt.rack", "outline": "rectangle", "w": 200, "h": 200, "fill": "#ffffff", "line": "#000000", "elementType": "nt.Rack", "container": {}},
            {"id": "nt.zone", "outline": "rectangle", "w": 200, "h": 200, "fill": "#ffffff", "line": "#000000", "elementType": "nt.Zone", "container": {"heading": "plain"}},
            {"id": "nt.device", "outline": "rectangle", "w": 60, "h": 40, "fill": "#ffffff", "line": "#000000", "elementType": "nt.Device"}
        ],
        "elementTypes": [
            {"id": "nt.Site", "glyph": "nt.site", "container": true},
            {"id": "nt.Rack", "glyph": "nt.rack", "container": true, "allowedParents": ["nt.Site", "top"]},
            {"id": "nt.Zone", "glyph": "nt.zone", "container": true},
            {"id": "nt.Device", "glyph": "nt.device", "allowedParents": ["nt.Rack", "nt.Site", "top"],
             "ports": [{"id": "p1", "x": 0, "y": 0.5}, {"id": "p2", "x": 1, "y": 0.5}]},
            {"id": "nt.Router", "glyph": "nt.device", "extends": "nt.Device"}
        ],
        "relationshipTypes": [{"id": "nt.Cable"}],
        "connections": [{"source": ["nt.Device"], "relationship": "nt.Cable", "target": ["nt.Device"]}],
        "variants": [{"id": "nt.alt", "glyphs": {"nt.Device": "nt.site"}}],
        "rules": [
            {"id": "nt.nesting", "check": {"kind": "nestingAllowed", "drawn": true}},
            {"id": "nt.labelled", "check": {"kind": "labelled"}},
            {"id": "nt.ports", "check": {"kind": "endsOnPorts", "relationship": "nt.Cable"}}
        ]
    })
}

fn cat() -> Catalog {
    let p = load_json(&pack_json().to_string()).unwrap();
    Catalog::builtin().with_pack(p).unwrap()
}

fn problems(edit: &dyn Fn(&mut Value)) -> String {
    let mut v = pack_json();
    edit(&mut v);
    match load_json(&v.to_string()) {
        Err(NotationError::Invalid(p)) => p.join("; "),
        other => panic!("{other:?}"),
    }
}

#[test]
fn check_pack_rejects_bad_parents_looks_and_container_variants() {
    assert!(problems(&|v| v["elementTypes"][3]["allowedParents"] = json!(["nt.Device"])).contains("not a container type"));
    assert!(problems(&|v| v["elementTypes"][3]["allowedParents"] = json!(["Rack"])).contains("is not `<pack>.<Type>`"));
    assert!(problems(&|v| v["glyphs"][0]["container"]["dash"] = json!("wavy")).contains("unknown container dash"));
    assert!(problems(&|v| v["glyphs"][0]["container"]["heading"] = json!("fancy")).contains("heading must be"));
    assert!(problems(&|v| v["glyphs"][0]["container"]["band"] = json!("red")).contains("not a colour"));
    assert!(problems(&|v| v["glyphs"][0]["container"]["corner"] = json!(-1)).contains("corner and margin"));
    assert!(problems(&|v| v["variants"][0]["glyphs"] = json!({"nt.Rack": "nt.site"})).contains("maps container type `nt.Rack`"));
    assert!(problems(&|v| v["rules"][2]["check"]["relationship"] = json!("nt.Ghost")).contains("unknown relationship type"));
    // Another pack's types are not checked here, and the good pack loads.
    let mut v = pack_json();
    v["elementTypes"][3]["allowedParents"] = json!(["other.Thing", "*"]);
    assert!(load_json(&v.to_string()).is_ok());
}

#[test]
fn a_look_becomes_container_settings_that_keep_margin_fit_and_lock() {
    let p = load_json(&pack_json().to_string()).unwrap();
    let look = p.glyph("nt.site").unwrap().container.clone().unwrap();
    assert_eq!(look.dash(), Dash::Dashed);
    let c = look.container(None);
    assert_eq!((c.heading_style, c.margin), (HeadingStyle::Line, 12.0));
    let keep = Container { margin: 3.0, auto_fit: true, locked: true, ..Container::default() };
    let c = look.container(Some(&keep));
    assert_eq!((c.margin, c.auto_fit, c.locked), (3.0, true, true));
}

fn bound(id: &str, element: &str, x: f64, container: bool) -> Shape {
    let mut s = Shape::new(id, ShapeKind::Rectangle, x, 0.0, 100.0, 100.0);
    s.element = Some(element.into());
    if container {
        s.container = Some(Container::default());
        s.behavior.container = true;
    }
    s
}

/// A rack `r` drawn as container `s_r`, a zone `z` as `s_z`, and device `d` drawn at the top.
fn doc() -> Document {
    let mut d = Document::new();
    for (id, ty) in [("r", "nt.Rack"), ("z", "nt.Zone"), ("d", "nt.Router")] {
        let mut e = Element::new(id, ty);
        e.name = id.to_uppercase();
        d.model.add_element(e).unwrap();
    }
    let page = &mut d.pages[0];
    page.shapes.push(bound("s_r", "r", 0.0, true));
    page.shapes.push(bound("s_z", "z", 300.0, true));
    page.shapes.push(bound("s_d", "d", 600.0, false));
    d
}

fn sync9(doc: &mut Document, before: &Document, cat: &Catalog) {
    owners(doc, before, cat);
    let once = doc.clone();
    owners(doc, before, cat);
    assert_eq!(*doc, once, "step 9 is idempotent");
}

#[test]
fn step_9_sets_keeps_and_clears_owners() {
    let cat = cat();
    let before = doc();
    // Into the rack: owned by it (the Router inherits the Device's list).
    let mut d = before.clone();
    d.pages[0].shapes[2].parent = Some("s_r".into());
    sync9(&mut d, &before, &cat);
    assert_eq!(d.model.element("d").unwrap().owner.as_deref(), Some("r"));
    // Into the zone, which the type does not allow: the rack owner stays.
    let before = d.clone();
    let mut d2 = d.clone();
    d2.pages[0].shapes[2].parent = Some("s_z".into());
    sync9(&mut d2, &before, &cat);
    assert_eq!(d2.model.element("d").unwrap().owner.as_deref(), Some("r"));
    // Out of the rack to the top: the owner the rack gave is cleared.
    let mut d3 = d.clone();
    d3.pages[0].shapes[2].parent = None;
    sync9(&mut d3, &before, &cat);
    assert_eq!(d3.model.element("d").unwrap().owner, None);
    // Unchanged shapes are left alone even when the model disagrees.
    let mut d4 = before.clone();
    d4.model.set_owner("d", None).unwrap();
    let snapshot = d4.clone();
    sync9(&mut d4, &snapshot, &cat);
    assert_eq!(d4, snapshot);
}

#[test]
fn step_9_leaves_documents_without_nesting_types_identical() {
    let cat = cat();
    let mut d = crate::rules::test_doc::Doc::new();
    d.el("a", "Process", "A").el("b", "Process", "B");
    let mut doc = d.doc.clone();
    let mut parented = doc.clone();
    parented.pages[0].shapes[1].parent = Some(parented.pages[0].shapes[0].id.clone());
    let before = doc.clone();
    owners(&mut doc, &Document::new(), &cat);
    assert_eq!(doc, before);
    let snap = parented.clone();
    owners(&mut parented, &before, &cat);
    assert_eq!(parented, snap);
}

fn run(doc: &Document, cat: &Catalog, rule: &str) -> Vec<Issue> {
    let pack = cat.pack("nt").unwrap();
    let r = pack.rule(rule).unwrap();
    let mut out = Vec::new();
    check(doc, cat, pack, r, &mut out);
    out
}

#[test]
fn a_device_drawn_in_a_zone_keeps_its_rack_owner_and_only_the_drawn_check_notices() {
    let cat = cat();
    let mut d = doc();
    d.model.set_owner("d", Some("r")).unwrap();
    assert!(run(&d, &cat, "nt.nesting").is_empty());
    let before = d.clone();
    d.pages[0].shapes[2].parent = Some("s_z".into());
    owners(&mut d, &before, &cat);
    assert_eq!(d.model.element("d").unwrap().owner.as_deref(), Some("r"));
    let found = run(&d, &cat, "nt.nesting");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].message.contains("is drawn inside Zone \"Z\""), "{}", found[0].message);
    assert_eq!(found[0].items, ["s_d"]);
    // A device owned by a zone breaks the model rule.
    d.model.element_mut("d").unwrap().owner = Some("z".into());
    assert!(run(&d, &cat, "nt.nesting").iter().any(|i| i.message.contains("may not sit in Zone")));
}

#[test]
fn labels_and_port_ends_are_checked() {
    let cat = cat();
    let mut d = doc();
    d.model.element_mut("r").unwrap().name.clear();
    d.model.add_element(Element::new("d2", "nt.Device")).unwrap();
    let mut cable = Relationship::new("c", "nt.Cable", "d", "d2");
    cable.source_port = Some("p1".into());
    d.model.add_relationship(cable).unwrap();
    let labels = run(&d, &cat, "nt.labelled");
    assert_eq!(labels.len(), 3, "{labels:?}");
    assert!(labels.iter().any(|i| i.message.contains("has no label")));
    let ports = run(&d, &cat, "nt.ports");
    assert_eq!(ports.len(), 1, "{ports:?}");
    assert!(ports[0].message.contains("on no port"));
    assert_eq!(ports[0].severity, Severity::Warning);
}
