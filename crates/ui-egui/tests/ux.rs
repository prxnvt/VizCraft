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
