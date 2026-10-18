use serde_json::{Value, json};
use vizcraft_diagram::data::{self, DataType, MAX_ROWS};
use vizcraft_diagram::{Document, Page, Shape, ShapeKind};

use crate::Session;

/// A page with free shapes `a` and `b`, plus a second page with shape `c`.
fn session() -> Session {
    let mut doc = Document::new();
    let page = doc.page_mut().unwrap();
    page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
    page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 300.0, 0.0, 100.0, 60.0));
    let mut p2 = Page::new("p2", "Second");
    p2.shapes.push(Shape::new("c", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
    doc.pages.push(p2);
    doc.sanitize();
    Session::new(doc)
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.run(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn rows(s: &Session, id: &str) -> Vec<(String, DataType, String)> {
    s.doc.shape(id).unwrap().data.rows.iter().map(|r| (r.key.clone(), r.kind, r.value.clone())).collect()
}

#[test]
fn set_creates_typed_rows_and_validates_values() {
    let mut s = session();
    let r = run(&mut s, "data.set", json!({"ids": ["a"], "key": "cost", "value": 12.5, "unit": "USD"}));
    assert_eq!(r["changed"], json!(["a"]));
    assert_eq!(rows(&s, "a"), [("cost".to_string(), DataType::Number, "12.5".to_string())]);
    // The existing row keeps its type: text that is a number with the unit is fine, junk is not.
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "cost", "value": "14 usd"}));
    assert_eq!(rows(&s, "a")[0].2, "14");
    assert!(s.run("data.set", &json!({"ids": ["a"], "key": "cost", "value": "lots"})).is_err());
    assert!(s.run("data.set", &json!({"ids": ["a"], "key": "due", "type": "date", "value": "2023-02-30"})).is_err());
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "due", "type": "date", "value": "2024-02-29"}));
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "done", "value": false}));
    assert_eq!(rows(&s, "a")[2].1, DataType::Bool);
    assert!(s.run("data.set", &json!({"ids": ["a"], "key": "level", "type": "enum", "value": "Low"})).is_err(), "an enum needs values");
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "level", "type": "enum", "values": ["Low", "High"], "value": "high"}));
    assert_eq!(rows(&s, "a")[3].2, "High", "enum values match ignoring case and store the defined spelling");
    assert!(s.run("data.set", &json!({"ids": ["a"], "key": "level", "value": "Medium"})).is_err());
    assert!(s.run("data.set", &json!({"ids": ["a"], "key": "site", "type": "url", "value": "javascript:alert(1)"})).is_err());
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "site", "type": "url", "value": "https://example.com"}));
    // Null clears; the row stays.
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "cost", "value": null}));
    assert_eq!(rows(&s, "a")[0].2, "");
    // Undo takes back one set.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(rows(&s, "a")[0].2, "14");
}

#[test]
fn set_defaults_to_the_selection_and_reports_required_warnings() {
    let mut s = session();
    assert!(s.run("data.set", &json!({"key": "x", "value": 1})).is_err(), "nothing selected");
    run(&mut s, "edit.selectAll", json!({}));
    let r = run(&mut s, "data.set", json!({"key": "owner", "value": "", "required": true}));
    assert_eq!(r["changed"], json!(["a", "b"]));
    assert_eq!(r["warnings"].as_array().unwrap().len(), 2);
    let r = run(&mut s, "data.get", json!({"id": "a"}));
    assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
    // Bad params are errors, never panics.
    for p in [
        json!({"ids": ["a"], "key": "", "value": 1}),
        json!({"ids": ["a"], "key": " lead", "value": 1}),
        json!({"ids": ["a"], "key": "k/k", "value": 1}),
        json!({"ids": ["a"], "key": "k"}),
        json!({"ids": ["a"], "key": "k", "value": [1]}),
        json!({"ids": ["a"], "key": "k", "value": 1, "type": "colour"}),
        json!({"ids": ["nope"], "key": "k", "value": 1}),
        json!({"ids": ["a"], "key": "k", "value": "x".repeat(5000)}),
        json!({"ids": ["a"], "key": "k", "value": 1, "label": "x".repeat(500)}),
        json!({"ids": ["a"], "key": "k", "value": 1, "values": "a,b"}),
    ] {
        assert!(s.run("data.set", &p).is_err(), "{p}");
    }
}

#[test]
fn rows_are_capped_per_shape() {
    let mut s = session();
    for i in 0..MAX_ROWS {
        run(&mut s, "data.set", json!({"ids": ["a"], "key": format!("k{i}"), "value": "v"}));
    }
    assert!(s.run("data.set", &json!({"ids": ["a"], "key": "one-more", "value": "v"})).is_err());
    assert_eq!(s.doc.shape("a").unwrap().data.rows.len(), MAX_ROWS);
}

#[test]
fn remove_drops_rows_and_refuses_unknown_keys() {
    let mut s = session();
    run(&mut s, "data.set", json!({"ids": ["a", "b"], "key": "cost", "value": 1}));
    let r = run(&mut s, "data.remove", json!({"ids": ["a", "b", "c"], "key": "cost"}));
    assert_eq!(r["changed"], json!(["a", "b"]));
    assert!(rows(&s, "a").is_empty());
    assert!(s.run("data.remove", &json!({"ids": ["a"], "key": "cost"})).is_err());
    assert!(s.run("data.remove", &json!({"ids": ["a"]})).is_err());
}

#[test]
fn page_keys_reach_shapes_that_opt_in() {
    let mut s = session();
    let r = run(&mut s, "data.defineKey", json!({"key": "cost", "type": "number", "unit": "USD", "default": 5, "ids": ["a"]}));
    assert_eq!(r["changed"], json!(["a"]));
    assert_eq!(rows(&s, "a"), [("cost".to_string(), DataType::Number, "5".to_string())]);
    assert!(rows(&s, "b").is_empty(), "b did not opt in");
    // b opts in and takes every page key; later keys reach both.
    run(&mut s, "data.usePageKeys", json!({"ids": ["b"]}));
    assert_eq!(rows(&s, "b").len(), 1);
    run(&mut s, "data.defineKey", json!({"key": "status", "type": "enum", "values": ["Open", "Done"], "required": true}));
    assert_eq!(rows(&s, "a").len(), 2);
    assert_eq!(rows(&s, "b").len(), 2);
    assert!(rows(&s, "c").is_empty(), "another page");
    // Setting a page key on a shape that did not opt in takes the definition.
    run(&mut s, "data.usePageKeys", json!({"ids": ["b"], "value": false}));
    let r = run(&mut s, "data.get", json!({"id": "a"}));
    assert_eq!(r["keys"].as_array().unwrap().len(), 2);
    assert!(r["rows"].as_array().unwrap().iter().all(|r| r["pageKey"] == true));
    assert!(r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("status")));
    // Redefining keeps values that still fit and refuses ones that do not.
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "status", "value": "done"}));
    run(&mut s, "data.defineKey", json!({"key": "status", "type": "enum", "values": ["Open", "Done", "Blocked"]}));
    assert_eq!(rows(&s, "a")[1].2, "Done");
    assert!(s.run("data.defineKey", &json!({"key": "status", "type": "number"})).is_err());
    assert!(s.run("data.defineKey", &json!({"key": "x", "type": "enum"})).is_err());
    assert!(s.run("data.defineKey", &json!({"key": "x"})).is_err());
    assert!(s.run("data.defineKey", &json!({"key": "x", "type": "date", "default": "soon"})).is_err());
    assert!(s.run("data.defineKey", &json!({"key": "x", "type": "string", "page": "nope"})).is_err());
    assert!(s.run("data.defineKey", &json!({"key": "x", "type": "string", "ids": ["c"]})).is_err(), "c is on another page");
    // A key on another page.
    run(&mut s, "data.defineKey", json!({"key": "room", "type": "string", "page": "Second", "ids": ["c"]}));
    assert_eq!(rows(&s, "c").len(), 1);
    assert_eq!(data::page_keys(&s.doc.pages[1]).len(), 1);
    // Undefining removes the definition and the rows of shapes that use page keys.
    let r = run(&mut s, "data.undefineKey", json!({"key": "status"}));
    assert_eq!(r["changed"], json!(["a"]));
    assert_eq!(rows(&s, "a").len(), 1);
    assert_eq!(rows(&s, "b").len(), 2, "b stopped using page keys, so its rows stay");
    run(&mut s, "data.undefineKey", json!({"key": "cost", "keepRows": true}));
    assert_eq!(rows(&s, "a").len(), 1);
    assert!(s.run("data.undefineKey", &json!({"key": "cost"})).is_err());
    assert!(!s.doc.pages[0].ext.contains_key(data::PAGE_KEYS_EXT));
}

#[test]
fn element_properties_merge_as_model_owned_rows() {
    let mut s = session();
    let r = run(&mut s, "element.create", json!({"type": "flowchart.Process", "name": "Mix", "x": 10, "y": 200}));
    let shape = r["shape"].as_str().unwrap().to_string();
    let el = r["id"].as_str().unwrap().to_string();
    run(&mut s, "element.setProperty", json!({"id": el, "key": "owner", "value": "Line 2"}));
    run(&mut s, "data.set", json!({"ids": [shape], "key": "cost", "value": 3}));
    let r = run(&mut s, "data.get", json!({"id": shape}));
    assert_eq!(r["element"], json!(el));
    let rows = r["rows"].as_array().unwrap();
    let owner = rows.iter().find(|r| r["key"] == "owner").unwrap();
    assert_eq!((owner["source"].as_str(), owner["readOnly"].as_bool(), owner["value"].as_str()), (Some("model"), Some(true), Some("Line 2")));
    let automated = rows.iter().find(|r| r["key"] == "automated").unwrap();
    assert_eq!(automated["type"], "bool");
    let cost = rows.iter().find(|r| r["key"] == "cost").unwrap();
    assert_eq!((cost["source"].as_str(), cost["value"].as_f64()), (Some("shape"), Some(3.0)));
    // Model-owned keys are written through element commands.
    assert!(s.run("data.set", &json!({"ids": [shape], "key": "owner", "value": "x"})).is_err());
    // A page key the element owns adds no shadowing row.
    run(&mut s, "data.defineKey", json!({"key": "owner", "type": "string", "ids": [shape]}));
    assert!(!s.doc.shape(&shape).unwrap().data.rows.iter().any(|r| r.key == "owner"));
    // The table holds both.
    let t = run(&mut s, "data.list", json!({}));
    let row = t["shapes"].as_array().unwrap().iter().find(|r| r["id"] == json!(shape)).unwrap().clone();
    assert_eq!(row["values"]["owner"], "Line 2");
    assert_eq!(row["values"]["cost"], 3.0);
}

#[test]
fn list_tabulates_shapes_by_key() {
    let mut s = session();
    run(&mut s, "data.defineKey", json!({"key": "cost", "type": "number", "ids": ["a", "b"]}));
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "cost", "value": 4}));
    run(&mut s, "data.set", json!({"ids": ["b"], "key": "note", "value": "hi"}));
    let t = run(&mut s, "data.list", json!({}));
    assert_eq!(t["keys"], json!(["cost", "note"]));
    assert_eq!(t["shapes"][0], json!({"id": "a", "values": {"cost": 4.0}}));
    assert_eq!(t["shapes"][1], json!({"id": "b", "values": {"cost": null, "note": "hi"}}));
    let t = run(&mut s, "data.list", json!({"page": 1}));
    assert_eq!(t["shapes"], json!([]));
    assert!(s.run("data.list", &json!({"page": true})).is_err());
}

#[test]
fn get_defaults_to_the_primary_selection() {
    let mut s = session();
    assert!(s.run("data.get", &json!({})).is_err());
    run(&mut s, "edit.selectAll", json!({}));
    let r = run(&mut s, "data.get", json!({}));
    assert_eq!(r["id"], "a");
    assert_eq!(r["rows"], json!([]));
    assert!(s.run("data.get", &json!({"id": "zzz"})).is_err());
}

#[test]
fn locked_layers_refuse_data_changes() {
    let mut s = session();
    run(&mut s, "layer.add", json!({"name": "L"}));
    run(&mut s, "layer.assign", json!({"ids": ["a"], "layer": "L"}));
    run(&mut s, "layer.set", json!({"layer": "L", "locked": true}));
    assert!(s.run("data.set", &json!({"ids": ["a"], "key": "k", "value": 1})).is_err());
    run(&mut s, "data.set", json!({"ids": ["b"], "key": "k", "value": 1}));
}

#[test]
fn pane_and_dialog_commands_ask_the_ui() {
    let mut s = session();
    let r = run(&mut s, "view.shapeDataPane", json!({"value": true}));
    assert_eq!(r, json!({"pane": "shapeData", "value": true}));
    run(&mut s, "data.defineDialog", json!({}));
    assert_eq!(s.ui_requests, vec![json!({"pane": "shapeData", "value": true}), json!({"open": "defineShapeData"})]);
    assert!(s.run("view.shapeDataPane", &json!({"value": "yes"})).is_err());
    assert!(!s.dirty, "neither changes the document");
}

#[test]
fn data_survives_save_and_load() {
    let mut s = session();
    run(&mut s, "data.defineKey", json!({"key": "cost", "type": "number", "unit": "USD", "ids": ["a"]}));
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "cost", "value": 7}));
    run(&mut s, "data.set", json!({"ids": ["a"], "key": "secret", "value": "x", "hidden": true}));
    let text = serde_json::to_string(&s.doc).unwrap();
    let back: Document = serde_json::from_str(&text).unwrap();
    assert_eq!(back.shape("a").unwrap().data, s.doc.shape("a").unwrap().data);
    assert_eq!(data::page_keys(&back.pages[0]), data::page_keys(&s.doc.pages[0]));
}
