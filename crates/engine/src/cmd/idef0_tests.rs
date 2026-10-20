use serde_json::json;

use crate::{Document, Session};

fn open() -> Session {
    let mut s = Session::new(Document::new());
    s.run("example.open", &json!({"example": "idef0.makeWidget"})).unwrap();
    s
}

#[test]
fn the_example_has_a_context_and_an_a0_page() {
    let mut s = open();
    let r = s.run("idef0.nodes", &json!({})).unwrap();
    let nodes: Vec<&str> = r["nodes"].as_array().unwrap().iter().map(|n| n["node"].as_str().unwrap()).collect();
    assert_eq!(nodes, ["A-0", "A0"]);
    assert_eq!(r["nodes"][1]["boxes"].as_array().unwrap().len(), 3);
}

#[test]
fn decomposing_box_two_makes_page_a2_with_four_codes_in_one_step() {
    let mut s = open();
    let a0 = s.doc.pages.iter().position(|p| p.ext.get("idef0.node").map(String::as_str) == Some("A0")).unwrap();
    s.doc.set_active_index(a0);
    let fg = |s: &Session| s.doc.pages.iter().filter(|p| !p.is_background).count();
    let pages = fg(&s);
    let b2 = s.doc.model.elements.iter().find(|e| e.name == "Cut parts").unwrap().id.clone();
    let r = s.run("idef0.decompose", &json!({"id": b2})).unwrap();
    assert_eq!(r["node"], json!("A2"));
    let codes: Vec<&str> = r["boundaries"].as_array().unwrap().iter().map(|b| b["code"].as_str().unwrap()).collect();
    assert_eq!(codes, ["C1", "I1", "M1", "O1"]);
    assert_eq!(fg(&s), pages + 1);
    assert_eq!(s.doc.page().unwrap().name, "A2");
    assert!(s.doc.pages[a0].shapes.iter().any(|sh| sh.links.iter().any(|l| l.target == r["page"].as_str().unwrap())), "the box links to its diagram");
    assert!(s.run("idef0.decompose", &json!({"id": b2})).is_err(), "decomposed once");
    let names = |s: &Session| s.doc.pages.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(fg(&s), pages, "{:?}", names(&s));
}

#[test]
fn renumber_and_icom_restore_numbers_and_codes() {
    let mut s = open();
    let a0 = s.doc.pages.iter().position(|p| p.ext.get("idef0.node").map(String::as_str) == Some("A0")).unwrap();
    let page = s.doc.pages[a0].id.clone();
    let b3 = s.doc.model.elements.iter().find(|e| e.name == "Inspect parts").unwrap().id.clone();
    s.run("element.setProperty", &json!({"id": b3, "key": "number", "value": "7"})).unwrap();
    assert!(s.issues.iter().any(|i| i.rule == "idef0.boxNumbers"));
    s.run("idef0.renumber", &json!({"page": page})).unwrap();
    assert!(!s.issues.iter().any(|i| i.rule == "idef0.boxNumbers"), "{:?}", s.issues);
    let i1 = s.doc.model.elements.iter().find(|e| e.name == "I1").unwrap().id.clone();
    s.run("element.rename", &json!({"id": i1, "name": "I4"})).unwrap();
    assert!(s.issues.iter().any(|i| i.rule == "idef0.icomCodes"));
    s.run("idef0.icom", &json!({"page": page})).unwrap();
    assert!(!s.issues.iter().any(|i| i.rule == "idef0.icomCodes"), "{:?}", s.issues);
}
