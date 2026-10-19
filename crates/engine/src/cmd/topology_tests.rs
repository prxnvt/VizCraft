//! Tests of `topology.toMermaid`: a drawn topology reads back through `file.import` with its
//! types once merged.

use serde_json::{Value, json};
use vizcraft_diagram::Document;

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn a_drawn_topology_becomes_architecture_text() {
    let mut s = Session::new(Document::new());
    let net = run(&mut s, "element.create", json!({"type": "cloud.VirtualNetwork", "name": "Main", "x": 0, "y": 0, "w": 600, "h": 300}));
    let app = run(&mut s, "element.create", json!({"type": "cloud.Compute", "name": "App", "x": 60, "y": 80}));
    let db = run(&mut s, "element.create", json!({"type": "cloud.Database", "name": "Orders", "x": 300, "y": 80}));
    run(&mut s, "container.addMember", json!({"container": net["shape"], "ids": [app["shape"], db["shape"]]}));
    let r = run(&mut s, "relationship.create", json!({"from": app["id"], "to": db["id"], "name": "query"}));
    run(&mut s, "connector.setStep", json!({"id": r["id"], "step": 1}));
    let out = run(&mut s, "topology.toMermaid", json!({}));
    let text = out["text"].as_str().unwrap();
    assert!(text.starts_with("architecture-beta\n"), "{text}");
    assert!(text.contains("%% type: ") && text.contains("cloud.VirtualNetwork"), "{text}");
    assert!(text.contains("service ") && text.contains("[Orders] in "), "{text}");
    assert_eq!(out["warnings"].as_array().unwrap().len(), 2, "the label and the step");
    assert!(s.run("topology.toMermaid", &json!({"page": "nope"})).is_err());
    // The text reads back as cloud shapes in a container.
    let mut back = Session::new(Document::new());
    let page = vizcraft_formats::mermaid::import(text).unwrap().pages.remove(0);
    assert!(page.shapes.iter().any(|sh| sh.stencil == "cloud.database"));
    assert!(page.shapes.iter().any(|sh| sh.container.is_some() && sh.stencil == "cloud.virtualNetwork"));
    back.doc.pages = vec![page];
    assert!(run(&mut back, "topology.toMermaid", json!({}))["text"].as_str().unwrap().contains("[Orders]"));
}
