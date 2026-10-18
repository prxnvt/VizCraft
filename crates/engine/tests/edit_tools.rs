//! The Home tab's editing tools: duplicate, paste special, format painter, change shape, select
//! by type, and Find and Replace.

use serde_json::{Value, json};
use vizcraft_diagram::{Color, Dash, End};
use vizcraft_engine::{Document, Session};

fn blank() -> Session {
    Session::new(Document::new())
}

fn add(s: &mut Session, stencil: &str, x: f64, text: &str) -> String {
    let r = s.run("shape.add", &json!({"stencil": stencil, "x": x, "y": 100, "text": text})).unwrap();
    r["id"].as_str().unwrap().to_string()
}

fn connect(s: &mut Session, a: &str, pa: usize, b: &str, pb: usize) -> String {
    let r = s.run("connector.add", &json!({"from": {"shape": a, "point": pa}, "to": {"shape": b, "point": pb}})).unwrap();
    r["id"].as_str().unwrap().to_string()
}

fn ids(v: &Value, k: &str) -> Vec<String> {
    v[k].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
}

#[test]
fn duplicate_keeps_internal_glue_and_selects_the_copies() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "A");
    let b = add(&mut s, "basic.rectangle", 300.0, "B");
    let c = connect(&mut s, &a, 1, &b, 3);
    s.run("edit.select", &json!({"ids": [a, b, c]})).unwrap();
    let r = s.run("edit.duplicate", &json!({})).unwrap();
    let new = ids(&r, "duplicated");
    assert_eq!(new.len(), 3);
    let page = s.doc.page().unwrap();
    let (na, nb, nc) = (page.shape(&new[0]).unwrap(), page.shape(&new[1]).unwrap(), page.connector(&new[2]).unwrap());
    assert_eq!((na.x, na.y), (20.0, 120.0), "the default offset is 20 right and down");
    assert_eq!(na.text, "A");
    assert_eq!(nc.start, End::point(na.id.clone(), 1), "glue between copied items points at the copies");
    assert_eq!(nc.end, End::point(nb.id.clone(), 3));
    assert_eq!(s.sel.items, new);

    // Explicit ids and offset; a connector left out of the copy is not copied.
    let r = s.run("edit.duplicate", &json!({"ids": [a], "dx": 0, "dy": 200})).unwrap();
    let only = ids(&r, "duplicated");
    assert_eq!(only.len(), 1);
    assert_eq!(s.doc.page().unwrap().shape(&only[0]).unwrap().y, 300.0);
    assert!(s.run("edit.duplicate", &json!({"ids": ["nope"]})).is_err());
    s.run("edit.selectNone", &json!({})).unwrap();
    assert!(s.run("edit.duplicate", &json!({})).is_err(), "nothing selected, nothing to duplicate");
}

#[test]
fn duplicate_copies_a_group_with_its_members() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "A");
    let b = add(&mut s, "basic.rectangle", 200.0, "B");
    s.run("edit.select", &json!({"ids": [a, b]})).unwrap();
    s.run("arrange.group", &json!({})).unwrap();
    let group = s.sel.items[0].clone();
    let before = s.doc.page().unwrap().shapes.len();
    s.run("edit.duplicate", &json!({})).unwrap();
    let page = s.doc.page().unwrap();
    assert_eq!(page.shapes.len(), before + 3);
    let copy = &s.sel.items;
    assert_eq!(copy.len(), 1, "the copied group is selected as one item");
    assert_ne!(copy[0], group);
    assert_eq!(page.shapes.iter().filter(|x| x.parent.as_deref() == Some(copy[0].as_str())).count(), 2);
}

#[test]
fn paste_special_pastes_text_style_or_shapes() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "Source");
    let b = add(&mut s, "basic.rectangle", 300.0, "Target");
    s.run("shape.setStyle", &json!({"ids": [a], "fill": "#ff0000", "bold": true})).unwrap();
    s.run("edit.select", &json!({"ids": [a]})).unwrap();
    s.run("edit.copy", &json!({})).unwrap();

    // No `as`: the UI is asked for the dialog and nothing changes.
    let rev = s.doc.clone();
    let r = s.run("edit.pasteSpecial", &json!({})).unwrap();
    assert_eq!(r["dialog"], "pasteSpecial");
    assert_eq!(s.ui_requests.last().unwrap()["open"], "pasteSpecial");
    assert_eq!(s.doc, rev);

    let fill_b = s.doc.page().unwrap().shape(&b).unwrap().style.fill;
    s.run("edit.select", &json!({"ids": [b]})).unwrap();
    s.run("edit.pasteSpecial", &json!({"as": "text"})).unwrap();
    let tb = s.doc.page().unwrap().shape(&b).unwrap().clone();
    assert_eq!(tb.text, "Source");
    assert_eq!(tb.style.fill, fill_b, "text only leaves the style");

    s.run("edit.pasteSpecial", &json!({"as": "style"})).unwrap();
    let tb = s.doc.page().unwrap().shape(&b).unwrap().clone();
    assert_eq!(tb.style.fill, Color::rgb(255, 0, 0));
    assert!(tb.style.bold);

    let n = s.doc.page().unwrap().shapes.len();
    s.run("edit.pasteSpecial", &json!({"as": "shapes"})).unwrap();
    assert_eq!(s.doc.page().unwrap().shapes.len(), n + 1);

    // Plain text from another program pastes as text but has no style.
    s.run("edit.select", &json!({"ids": [b]})).unwrap();
    s.run("edit.pasteSpecial", &json!({"as": "text", "text": "from elsewhere"})).unwrap();
    assert_eq!(s.doc.page().unwrap().shape(&b).unwrap().text, "from elsewhere");
    assert!(s.run("edit.pasteSpecial", &json!({"as": "style", "text": "plain"})).is_err());
    assert!(s.run("edit.pasteSpecial", &json!({"as": "picture"})).is_err());
}

#[test]
fn format_painter_copies_fill_line_and_effects_but_not_text() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "Source");
    let b = add(&mut s, "basic.ellipse", 300.0, "Target");
    let c = connect(&mut s, &a, 1, &b, 3);
    s.run(
        "shape.setStyle",
        &json!({"ids": [a], "fill": "#00ff00", "line": "#0000ff", "lineWidth": 4, "dash": "dashed", "shadow": true, "fontSize": 30, "bold": true}),
    )
    .unwrap();
    let before = s.doc.page().unwrap().shape(&b).unwrap().clone();
    let r = s.run("edit.formatPainter", &json!({"from": a, "to": [b, c]})).unwrap();
    assert_eq!(ids(&r, "painted"), vec![b.clone(), c.clone()]);
    let page = s.doc.page().unwrap();
    let tb = page.shape(&b).unwrap();
    assert_eq!(tb.style.fill, Color::rgb(0, 255, 0));
    assert_eq!(tb.style.line, Color::rgb(0, 0, 255));
    assert_eq!(tb.style.line_width, 4.0);
    assert_eq!(tb.style.dash, Dash::Dashed);
    assert!(tb.style.shadow.is_some());
    assert_eq!(tb.text, "Target", "text stays");
    assert_eq!(tb.style.font_size, before.style.font_size, "text formatting stays");
    assert!(!tb.style.bold);
    assert_eq!((tb.x, tb.y, tb.w, tb.h, tb.kind), (before.x, before.y, before.w, before.h, before.kind), "geometry stays");
    let tc = page.connector(&c).unwrap();
    assert_eq!(tc.style.line, Color::rgb(0, 0, 255));
    assert_eq!(tc.style.dash, Dash::Dashed);

    // Defaults: from the primary selection to the rest of it.
    let d = add(&mut s, "basic.rectangle", 600.0, "D");
    s.run("edit.select", &json!({"ids": [a, d]})).unwrap();
    s.run("edit.formatPainter", &json!({})).unwrap();
    assert_eq!(s.doc.page().unwrap().shape(&d).unwrap().style.fill, Color::rgb(0, 255, 0));
    assert!(s.run("edit.formatPainter", &json!({"from": a, "to": "s2"})).is_err());
    assert!(s.run("edit.formatPainter", &json!({"from": "nope", "to": [b]})).is_err());
    assert!(s.run("edit.formatPainter", &json!({"from": a, "to": [a]})).is_err());
}

#[test]
fn change_shape_keeps_text_glue_data_and_hand_formatting() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "Keep me");
    let b = add(&mut s, "basic.rectangle", 300.0, "Other");
    let c = connect(&mut s, &a, 1, &b, 3);
    s.run("shape.setStyle", &json!({"ids": [a], "fill": "#123456"})).unwrap();
    s.run("layer.add", &json!({"name": "Walls"})).unwrap();
    s.run("layer.assign", &json!({"ids": [a], "layer": "Walls"})).unwrap();
    s.doc.page_mut().unwrap().shape_mut(&a).unwrap().ext.insert("cost".into(), "12".into());
    let before = s.doc.page().unwrap().shape(&a).unwrap().clone();
    let old_point = before.connection_point_world(1).unwrap();

    let r = s.run("edit.changeShape", &json!({"ids": [a], "stencil": "basic.ellipse"})).unwrap();
    assert_eq!(ids(&r, "changed"), vec![a.clone()]);
    let page = s.doc.page().unwrap();
    let sh = page.shape(&a).unwrap();
    let ellipse = vizcraft_stencils::master("basic.ellipse").unwrap();
    assert_eq!(sh.stencil, "basic.ellipse");
    assert_eq!(sh.kind, ellipse.kind);
    assert_eq!(sh.text, "Keep me");
    assert_eq!(sh.ext.get("cost").map(String::as_str), Some("12"));
    assert_eq!(sh.layers, before.layers);
    assert_eq!((sh.x, sh.y, sh.w, sh.h), (before.x, before.y, before.w, before.h));
    assert_eq!(sh.style.fill, Color::parse("#123456").unwrap(), "hand formatting is kept");
    assert_eq!(sh.style.line, ellipse.style.line, "untouched fields follow the new master");
    // The connector stays glued, now to the point of the ellipse nearest the old one.
    let conn = page.connector(&c).unwrap();
    let End::Glued { shape, point: Some(i) } = &conn.start else { panic!("still glued to a point: {:?}", conn.start) };
    assert_eq!(shape, &a);
    let new_point = sh.connection_point_world(*i).unwrap();
    let best = sh.connection_points_world().into_iter().map(|p| (p - old_point).length()).fold(f64::INFINITY, f64::min);
    assert!(((new_point - old_point).length() - best).abs() < 1e-9);

    // Selection default, unknown masters and groups.
    s.run("edit.select", &json!({"ids": [b]})).unwrap();
    s.run("edit.changeShape", &json!({"stencil": "flowchart.decision"})).unwrap();
    assert_eq!(s.doc.page().unwrap().shape(&b).unwrap().stencil, "flowchart.decision");
    assert!(s.run("edit.changeShape", &json!({"ids": [a], "stencil": "nope.nope"})).is_err());
    s.run("edit.select", &json!({"ids": [a, b]})).unwrap();
    s.run("arrange.group", &json!({})).unwrap();
    let g = s.sel.items[0].clone();
    assert!(s.run("edit.changeShape", &json!({"ids": [g], "stencil": "basic.ellipse"})).is_err());
}

#[test]
fn select_by_type_picks_each_kind() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "A");
    let b = add(&mut s, "basic.rectangle", 300.0, "");
    let c = connect(&mut s, &a, 1, &b, 3);
    let d = add(&mut s, "basic.rectangle", 600.0, "");
    let e = add(&mut s, "basic.rectangle", 900.0, "");
    s.run("edit.select", &json!({"ids": [d, e]})).unwrap();
    s.run("arrange.group", &json!({})).unwrap();
    let g = s.sel.items[0].clone();
    s.run("layer.add", &json!({"name": "L"})).unwrap();
    s.run("layer.assign", &json!({"ids": [b], "layer": "L"})).unwrap();

    let sel = |s: &mut Session, p: Value| {
        s.run("edit.selectByType", &p).unwrap();
        s.sel.items.clone()
    };
    assert_eq!(sel(&mut s, json!({"kind": "shapes"})), vec![a.clone(), b.clone()]);
    assert_eq!(sel(&mut s, json!({"kind": "connectors"})), vec![c.clone()]);
    assert_eq!(sel(&mut s, json!({"kind": "groups"})), vec![g]);
    assert_eq!(sel(&mut s, json!({"kind": "text"})), vec![a]);
    assert_eq!(sel(&mut s, json!({"kind": "layer", "layer": "L"})), vec![b]);
    assert!(s.run("edit.selectByType", &json!({"kind": "layer", "layer": "nope"})).is_err());
    assert!(s.run("edit.selectByType", &json!({"kind": "circles"})).is_err());
}

#[test]
fn find_searches_text_and_data_with_case_and_whole_word() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "Pump station");
    let b = add(&mut s, "basic.rectangle", 300.0, "pumping");
    s.doc.page_mut().unwrap().shape_mut(&b).unwrap().ext.insert("part".into(), "PUMP-7".into());
    let find = |s: &mut Session, p: Value| -> Vec<(String, String)> {
        let r = s.run("edit.find", &p).unwrap();
        r["hits"].as_array().unwrap().iter().map(|h| (h["id"].as_str().unwrap().to_string(), h["field"].as_str().unwrap().to_string())).collect()
    };
    let t = |id: &str| (id.to_string(), "text".to_string());
    assert_eq!(find(&mut s, json!({"text": "pump"})), vec![t(&a), t(&b)]);
    assert_eq!(find(&mut s, json!({"text": "pump", "matchCase": true})), vec![t(&b)]);
    assert_eq!(find(&mut s, json!({"text": "pump", "wholeWord": true})), vec![t(&a)]);
    assert_eq!(find(&mut s, json!({"text": "pump", "wholeWord": true, "inData": true})), vec![t(&a), (b.clone(), "data:part".into())]);
    assert!(find(&mut s, json!({"text": "Pump", "matchCase": true, "wholeWord": true, "inData": true})).len() == 1);
    assert!(s.run("edit.find", &json!({"text": ""})).is_err());
    assert!(s.run("edit.find", &json!({"text": "x", "scope": "galaxy"})).is_err());
}

#[test]
fn replace_works_across_pages_and_skips_locked_layers() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "Old valve");
    let first = s.doc.page().unwrap().id.clone();
    let second = s.run("page.add", &json!({})).unwrap()["page"].as_str().unwrap().to_string();
    let b = add(&mut s, "basic.rectangle", 0.0, "old Valve, OLD valve");
    let c = add(&mut s, "basic.rectangle", 300.0, "Bold valve");

    // Page scope sees the active page only.
    let r = s.run("edit.find", &json!({"text": "old"})).unwrap();
    assert!(r["hits"].as_array().unwrap().iter().all(|h| h["page"] == second.as_str()));
    // Document scope, whole word: "Bold" is not "old".
    let r = s.run("edit.find", &json!({"text": "old", "scope": "document", "wholeWord": true})).unwrap();
    let hits: Vec<&str> = r["hits"].as_array().unwrap().iter().map(|h| h["id"].as_str().unwrap()).collect();
    assert_eq!(hits, vec![a.as_str(), b.as_str()]);
    assert_eq!(r["hits"][0]["page"], first.as_str());

    // One replace: the first hit unless `at` names another.
    let r = s
        .run(
            "edit.replace",
            &json!({"text": "old", "with": "new", "scope": "document", "wholeWord": true, "at": {"page": second, "id": b, "field": "text"}}),
        )
        .unwrap();
    assert_eq!(r["replaced"], 2);
    assert_eq!(s.doc.pages[1].shape(&b).unwrap().text, "new Valve, new valve");
    assert_eq!(s.doc.pages[0].shape(&a).unwrap().text, "Old valve", "only the chosen hit");

    // Replace all with match case.
    s.run("edit.undo", &json!({})).unwrap();
    let r = s.run("edit.replace", &json!({"text": "old", "with": "new", "scope": "document", "matchCase": true, "all": true})).unwrap();
    assert_eq!(r["replaced"], 2, "`old` in b and in `Bold`");
    assert_eq!(s.doc.pages[1].shape(&b).unwrap().text, "new Valve, OLD valve");
    assert_eq!(s.doc.pages[1].shape(&c).unwrap().text, "Bnew valve");
    assert_eq!(s.doc.pages[0].shape(&a).unwrap().text, "Old valve");
    s.run("edit.undo", &json!({})).unwrap();

    // A shape on a locked layer is skipped.
    s.run("layer.add", &json!({"name": "Locked", "page": second})).unwrap();
    s.run("layer.assign", &json!({"ids": [c], "layer": "Locked", "page": second})).unwrap();
    s.run("layer.set", &json!({"layer": "Locked", "locked": true, "page": second})).unwrap();
    let r = s.run("edit.replace", &json!({"text": "valve", "with": "tap", "all": true})).unwrap();
    assert_eq!(ids(&r, "skipped"), vec![c.clone()]);
    assert_eq!(s.doc.pages[1].shape(&c).unwrap().text, "Bold valve");
    assert_eq!(s.doc.pages[1].shape(&b).unwrap().text, "old tap, OLD tap");
    assert!(s.run("edit.replace", &json!({"text": "zzz", "with": "y", "at": {"page": second, "id": b, "field": "text"}})).is_err());
}

#[test]
fn replace_in_data_and_connector_text() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "A");
    let b = add(&mut s, "basic.rectangle", 300.0, "B");
    let c = s.run("connector.add", &json!({"from": {"shape": a}, "to": {"shape": b}, "text": "flow 1"})).unwrap()["id"].as_str().unwrap().to_string();
    s.doc.page_mut().unwrap().shape_mut(&a).unwrap().ext.insert("tag".into(), "flow-A".into());
    let r = s.run("edit.replace", &json!({"text": "flow", "with": "line", "all": true, "inData": true})).unwrap();
    assert_eq!(r["replaced"], 2);
    let page = s.doc.page().unwrap();
    assert_eq!(page.connector(&c).unwrap().text, "line 1");
    assert_eq!(page.shape(&a).unwrap().ext.get("tag").map(String::as_str), Some("line-A"));
    assert!(r["hits"].as_array().unwrap().is_empty());
}

#[test]
fn find_and_replace_reach_typed_shape_data() {
    let mut s = blank();
    let a = add(&mut s, "basic.rectangle", 0.0, "Pump");
    s.run("data.set", &json!({"ids": [a], "key": "vendor", "value": "Acme"})).unwrap();
    s.run("data.set", &json!({"ids": [a], "key": "flow", "value": 12, "type": "number"})).unwrap();
    let r = s.run("edit.find", &json!({"text": "acme", "inData": true})).unwrap();
    assert_eq!(r["hits"][0]["field"], "data:vendor", "{r}");
    s.run("edit.replace", &json!({"text": "Acme", "with": "Apex", "inData": true, "all": true})).unwrap();
    let rows = s.run("data.get", &json!({"id": a})).unwrap();
    assert!(rows.to_string().contains("Apex"), "{rows}");
    // A replacement that does not fit the row's type is refused, and the value stays.
    s.run("edit.replace", &json!({"text": "12", "with": "lots", "inData": true, "all": true})).ok();
    let rows = s.run("data.get", &json!({"id": a})).unwrap();
    assert!(rows.to_string().contains("12") && !rows.to_string().contains("lots"), "{rows}");
}
