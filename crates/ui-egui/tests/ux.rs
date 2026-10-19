//! Keyboard insertion, the relationship-aware picker, label editing, relationship and row editing
//! under egui_kittest with real key and pointer events. Assertions read the session, never pixels.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use serde_json::{Value, json};
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness_with(session: Session) -> Harness<'static, CraftApp> {
    let app = CraftApp::new(session, Services::default());
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

/// A harness on a fresh document made from template `id`.
fn from_template(id: &str) -> Harness<'static, CraftApp> {
    let mut s = Session::new(vizcraft_engine::Document::new());
    s.run("template.new", &json!({"template": id})).unwrap();
    s.run("edit.selectNone", &json!({})).unwrap();
    harness_with(s)
}

fn key(h: &mut Harness<'_, CraftApp>, k: Key) {
    let m = Modifiers::NONE;
    h.event(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m });
    h.event(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m });
    h.run_steps(2);
}

fn type_text(h: &mut Harness<'_, CraftApp>, t: &str) {
    h.event(Event::Text(t.to_string()));
    h.run_steps(2);
}

fn select(h: &mut Harness<'_, CraftApp>, ids: &[&str]) {
    h.state_mut().run("edit.select", json!({"ids": ids})).unwrap();
    h.run_steps(2);
}

fn run(h: &mut Harness<'_, CraftApp>, id: &str, p: Value) -> Value {
    let r = h.state_mut().run(id, p).unwrap();
    h.run_steps(2);
    r
}

fn shape_count(h: &Harness<'_, CraftApp>) -> usize {
    h.state().session.doc.page().unwrap().shapes.len()
}

fn selected(h: &Harness<'_, CraftApp>) -> Vec<String> {
    h.state().session.sel.items.clone()
}

fn text_of(h: &Harness<'_, CraftApp>, id: &str) -> String {
    h.state().session.doc.page().unwrap().shape(id).unwrap().text.clone()
}

/// Select the first shape of a template page and return its id.
fn first_shape(h: &mut Harness<'_, CraftApp>) -> String {
    let id = h.state().session.doc.page().unwrap().shapes[0].id.clone();
    select(h, &[&id]);
    id
}

#[allow(dead_code)]
fn centre(h: &Harness<'_, CraftApp>, id: &str) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    let c = app.session.doc.page().unwrap().shape(id).unwrap().center();
    vp.world_to_screen(pos2(c.x as f32, c.y as f32))
}

#[allow(dead_code)]
fn click(h: &mut Harness<'_, CraftApp>, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

#[test]
fn tab_chains_three_steps_downwards_on_a_flowchart_and_typing_names_them() {
    let mut h = from_template("flowchart.decision");
    let start = first_shape(&mut h);
    let n = shape_count(&h);
    let mut prev = start.clone();
    for name in ["One", "Two", "Three"] {
        key(&mut h, Key::Tab);
        assert!(h.state().canvas.editing.is_some(), "Tab starts naming the new shape");
        type_text(&mut h, name);
        key(&mut h, Key::Enter);
        let new = selected(&h);
        assert_eq!(new.len(), 1);
        assert_ne!(new[0], prev);
        assert_eq!(text_of(&h, &new[0]), name);
        let page = h.state().session.doc.page().unwrap();
        let (a, b) = (page.shape(&prev).unwrap().center(), page.shape(&new[0]).unwrap().center());
        assert!(b.y > a.y, "the flowchart grows downwards: {a:?} then {b:?}");
        prev = new[0].clone();
    }
    assert_eq!(shape_count(&h), n + 3);
}

#[test]
fn insert_direction_turns_the_chain_and_reports_what_auto_resolves_to() {
    let mut h = from_template("flowchart.decision");
    let r = run(&mut h, "ui.insertDirection", json!({"direction": "auto"}));
    assert_eq!(r["resolved"], "down");
    let start = first_shape(&mut h);
    run(&mut h, "ui.insertDirection", json!({"direction": "right"}));
    key(&mut h, Key::Tab);
    key(&mut h, Key::Escape);
    let new = selected(&h);
    let page = h.state().session.doc.page().unwrap();
    assert!(page.shape(&new[0]).unwrap().center().x > page.shape(&start).unwrap().center().x);
    assert!(h.state().session.doc.page().unwrap().shape(&new[0]).unwrap().text.is_empty(), "Escape leaves it unnamed");
    assert!(h.state_mut().run("ui.insertDirection", json!({"direction": "sideways"})).is_err());
}

#[test]
fn enter_adds_a_sibling_under_the_same_predecessor_without_overlap() {
    let mut h = from_template("flowchart.decision");
    first_shape(&mut h);
    key(&mut h, Key::Tab);
    type_text(&mut h, "Child");
    key(&mut h, Key::Enter);
    let child = selected(&h)[0].clone();
    let parent = h
        .state()
        .session
        .doc
        .page()
        .unwrap()
        .connectors
        .iter()
        .find(|c| c.end.shape() == Some(child.as_str()))
        .unwrap()
        .start
        .shape()
        .unwrap()
        .to_string();
    key(&mut h, Key::Enter);
    type_text(&mut h, "Sibling");
    key(&mut h, Key::Enter);
    let sib = selected(&h)[0].clone();
    assert_ne!(sib, child);
    assert_eq!(text_of(&h, &sib), "Sibling");
    let page = h.state().session.doc.page().unwrap();
    let from = page.connectors.iter().find(|c| c.end.shape() == Some(sib.as_str())).unwrap().start.shape().unwrap();
    assert_eq!(from, parent, "the sibling hangs off the same predecessor");
    let (a, b) = (page.shape(&child).unwrap().bounds(), page.shape(&sib).unwrap().bounds());
    let overlap = a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;
    assert!(!overlap, "siblings do not overlap: {a:?} {b:?}");
}

#[test]
fn enter_without_a_predecessor_edits_the_text_and_a_connector_edits_its_label() {
    let mut h = from_template("flowchart.decision");
    let start = first_shape(&mut h);
    let n = shape_count(&h);
    // The template's first shape may have an incoming connector; use a fresh free shape instead.
    let r = run(&mut h, "element.create", json!({"type": "flowchart.Process", "name": "Solo", "x": 2000, "y": 2000}));
    let solo = r["shape"].as_str().unwrap().to_string();
    select(&mut h, &[&solo]);
    key(&mut h, Key::Enter);
    assert_eq!(h.state().canvas.editing.as_ref().map(|e| e.id().to_string()), Some(solo.clone()));
    assert_eq!(shape_count(&h), n + 1, "no sibling was added");
    key(&mut h, Key::Escape);
    let _ = start;
    // One connector selected: Enter edits its label.
    let cid = h.state().session.doc.page().unwrap().connectors[0].id.clone();
    select(&mut h, &[&cid]);
    key(&mut h, Key::Enter);
    assert_eq!(h.state().canvas.editing.as_ref().map(|e| e.id().to_string()), Some(cid));
}

#[test]
fn tab_and_enter_stay_silent_in_a_focused_field_the_picker_and_on_a_sequence_page() {
    let mut h = from_template("flowchart.decision");
    first_shape(&mut h);
    let n = shape_count(&h);
    // The picker owns the keys.
    run(&mut h, "ui.insertPicker", json!({}));
    key(&mut h, Key::Tab);
    assert_eq!(shape_count(&h), n);
    key(&mut h, Key::Escape);
    // A sequence page: Tab does nothing.
    let mut seq = from_template("uml.sequence");
    let first = seq.state().session.doc.page().unwrap().shapes[0].id.clone();
    select(&mut seq, &[&first]);
    let m = seq.state().session.doc.page().unwrap().shapes.len();
    key(&mut seq, Key::Tab);
    key(&mut seq, Key::Enter);
    assert_eq!(seq.state().session.doc.page().unwrap().shapes.len(), m);
    assert!(seq.state().canvas.editing.is_none());
}

#[test]
fn f2_edits_the_selection_and_a_multiple_selection_gets_a_status_hint() {
    let mut h = from_template("flowchart.decision");
    let id = first_shape(&mut h);
    key(&mut h, Key::F2);
    assert_eq!(h.state().canvas.editing.as_ref().map(|e| e.id().to_string()), Some(id));
    key(&mut h, Key::Escape);
    run(&mut h, "edit.selectAll", json!({}));
    key(&mut h, Key::Tab);
    assert_eq!(h.state().status_msg.as_ref().map(|s| s.0.as_str()), Some("Select one shape to continue from"));
}

/// Ids of the shapes bound to elements of `type_id`, in page order.
fn shapes_of(h: &Harness<'_, CraftApp>, type_id: &str) -> Vec<String> {
    let app = h.state();
    let doc = &app.session.doc;
    doc.page()
        .unwrap()
        .shapes
        .iter()
        .filter(|s| s.element.as_deref().and_then(|e| doc.model.element(e)).is_some_and(|e| e.type_id == type_id))
        .map(|s| s.id.clone())
        .collect()
}

fn picker_rows(h: &mut Harness<'_, CraftApp>, query: &str) -> Vec<Value> {
    let r = h.state_mut().run("ui.insertPicker", json!({"query": query})).unwrap();
    h.run_steps(2);
    r["rows"].as_array().unwrap().clone()
}

#[test]
fn slash_opens_the_picker_without_a_slash_in_the_query() {
    let mut h = from_template("flowchart.decision");
    first_shape(&mut h);
    h.event(Event::Key { key: Key::Slash, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.event(Event::Text("/".into()));
    h.event(Event::Key { key: Key::Slash, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    let pk = h.state().insertion.picker.clone().expect("/ opens the picker");
    assert_eq!(pk.query, "", "the slash that opened it is not typed into it");
    // While open, / is typed into the query.
    type_text(&mut h, "a/b");
    assert_eq!(h.state().insertion.picker.as_ref().unwrap().query, "a/b");
}

#[test]
fn the_picker_leads_with_next_steps_for_one_class_and_connect_rows_for_two() {
    let mut h = from_template("uml.classDiagram");
    let classes = shapes_of(&h, "uml.Class");
    assert!(classes.len() >= 2, "the template draws classes: {classes:?}");
    select(&mut h, &[&classes[0]]);
    let rows = picker_rows(&mut h, "");
    assert_eq!(rows[0]["kind"], "next", "{rows:?}");
    assert!(rows[0]["group"].as_str().unwrap().starts_with("From "), "{rows:?}");
    assert!(rows.iter().any(|r| r["kind"] == "shape"), "{rows:?}");
    key(&mut h, Key::Escape);
    let rows = picker_rows(&mut h, "pattern");
    assert!(rows.iter().any(|r| r["kind"] == "pattern" && r["group"] == "Pattern"), "{rows:?}");
    key(&mut h, Key::Escape);
    select(&mut h, &[&classes[0], &classes[1]]);
    let rows = picker_rows(&mut h, "");
    let connects: Vec<&Value> = rows.iter().filter(|r| r["kind"] == "connect").collect();
    assert!(!connects.is_empty(), "{rows:?}");
    assert!(rows.iter().position(|r| r["kind"] == "connect") < rows.iter().position(|r| r["kind"] == "shape"));
    // Enter on the first Connect row draws that relationship.
    let before = h.state().session.doc.model.relationships.len();
    key(&mut h, Key::Enter);
    assert_eq!(h.state().session.doc.model.relationships.len(), before + 1);
}

#[test]
fn a_next_row_inserts_the_step_and_starts_naming_it() {
    let mut h = from_template("uml.classDiagram");
    let classes = shapes_of(&h, "uml.Class");
    select(&mut h, &[&classes[0]]);
    let rows = picker_rows(&mut h, "");
    let first = rows[0]["label"].as_str().unwrap().to_string();
    assert!(first.contains(" to "), "relationship rows read `<relationship> to <type>`: {first}");
    let n = shape_count(&h);
    key(&mut h, Key::Enter);
    assert_eq!(shape_count(&h), n + 1);
    assert!(h.state().canvas.editing.is_some(), "the new element is ready to be named");
}

#[test]
fn a_pattern_row_inserts_a_pattern() {
    let mut h = from_template("flowchart.decision");
    let rows = picker_rows(&mut h, "");
    let pattern = rows.iter().find(|r| r["kind"] == "pattern").expect("patterns are listed")["label"].as_str().unwrap().to_string();
    key(&mut h, Key::Escape);
    let n = shape_count(&h);
    h.state_mut().run("ui.insertPicker", json!({"query": pattern})).unwrap();
    h.run_steps(2);
    key(&mut h, Key::Enter);
    assert!(shape_count(&h) > n, "the pattern drew shapes");
}

fn drag(h: &mut Harness<'_, CraftApp>, from: Pos2, to: Pos2) {
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=8 {
        h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

#[test]
fn a_connector_drawn_between_two_actions_is_a_control_flow() {
    let mut h = from_template("uml.activity");
    let a = run(&mut h, "element.create", json!({"type": "uml.Action", "name": "Pack", "x": 3000, "y": 3000}));
    let b = run(&mut h, "element.create", json!({"type": "uml.Action", "name": "Ship", "x": 3400, "y": 3000}));
    let (a, b) = (a["shape"].as_str().unwrap().to_string(), b["shape"].as_str().unwrap().to_string());
    run(&mut h, "view.pan", json!({"x": 3300, "y": 3030}));
    run(&mut h, "edit.selectNone", json!({}));
    run(&mut h, "ui.tool", json!({"tool": "connector"}));
    let n = h.state().session.doc.model.relationships.len();
    let (pa, pb) = (centre(&h, &a), centre(&h, &b));
    drag(&mut h, pa, pb);
    let app = h.state();
    assert_eq!(app.session.doc.model.relationships.len(), n + 1, "the drawn connector is a model relationship");
    let c = app.session.doc.page().unwrap().connectors.last().unwrap();
    let rel = app.session.doc.model.relationship(c.relationship.as_deref().expect("bound")).unwrap();
    assert_eq!(rel.type_id, "uml.ControlFlow");
    assert_eq!(app.session.undo_label(), Some("Add Connector"));
}
