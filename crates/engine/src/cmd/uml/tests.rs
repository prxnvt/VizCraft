//! The `uml.*` member commands on the class diagram template.

use serde_json::{Value, json};
use vizcraft_diagram::Document;

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn classes() -> Session {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "uml.classDiagram"}));
    s
}

/// The id of the first element of type `t`.
fn first_of(s: &Session, t: &str) -> String {
    s.doc.model.elements.iter().find(|e| e.type_id == t).map(|e| e.id.clone()).unwrap_or_else(|| panic!("no {t}"))
}

fn rows(r: &Value) -> Vec<String> {
    r["rows"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
}

#[test]
fn add_picks_the_list_from_the_text_and_stores_the_canonical_row() {
    let mut s = classes();
    let c = first_of(&s, "uml.Class");
    let a = run(&mut s, "uml.addMember", json!({"element": c, "text": "-  total :  Money [0..1]"}));
    assert_eq!(a["list"], "attributes");
    assert!(rows(&a).contains(&"-total: Money [0..1]".to_string()), "{a}");
    assert!(a["problems"].as_array().unwrap().is_empty());
    let o = run(&mut s, "uml.addMember", json!({"element": c, "text": "+ pay(amount: Money): bool"}));
    assert_eq!(o["list"], "operations", "a row with a parameter list is an operation");
    // The model property holds the rows, one per line, and the shape draws them.
    let prop = s.doc.model.element(&c).unwrap().prop("attributes").unwrap().display();
    assert!(prop.lines().any(|l| l == "-total: Money [0..1]"), "{prop}");
    let shape = s.doc.page().unwrap().shapes.iter().find(|x| x.element.as_deref() == Some(c.as_str())).unwrap();
    assert!(shape.compartments.iter().any(|k| k.rows.iter().any(|r| r.contains("total"))), "the compartment shows it after the sync");
}

#[test]
fn set_move_and_remove_edit_one_row_each_in_one_undo_step() {
    let mut s = classes();
    let c = first_of(&s, "uml.Class");
    run(&mut s, "uml.setMember", json!({"element": c, "list": "attributes", "index": 0, "text": "# first: int"}).clone());
    let before = rows(&run(&mut s, "uml.addMember", json!({"element": c, "list": "attributes", "text": "+ second: int"})));
    let last = before.len() - 1;
    let m = run(&mut s, "uml.moveMember", json!({"element": c, "list": "attributes", "from": last, "to": 0}));
    assert_eq!(rows(&m)[0], "+second: int");
    assert_eq!(s.undo_label(), Some("Move Member"));
    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(rows(&run(&mut s, "uml.removeMember", json!({"element": c, "list": "attributes", "index": 0}))).len(), before.len() - 1);
    s.run("edit.undo", &json!({})).unwrap();
    let again = s.doc.model.element(&c).unwrap().prop("attributes").unwrap().display();
    assert_eq!(again.lines().count(), before.len(), "undo put the row back");
    // Out-of-range moves clamp; bad indexes are refused.
    run(&mut s, "uml.moveMember", json!({"element": c, "list": "attributes", "from": 0, "to": 9999}));
    assert!(s.run("uml.removeMember", &json!({"element": c, "list": "attributes", "index": 9999})).is_err());
    assert!(s.run("uml.setMember", &json!({"element": c, "list": "attributes", "index": 1.5, "text": "x"})).is_err());
}

#[test]
fn a_bad_multiplicity_is_reported_and_kept() {
    let mut s = classes();
    let c = first_of(&s, "uml.Class");
    let r = run(&mut s, "uml.addMember", json!({"element": c, "text": "- n: int [3..1]"}));
    let problems = r["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{r}");
    assert!(problems[0]["problem"].as_str().unwrap().contains("3..1"));
    assert!(rows(&r).iter().any(|x| x.contains("[3..1]")), "never refused");
}

#[test]
fn a_shape_id_names_the_element_and_other_elements_are_refused() {
    let mut s = classes();
    let c = first_of(&s, "uml.Class");
    let shape = s.doc.page().unwrap().shapes.iter().find(|x| x.element.as_deref() == Some(c.as_str())).unwrap().id.clone();
    run(&mut s, "uml.addMember", json!({"element": shape, "text": "name: string"}));
    // An enumeration keeps literals; the class lists are refused on it.
    let e = run(&mut s, "element.create", json!({"type": "uml.Enumeration", "name": "Colour", "x": 3000, "y": 3000}));
    let en = s.doc.page().unwrap().shape(e["shape"].as_str().unwrap()).unwrap().element.clone().unwrap();
    let l = run(&mut s, "uml.addMember", json!({"element": en, "text": "RED"}));
    assert_eq!(l["list"], "literals");
    assert!(s.run("uml.addMember", &json!({"element": en, "list": "operations", "text": "x()"})).is_err());
    // A flowchart element has no member lists.
    let mut f = Session::new(Document::new());
    run(&mut f, "template.new", json!({"template": "flowchart.decision"}));
    let p = first_of(&f, "flowchart.Process");
    let err = f.run("uml.addMember", &json!({"element": p, "text": "x"})).unwrap_err().to_string();
    assert!(err.contains("no member lists"), "{err}");
}

#[test]
fn hostile_params_are_refused_without_a_panic() {
    let mut s = classes();
    let c = first_of(&s, "uml.Class");
    let junk = [
        json!({"element": c, "list": 5, "text": 5, "index": 1e300}),
        json!({"element": c, "list": "attributes", "text": "x".repeat(70_000), "index": -1}),
        json!({"element": c, "list": "attributes", "from": f64::NAN, "to": "x"}),
        json!({"element": c, "list": "attributes", "text": "\u{0}\u{ffff}\n\n[", "index": 0}),
        json!({"element": {}, "list": [], "index": "a"}),
    ];
    for id in ["uml.addMember", "uml.setMember", "uml.removeMember", "uml.moveMember"] {
        for j in &junk {
            let _ = s.run(id, j);
        }
    }
    // The list never passes the cap.
    for _ in 0..300 {
        let _ = s.run("uml.addMember", &json!({"element": c, "list": "attributes", "text": "a: int"}));
    }
    let n = s.doc.model.element(&c).unwrap().prop("attributes").map_or(0, |v| v.display().lines().count());
    assert!(n <= vizcraft_diagram::compartment::MAX_ROWS, "{n}");
}
