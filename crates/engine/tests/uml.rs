//! The pack for OMG UML end to end: member rows, keyword lines and end labels kept in step by the
//! sync pass, the merging Mermaid import of class and state diagrams, and every UML template
//! written as PlantUML against its golden file in `crates/formats/testdata/uml/` (refresh with
//! `VIZCRAFT_UPDATE_GOLDEN=1` and review the diff).

use serde_json::{Value, json};
use vizcraft_engine::{Document, Session};

fn blank() -> Session {
    Session::new(Document::new())
}

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn testdata(name: &str) -> String {
    format!("{}/../formats/testdata/uml/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// A file path in the system temp folder unique to this test run.
fn temp(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("vizcraft-uml-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().to_string()
}

fn shape_of<'a>(s: &'a Session, element: &str) -> &'a vizcraft_diagram::Shape {
    s.doc.pages.iter().flat_map(|p| p.shapes.iter()).find(|x| x.element.as_deref() == Some(element)).unwrap()
}

fn class(s: &mut Session, name: &str, props: Value, x: f64) -> String {
    let r = run(s, "element.create", json!({"type": "uml.Class", "name": name, "properties": props, "x": x, "y": 0}));
    r["id"].as_str().unwrap().to_string()
}

#[test]
fn member_rows_follow_the_attributes_and_the_variant() {
    let mut s = blank();
    let id = class(&mut s, "Order", json!({"attributes": "+name: String\n-items: Item [0..*]", "operations": "+total(): Money"}), 0.0);
    let rows = |s: &Session, c: &str| shape_of(s, &id).compartments.iter().find(|x| x.id == c).map(|x| x.rows.clone()).unwrap();
    assert_eq!(rows(&s, "attrs"), ["+name: String", "-items: Item [0..*]"]);
    assert_eq!(rows(&s, "ops"), ["+total(): Money"]);
    run(&mut s, "variant.set", json!({"variant": "uml.keywords"}));
    assert_eq!(rows(&s, "attrs"), ["public name: String", "private items: Item [0..*]"]);
    run(&mut s, "variant.set", json!({"variant": "uml.nameOnly"}));
    assert!(rows(&s, "attrs").is_empty());
    run(&mut s, "variant.set", json!({"variant": ""}));
    run(&mut s, "element.setProperty", json!({"id": id, "key": "attributes", "value": "#count: Integer = 0"}));
    assert_eq!(rows(&s, "attrs"), ["#count: Integer = 0"]);
}

#[test]
fn keyword_lines_show_and_come_off_the_text() {
    let mut s = blank();
    let r = run(&mut s, "element.create", json!({"type": "uml.Interface", "name": "Payable", "x": 0, "y": 0}));
    let id = r["id"].as_str().unwrap().to_string();
    let shape = shape_of(&s, &id).id.clone();
    assert_eq!(shape_of(&s, &id).text, "\u{ab}interface\u{bb}\nPayable");
    run(&mut s, "shape.setText", json!({"id": shape, "text": "\u{ab}interface\u{bb}\nBillable"}));
    assert_eq!(s.doc.model.element(&id).unwrap().name, "Billable", "editing the name line renames the element");
    assert_eq!(shape_of(&s, &id).text, "\u{ab}interface\u{bb}\nBillable");
    run(&mut s, "element.setProperty", json!({"id": id, "key": "stereotype", "value": "service"}));
    assert_eq!(shape_of(&s, &id).text, "\u{ab}service\u{bb}\nBillable", "a stereotype wins over the keyword");
}

#[test]
fn abstract_classes_turn_italic() {
    let mut s = blank();
    let id = class(&mut s, "Shape", json!({}), 0.0);
    assert!(!shape_of(&s, &id).style.italic);
    run(&mut s, "element.setProperty", json!({"id": id, "key": "isAbstract", "value": true}));
    assert!(shape_of(&s, &id).style.italic);
    run(&mut s, "element.setProperty", json!({"id": id, "key": "isAbstract", "value": false}));
    assert!(!shape_of(&s, &id).style.italic);
}

#[test]
fn multiplicities_are_end_labels_and_edited_labels_write_back() {
    let mut s = blank();
    let a = class(&mut s, "Customer", json!({}), 0.0);
    let b = class(&mut s, "Order", json!({}), 400.0);
    let r = run(
        &mut s,
        "relationship.create",
        json!({"from": a, "to": b, "type": "uml.Association", "properties": {"sourceMultiplicity": "1", "targetRole": "orders", "targetMultiplicity": "0..*"}}),
    );
    let (rid, cid) = (r["id"].as_str().unwrap().to_string(), r["connector"].as_str().unwrap().to_string());
    let conn = |s: &Session| s.doc.page().unwrap().connector(&cid).unwrap().clone();
    assert_eq!((conn(&s).start_label.as_str(), conn(&s).end_label.as_str()), ("1", "orders 0..*"));
    run(&mut s, "connector.setEndLabels", json!({"ids": [cid], "end": "lines 1..*"}));
    let rel = s.doc.model.relationship(&rid).unwrap();
    assert_eq!(
        (rel.prop("targetRole").and_then(|v| v.as_str()), rel.prop("targetMultiplicity").and_then(|v| v.as_str())),
        (Some("lines"), Some("1..*"))
    );
    assert_eq!(conn(&s).end_label, "lines 1..*");
    run(&mut s, "relationship.setProperty", json!({"id": rid, "key": "sourceMultiplicity", "value": "0..1"})).to_string();
    assert_eq!(conn(&s).start_label, "0..1");
    // The composition is drawn with its diamond at the whole.
    let r = run(&mut s, "relationship.create", json!({"from": b, "to": a, "type": "uml.Composition"}));
    let c = s.doc.page().unwrap().connector(r["connector"].as_str().unwrap()).unwrap();
    assert_eq!((c.start_arrow.name(), c.end_arrow.name()), ("diamond", "none"));
}

const STATES: &str = "stateDiagram-v2\n  state \"Running\" as run {\n    [*] --> fetch\n    fetch --> decode\n    --\n    [*] --> watch\n  }\n  [*] --> idle\n  idle --> run : go\n";

fn import(s: &mut Session, text: &str, source: &str) -> Value {
    run(s, "file.import", json!({"format": "mermaid", "text": text, "merge": true, "source": source}))
}

fn element_named<'a>(s: &'a Session, name: &str) -> &'a vizcraft_model::Element {
    s.doc.model.elements.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no element named {name}"))
}

#[test]
fn a_merging_class_import_builds_typed_elements_owners_and_relationships() {
    let mut s = blank();
    let text = std::fs::read_to_string(testdata("class.mmd")).unwrap();
    let r = import(&mut s, &text, "library.mmd");
    assert!(r["added"].as_u64().unwrap() >= 7, "{r}");
    let item = element_named(&s, "Item");
    assert_eq!(item.type_id, "uml.Class");
    assert_eq!(item.prop("attributes").and_then(|v| v.as_str()), Some("+title: String\n#copies: Integer"));
    assert_eq!(item.prop("isAbstract").and_then(|v| v.as_bool()), Some(true), "typed by the schema, not text");
    let member = element_named(&s, "Member");
    assert_eq!(member.owner.as_deref().and_then(|o| s.doc.model.element(o)).map(|o| o.type_id.as_str()), Some("uml.Package"));
    let rel = |t: &str| s.doc.model.relationships.iter().find(|r| r.type_id == t).unwrap_or_else(|| panic!("no {t}"));
    let g = rel("uml.Generalization");
    assert_eq!((s.doc.model.element(&g.source).unwrap().name.as_str(), s.doc.model.element(&g.target).unwrap().name.as_str()), ("Book", "Item"));
    rel("uml.Composition");
    let borrows = s.doc.model.relationships.iter().find(|r| r.name == "borrows").unwrap();
    assert_eq!(borrows.prop("targetMultiplicity").and_then(|v| v.as_str()), Some("0..*"));
    // The sync pass shows the multiplicities at the ends, and no rule finds an error.
    let c = s.doc.pages[0].connectors.iter().find(|c| c.relationship.as_deref() == Some(borrows.id.as_str())).unwrap();
    assert_eq!((c.start_label.as_str(), c.end_label.as_str()), ("1", "0..*"));
    let issues = run(&mut s, "validate.run", json!({}));
    let errors: Vec<&Value> = issues["issues"].as_array().unwrap().iter().filter(|i| i["severity"] == "error").collect();
    assert!(errors.is_empty(), "{errors:?}");
    // One undo step takes the whole import away.
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc.model.elements.is_empty() && s.doc.pages[0].shapes.is_empty());
}

#[test]
fn a_merging_state_import_owns_states_by_region_and_reimport_keeps_ids_and_positions() {
    let mut s = blank();
    import(&mut s, STATES, "machine.mmd");
    let fetch = element_named(&s, "fetch").clone();
    let owner = s.doc.model.element(fetch.owner.as_deref().unwrap()).unwrap();
    assert_eq!(owner.type_id, "uml.Region");
    assert_eq!(s.doc.model.element(owner.owner.as_deref().unwrap()).unwrap().name, "Running");
    let initials = s.doc.model.elements.iter().filter(|e| e.type_id == "uml.Initial").count();
    assert_eq!(initials, 3, "one at the top and one per region");
    let issues = run(&mut s, "validate.run", json!({}));
    assert!(!issues["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "uml.oneInitial"), "{issues}");
    let at = |s: &Session, id: &str| {
        let sh = shape_of(s, id);
        (sh.id.clone(), sh.x, sh.y)
    };
    let before = at(&s, &fetch.id);
    let edited = STATES.replace("idle --> run : go", "idle --> run : go\n  run --> idle : halt");
    let r = import(&mut s, &edited, "machine.mmd");
    assert_eq!(r["added"], 0, "{r}");
    assert_eq!(at(&s, &fetch.id), before, "the same element keeps its shape where it was");
    assert!(s.doc.model.relationships.iter().any(|r| r.name == "halt" && r.type_id == "uml.Transition"));
}

#[test]
fn a_fork_used_as_a_join_and_a_join_used_as_a_fork_are_reported_at_info_level() {
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "uml.activity"}));
    let issues = run(&mut s, "validate.run", json!({}));
    let has = |v: &Value, rule: &str| v["issues"].as_array().unwrap().iter().any(|i| i["rule"] == rule);
    assert!(!has(&issues, "uml.forkUsedAsJoin") && !has(&issues, "uml.joinUsedAsFork"), "{issues}");
    let find = |s: &Session, t: &str| s.doc.model.elements.iter().find(|e| e.type_id == t).unwrap().id.clone();
    let (fork, join) = (find(&s, "uml.ForkNode"), find(&s, "uml.JoinNode"));
    let a = run(&mut s, "element.create", json!({"type": "uml.Action", "name": "Extra", "x": 0, "y": 0}))["id"].as_str().unwrap().to_string();
    // A second flow into the fork gives it more in than out only once it also loses outputs, so
    // add inputs until it does; likewise outputs for the join.
    for _ in 0..2 {
        run(&mut s, "relationship.create", json!({"from": a, "to": fork, "type": "uml.ControlFlow"}));
        run(&mut s, "relationship.create", json!({"from": join, "to": a, "type": "uml.ControlFlow"}));
    }
    let issues = run(&mut s, "validate.run", json!({}));
    for rule in ["uml.forkUsedAsJoin", "uml.joinUsedAsFork"] {
        let i = issues["issues"].as_array().unwrap().iter().find(|i| i["rule"] == rule).unwrap_or_else(|| panic!("{rule}: {issues}"));
        assert_eq!(i["severity"], "info");
    }
}

const TEMPLATES: [&str; 7] = ["uml.class", "uml.stateMachine", "uml.activity", "uml.useCase", "uml.component", "uml.deployment", "uml.package"];

#[test]
fn every_uml_template_exports_to_its_plantuml_golden() {
    for t in TEMPLATES {
        let mut s = blank();
        run(&mut s, "template.new", json!({"template": t}));
        let path = temp(&format!("{t}.puml"));
        let r = run(&mut s, "file.export", json!({"path": path}));
        assert_eq!(r["format"], "plantuml");
        assert!(r["warnings"].as_array().unwrap().is_empty(), "{t}: {r}");
        let text = std::fs::read_to_string(&path).unwrap();
        let golden = testdata(&format!("{}.puml", t.trim_start_matches("uml.")));
        if std::env::var_os("VIZCRAFT_UPDATE_GOLDEN").is_some() {
            std::fs::write(&golden, &text).unwrap();
        }
        let want = std::fs::read_to_string(&golden).unwrap_or_else(|_| panic!("{golden} exists (run with VIZCRAFT_UPDATE_GOLDEN=1)"));
        assert_eq!(text, want, "{t}");
    }
}

#[test]
fn mermaid_export_writes_class_and_state_pages_and_refuses_others() {
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "uml.class"}));
    let path = temp("class.mmd");
    run(&mut s, "file.export", json!({"path": path}));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("classDiagram\n") && text.contains("<|--") && text.contains("*--"), "{text}");
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "uml.stateMachine"}));
    let path = temp("state.mermaid");
    run(&mut s, "file.export", json!({"path": path, "format": "mermaid"}));
    assert!(std::fs::read_to_string(&path).unwrap().starts_with("stateDiagram-v2\n"));
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "uml.activity"}));
    assert!(s.run("file.export", &json!({"path": temp("activity.mmd")})).is_err());
}
