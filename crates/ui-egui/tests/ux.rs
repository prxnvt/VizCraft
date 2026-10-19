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

fn double_click(h: &mut Harness<'_, CraftApp>, p: Pos2) {
    // Let any earlier click age out, or egui counts these presses as part of it.
    h.run_steps(30);
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    for _ in 0..2 {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.run_steps(1);
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
}

fn to_screen(h: &Harness<'_, CraftApp>, p: vizcraft_diagram::Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

#[test]
fn double_clicking_a_wide_label_beside_a_thin_line_edits_it() {
    let mut h = from_template("flowchart.decision");
    let a = run(&mut h, "element.create", json!({"type": "flowchart.Process", "name": "Top", "x": 3000, "y": 3000}));
    let b = run(&mut h, "element.create", json!({"type": "flowchart.Process", "name": "Bottom", "x": 3000, "y": 3400}));
    let (a, b) = (a["shape"].as_str().unwrap().to_string(), b["shape"].as_str().unwrap().to_string());
    let r = run(&mut h, "insert.connect", json!({"from": a, "to": b}));
    let cid = r["connector"].as_str().unwrap().to_string();
    run(&mut h, "connector.setText", json!({"id": cid, "text": "A rather wide label for a thin line"}));
    run(&mut h, "view.pan", json!({"x": 3050, "y": 3250}));
    run(&mut h, "edit.selectNone", json!({}));
    let page = h.state().session.doc.page().unwrap();
    let g = vizcraft_diagram::connector_geometry(page, page.connector(&cid).unwrap());
    let mid = to_screen(&h, g.label_pos);
    // Well to the side of the vertical line, still inside the label's box.
    double_click(&mut h, mid + vec2(45.0, 0.0));
    let ed = h.state().canvas.editing.clone().expect("the label opens for editing");
    assert_eq!(ed.id(), cid);
    assert_eq!(ed.label, None);
    assert!(ed.text.starts_with("A rather wide"));
}

#[test]
fn double_clicking_near_an_association_end_edits_the_multiplicity() {
    let mut h = from_template("uml.classDiagram");
    let (cid, rid) = {
        let app = h.state();
        let doc = &app.session.doc;
        let c = doc
            .page()
            .unwrap()
            .connectors
            .iter()
            .find(|c| c.relationship.as_deref().and_then(|r| doc.model.relationship(r)).is_some_and(|r| r.type_id == "uml.Association"))
            .expect("the template draws an association");
        (c.id.clone(), c.relationship.clone().unwrap())
    };
    run(&mut h, "relationship.setProperty", json!({"id": rid, "key": "targetMultiplicity", "value": null}));
    run(&mut h, "connector.setEndLabels", json!({"ids": [cid], "end": ""}));
    let page = h.state().session.doc.page().unwrap();
    let g = vizcraft_diagram::connector_geometry(page, page.connector(&cid).unwrap());
    let (a, b) = (g.polyline[g.polyline.len() - 2], *g.polyline.last().unwrap());
    // A point 8 percent of the last segment back from the end shape, on the route.
    let near = vizcraft_diagram::Point::new(b.x + (a.x - b.x) * 0.08, b.y + (a.y - b.y) * 0.08);
    let c = near;
    run(&mut h, "view.pan", json!({"x": c.x, "y": c.y}));
    run(&mut h, "edit.selectNone", json!({}));
    let at = to_screen(&h, near);
    double_click(&mut h, at);
    let ed = h.state().canvas.editing.clone().expect("an empty end label opens for editing");
    assert!(ed.label.is_some());
    type_text(&mut h, "0..*");
    key(&mut h, Key::Enter);
    let app = h.state();
    let rel = app.session.doc.model.relationship(&rid).unwrap();
    assert_eq!(rel.properties.get("targetMultiplicity").and_then(|v| v.as_str()), Some("0..*"), "{:?}", rel.properties);
}

/// The screen point at the middle of row `row` (or of the whole body) of compartment `cid`, at
/// table column `col` when given, of shape `shape`.
fn band_point(h: &Harness<'_, CraftApp>, shape: &str, cid: &str, row: Option<usize>, col: Option<usize>) -> Pos2 {
    let s = h.state().session.doc.page().unwrap().shape(shape).unwrap().clone();
    let band = vizcraft_diagram::compartment::layout(&s)
        .into_iter()
        .find(|b| b.compartment.and_then(|i| s.compartments.get(i)).is_some_and(|c| c.id == cid))
        .expect("the compartment is laid out");
    let r = match row {
        Some(i) => band.rows[i],
        None => band.body,
    };
    let x = match col {
        Some(c) => (band.columns[c] + band.columns[c + 1]) / 2.0,
        None => r.center().x,
    };
    to_screen(h, s.transform().apply(vizcraft_diagram::Point::new(x, r.center().y)))
}

fn centre_view_on(h: &mut Harness<'_, CraftApp>, shape: &str) {
    let c = h.state().session.doc.page().unwrap().shape(shape).unwrap().center();
    run(h, "view.pan", json!({"x": c.x, "y": c.y}));
    run(h, "edit.selectNone", json!({}));
}

fn element_of(h: &Harness<'_, CraftApp>, shape: &str) -> String {
    h.state().session.doc.page().unwrap().shape(shape).unwrap().element.clone().unwrap()
}

#[test]
fn a_canvas_edit_of_the_attributes_band_writes_the_model_and_survives_the_next_command() {
    let mut h = from_template("uml.classDiagram");
    let class = shapes_of(&h, "uml.Class")[0].clone();
    let el = element_of(&h, &class);
    centre_view_on(&mut h, &class);
    let at = band_point(&h, &class, "attrs", None, None);
    double_click(&mut h, at);
    let ed = h.state().canvas.editing.clone().expect("the attributes band opens for editing");
    assert!(ed.model.is_some(), "it writes the model property");
    type_text(&mut h, "- total: Money [0..1]\n+ id: int");
    key(&mut h, Key::Enter);
    let attrs = h.state().session.doc.model.element(&el).unwrap().prop("attributes").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
    assert!(attrs.contains("total: Money"), "{attrs:?}");
    // The next command re-syncs from the model; the edit is still there.
    run(&mut h, "shape.move", json!({"ids": [class.clone()], "dx": 10, "dy": 0}));
    let s = h.state().session.doc.page().unwrap().shape(&class).unwrap();
    let rows = &s.compartments.iter().find(|c| c.id == "attrs").unwrap().rows;
    assert!(rows.iter().any(|r| r.contains("total")), "{rows:?}");
    assert_eq!(h.state().session.undo_label(), Some("Move"), "the edit was its own undo step");
}

#[test]
fn er_cells_edit_the_columns_in_place() {
    let mut h = from_template("er.schema");
    let table = shapes_of(&h, "er.Entity")[0].clone();
    centre_view_on(&mut h, &table);
    let s = h.state().session.doc.page().unwrap().shape(&table).unwrap().clone();
    let attrs_rows = s.compartments.iter().find(|c| c.id == "attrs").unwrap().rows.len();
    let keys_rows = s.compartments.iter().find(|c| c.id == "keys").unwrap().rows.len();
    assert!(attrs_rows > 0 && keys_rows > 0);
    // Type cell (third column) of the first plain column.
    let owner = element_of(&h, &table);
    let attr_ids = {
        let doc = &h.state().session.doc;
        let cat = vizcraft_notation::catalog();
        let el = doc.model.element(&owner).unwrap();
        let d = cat.compartments_in(&el.type_id, "").iter().find(|d| d.id == "attrs").unwrap();
        vizcraft_notation::sheet_pass::table_row_ids(doc, cat, el, d.query.as_ref().unwrap())
    };
    let at = band_point(&h, &table, "attrs", Some(0), Some(2));
    double_click(&mut h, at);
    assert!(h.state().canvas.editing.is_some(), "a cell opens an editor");
    type_text(&mut h, "uuid");
    key(&mut h, Key::Enter);
    let data_type = h.state().session.doc.model.element(&attr_ids[0]).unwrap().prop("dataType").and_then(|v| v.as_str().map(str::to_string));
    assert_eq!(data_type.as_deref(), Some("uuid"));
    // Name cell renames.
    let at = band_point(&h, &table, "attrs", Some(0), Some(1));
    double_click(&mut h, at);
    type_text(&mut h, "renamed_col");
    key(&mut h, Key::Enter);
    assert_eq!(h.state().session.doc.model.element(&attr_ids[0]).unwrap().name, "renamed_col");
    // A flag cell (the FK column of a plain column) toggles on the double-click; PK moves the row up.
    let at = band_point(&h, &table, "attrs", Some(0), Some(0));
    double_click(&mut h, at);
    assert!(h.state().canvas.editing.is_none(), "a flag cell opens no editor");
    let fk = h.state().session.doc.model.element(&attr_ids[0]).unwrap().prop("fk").and_then(|v| v.as_bool());
    assert_eq!(fk, Some(true));
    // PK: run the same gesture on the keys band's first row, which clears PK and moves it down.
    let keys_before = h.state().session.doc.page().unwrap().shape(&table).unwrap().compartments.iter().find(|c| c.id == "keys").unwrap().rows.len();
    let at = band_point(&h, &table, "keys", Some(0), Some(0));
    double_click(&mut h, at);
    let s = h.state().session.doc.page().unwrap().shape(&table).unwrap();
    let keys_after = s.compartments.iter().find(|c| c.id == "keys").unwrap().rows.len();
    assert_eq!(keys_after, keys_before - 1, "the row left the keys band");
}

#[test]
fn a_band_filled_from_properties_opens_the_inspector_instead_of_an_editor() {
    let mut h = from_template("uml.stateMachine");
    let state = shapes_of(&h, "uml.State").into_iter().find(|id| {
        let s = h.state().session.doc.page().unwrap().shape(id).unwrap();
        s.compartments.iter().any(|c| c.id == "behaviors")
    });
    let Some(state) = state else { panic!("a state with a behaviors band") };
    centre_view_on(&mut h, &state);
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": false}));
    let at = band_point(&h, &state, "behaviors", None, None);
    double_click(&mut h, at);
    assert!(h.state().canvas.editing.is_none(), "no editor that the next sync would overwrite");
    assert!(h.state().ui.panes.is_open("shapeData"), "the Inspector is open");
    assert_eq!(h.state().ui.inspector.tab, vizcraft_ui_egui::panes::InspectorTab::Properties);
}

fn click_with(h: &mut Harness<'_, CraftApp>, p: Pos2, m: Modifiers) {
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    h.event(Event::ModifiersChanged(m));
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: m });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: m });
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}

/// The screen point at the middle of connector `id`'s route.
fn connector_point(h: &Harness<'_, CraftApp>, id: &str) -> Pos2 {
    let page = h.state().session.doc.page().unwrap();
    let g = vizcraft_diagram::connector_geometry(page, page.connector(id).unwrap());
    let n = g.polyline.len();
    let (a, b) = (g.polyline[n / 2 - 1], g.polyline[n / 2]);
    to_screen(h, vizcraft_diagram::Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0))
}

fn steps_of(h: &Harness<'_, CraftApp>) -> Vec<u16> {
    h.state().session.doc.page().unwrap().connectors.iter().map(|c| c.step).collect()
}

#[test]
fn the_steps_tool_numbers_connectors_and_shift_click_clears() {
    let mut h = from_template("cloud.topology");
    let ids: Vec<String> = h.state().session.doc.page().unwrap().connectors.iter().map(|c| c.id.clone()).collect();
    assert!(ids.len() >= 3);
    run(&mut h, "connector.renumberSteps", json!({}));
    for id in &ids {
        run(&mut h, "connector.setStep", json!({"id": id, "step": null}));
    }
    let first_shape = h.state().session.doc.page().unwrap().connectors[0].start.shape().unwrap().to_string();
    centre_view_on(&mut h, &first_shape);
    run(&mut h, "ui.tool", json!({"tool": "steps"}));
    assert_eq!(h.state().tool.name(), "steps");
    // Click three connectors in an order of our choosing.
    for id in [&ids[2], &ids[0], &ids[1]] {
        run(&mut h, "view.zoomFit", json!({}));
        let p = connector_point(&h, id);
        click_with(&mut h, p, Modifiers::NONE);
    }
    let steps = steps_of(&h);
    assert_eq!((steps[2], steps[0], steps[1]), (1, 2, 3), "{steps:?}");
    // Shift-click clears one.
    run(&mut h, "view.zoomFit", json!({}));
    let p = connector_point(&h, &ids[0]);
    click_with(&mut h, p, Modifiers::SHIFT);
    assert_eq!(steps_of(&h)[0], 0);
    // Renumber closes the gap.
    run(&mut h, "connector.renumberSteps", json!({}));
    let steps = steps_of(&h);
    assert_eq!((steps[2], steps[1]), (1, 2), "{steps:?}");
    // Escape returns to the pointer.
    key(&mut h, Key::Escape);
    assert_eq!(h.state().tool.name(), "pointer");
}

#[test]
fn the_notation_tab_has_key_and_steps_groups() {
    let h = from_template("cloud.topology");
    let tabs = vizcraft_ui_egui::ribbon::tabs(h.state());
    let tab = tabs.iter().find(|t| t.name == "Cloud Topology").expect("a Cloud tab");
    let ids = tab.ids();
    for id in ["key.insert", "key.refresh", "key.remove", "ui.tool", "connector.renumberSteps", "connector.setStep"] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    assert!(tab.groups.iter().any(|g| g.label == "Key") && tab.groups.iter().any(|g| g.label == "Steps"));
}

#[test]
fn include_in_key_shrinks_the_key_in_one_undo_step() {
    use egui_kittest::kittest::Queryable;
    let mut h = from_template("cloud.topology");
    run(&mut h, "key.insert", json!({}));
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": true}));
    let kg = run(&mut h, "key.get", json!({}));
    let before = kg["rows"].as_array().unwrap().len();
    assert!(before > 1);
    let ty = kg["rows"].as_array().unwrap().iter().find(|r| r["kind"] == "type").expect("a type row")["ref"].as_str().unwrap().to_string();
    let shape = shapes_of(&h, &ty)[0].clone();
    select(&mut h, &[&shape]);
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Include in key").click();
    h.run_steps(3);
    let after = run(&mut h, "key.get", json!({}))["rows"].as_array().unwrap().len();
    assert!(after < before, "{before} -> {after}");
    assert_eq!(h.state().session.undo_label(), Some("Insert Key"));
}

/// The Model Tree pane drawn alone in a narrow panel, so queries only see its rows.
fn tree_pane(template: &str) -> Harness<'static, CraftApp> {
    let mut s = Session::new(vizcraft_engine::Document::new());
    s.run("template.new", &json!({"template": template})).unwrap();
    s.run("edit.selectNone", &json!({})).unwrap();
    let mut h = Harness::builder().with_size(vec2(360.0, 900.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            let spec = vizcraft_ui_egui::panes::spec("navigation").expect("the pane exists");
            (spec.show)(app, ui);
        },
        CraftApp::new(s, Services::default()),
    );
    h.run_steps(4);
    h
}

#[test]
fn model_tree_lists_relationships_under_an_open_element_and_a_click_selects_the_connector() {
    use egui_kittest::kittest::Queryable;
    let mut h = tree_pane("uml.classDiagram");
    let classes = shapes_of(&h, "uml.Class");
    let el = element_of(&h, &classes[0]);
    // Closed: no relationship rows. Open: the element's relationships follow, with arrows.
    let r = run(&mut h, "ui.tree", json!({}));
    assert!(r["relationships"].as_array().unwrap().is_empty(), "{r}");
    let r = run(&mut h, "ui.tree", json!({"expand": [el]}));
    let rows = r["relationships"].as_array().unwrap().clone();
    assert!(!rows.is_empty(), "{r}");
    assert!(rows.iter().all(|x| x["element"] == json!(el)));
    h.run_steps(3);
    let arrow = if rows[0]["direction"] == "out" { "-> " } else { "<- " };
    h.get_all_by_label_contains(arrow).next().unwrap().click();
    h.run_steps(3);
    assert_eq!(selected(&h).len(), 1);
    assert!(h.state().session.doc.page().unwrap().connector(&selected(&h)[0]).is_some(), "the click selects a connector");
    // Context mode keeps the selected element's relationships showing.
    run(&mut h, "edit.select", json!({"ids": [classes[0]]}));
    let r = run(&mut h, "ui.tree", json!({"mode": "context"}));
    assert!(!r["relationships"].as_array().unwrap().is_empty(), "{r}");
}

fn attributes_of(h: &Harness<'_, CraftApp>, el: &str) -> Vec<String> {
    h.state()
        .session
        .doc
        .model
        .element(el)
        .and_then(|e| e.prop("attributes"))
        .map(|v| v.display().lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn open_inspector_on(h: &mut Harness<'_, CraftApp>, shape: &str) {
    run(h, "ui.pane", json!({"name": "shapeData", "value": true}));
    run(h, "ui.inspector", json!({"tab": "properties"}));
    select(h, &[shape]);
    h.run_steps(3);
}

#[test]
fn members_add_move_and_delete_are_one_undo_step_each() {
    use egui_kittest::kittest::Queryable;
    let mut h = from_template("uml.classDiagram");
    let class = shapes_of(&h, "uml.Class")[0].clone();
    let el = element_of(&h, &class);
    run(&mut h, "uml.addMember", json!({"element": el, "list": "attributes", "text": "+a: int"}));
    run(&mut h, "uml.addMember", json!({"element": el, "list": "attributes", "text": "+b: int"}));
    open_inspector_on(&mut h, &class);
    let r = vizcraft_ui_egui::panes::inspector_fields(h.state());
    let members = &r["members"];
    assert_eq!(members["element"], json!(el), "{members}");
    assert!(members["lists"]["attributes"].as_array().unwrap().len() >= 3);
    let rows = attributes_of(&h, &el);
    let n = rows.len();
    // Move the last row up: the Up button of the last row.
    let ups: Vec<_> = h.query_all_by_label("Move up").collect();
    assert!(ups.len() >= n, "an Up button per row");
    ups[1].click();
    h.run_steps(3);
    assert_eq!(h.state().session.undo_label(), Some("Move Member"));
    let moved = attributes_of(&h, &el);
    assert_eq!(moved[0], rows[1], "the second row moved up");
    // Delete the first row.
    h.query_all_by_label("Remove member").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().session.undo_label(), Some("Remove Member"));
    assert_eq!(attributes_of(&h, &el).len(), n - 1);
    // Edit a row's text: focus its field, type, Enter.
    let first = attributes_of(&h, &el)[0].clone();
    let body = first.trim_start_matches(['+', '-', '#', '~']).to_string();
    let field = h.get_all_by_value(body.as_str()).next().expect("the row's field");
    field.focus();
    h.get_all_by_value(body.as_str()).next().unwrap().type_text("2");
    h.run_steps(2);
    key(&mut h, Key::Enter);
    assert_eq!(h.state().session.undo_label(), Some("Set Member"));
    assert!(attributes_of(&h, &el)[0].ends_with('2'), "{:?}", attributes_of(&h, &el));
}

#[test]
fn columns_grid_adds_a_column_toggles_flags_and_deletes() {
    use egui_kittest::kittest::Queryable;
    let mut h = from_template("er.schema");
    let table = shapes_of(&h, "er.Entity")[0].clone();
    let el = element_of(&h, &table);
    open_inspector_on(&mut h, &table);
    let r = vizcraft_ui_egui::panes::inspector_fields(h.state());
    let cols = r["columns"]["rows"].as_array().unwrap().clone();
    assert!(cols.len() >= 2, "{r}");
    assert_eq!(r["columns"]["element"], json!(el));
    // Drawing order: key rows first.
    assert_eq!(cols[0]["pk"], true);
    // Add through the field: it follows the two cells (name, type) of every column.
    let n = cols.len();
    {
        let fields: Vec<_> = h.query_all_by_role(egui::accesskit::Role::TextInput).collect();
        let first = cols[0]["name"].as_str().unwrap();
        let base = fields.iter().position(|f| f.value().as_deref() == Some(first)).expect("the first column's name cell");
        fields.get(base + 2 * n).expect("the Add column field").focus();
    }
    h.run_steps(2);
    h.event(Event::Text("loyalty".into()));
    h.run_steps(2);
    key(&mut h, Key::Enter);
    let names: Vec<String> = vizcraft_ui_egui::panes::inspector_fields(h.state())["columns"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"loyalty".to_string()), "{names:?}");
    assert_eq!(h.state().session.undo_label(), Some("Add Column"));
    // A flag checkbox toggles through element.setProperty.
    select(&mut h, &[&table]);
    let first_fk = vizcraft_ui_egui::panes::inspector_fields(h.state())["columns"]["rows"][0]["fk"].as_bool().unwrap();
    let label = format!("FK of {}", cols[0]["name"].as_str().unwrap());
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, &label).click();
    h.run_steps(3);
    let now = vizcraft_ui_egui::panes::inspector_fields(h.state())["columns"]["rows"][0]["fk"].as_bool().unwrap();
    assert_eq!(now, !first_fk);
    // Delete removes the column from the model.
    let before = vizcraft_ui_egui::panes::inspector_fields(h.state())["columns"]["rows"].as_array().unwrap().len();
    h.query_all_by_label_contains("Delete column").next().expect("a delete button").click();
    h.run_steps(3);
    select(&mut h, &[&table]);
    let after = vizcraft_ui_egui::panes::inspector_fields(h.state())["columns"]["rows"].as_array().unwrap().len();
    assert_eq!(after, before - 1);
}

#[test]
fn relationship_ends_edit_navigability_then_fix_ends() {
    use egui_kittest::kittest::Queryable;
    let mut h = from_template("uml.classDiagram");
    let (cid, rid) = {
        let app = h.state();
        let doc = &app.session.doc;
        let c = doc
            .page()
            .unwrap()
            .connectors
            .iter()
            .find(|c| c.relationship.as_deref().and_then(|r| doc.model.relationship(r)).is_some_and(|r| r.type_id == "uml.Association"))
            .unwrap();
        (c.id.clone(), c.relationship.clone().unwrap())
    };
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": true}));
    select(&mut h, &[&cid]);
    h.run_steps(3);
    let r = vizcraft_ui_egui::panes::inspector_fields(h.state());
    assert_eq!(r["relationship"]["id"], json!(rid), "{r}");
    let ends = &r["relationship"]["ends"];
    assert!(ends["start"].as_array().unwrap().iter().any(|k| k == "sourceMultiplicity"), "{ends}");
    assert!(ends["end"].as_array().unwrap().iter().any(|k| k == "targetRole"), "{ends}");
    // The section shows the end elements' names as headings.
    let src = r["relationship"]["sourceName"].as_str().unwrap().to_string();
    assert!(h.query_all_by_label(&src).next().is_some(), "{src} heads a column");
    // A multiplicity that does not read is still written, with its reason shown under the field.
    run(&mut h, "relationship.setProperty", json!({"id": rid, "key": "sourceMultiplicity", "value": "3..1"}));
    h.run_steps(3);
    assert!(h.query_by_label_contains("lower bound").is_some(), "the reason is shown");
    run(&mut h, "relationship.setProperty", json!({"id": rid, "key": "sourceMultiplicity", "value": null}));
    // Navigability no longer matches the drawn ends: the rule reports it and Fix ends repairs it.
    run(&mut h, "relationship.setProperty", json!({"id": rid, "key": "navigability", "value": "both"}));
    h.run_steps(3);
    let r = vizcraft_ui_egui::panes::inspector_fields(h.state());
    assert!(r["relationship"]["fixEnds"].is_string(), "an end-marker issue offers a fix: {r}");
    h.get_by_label("Fix ends").click();
    h.run_steps(3);
    let r = vizcraft_ui_egui::panes::inspector_fields(h.state());
    assert!(r["relationship"]["fixEnds"].is_null(), "the fix removed the issue: {r}");
    let c = h.state().session.doc.page().unwrap().connector(&cid).unwrap().clone();
    assert!(c.start_arrow != vizcraft_diagram::Arrow::None && c.end_arrow != vizcraft_diagram::Arrow::None, "both ends open now");
}

fn menu_labels(h: &Harness<'_, CraftApp>) -> Vec<String> {
    let r = vizcraft_ui_egui::control::inspect(h.state(), &h.ctx);
    r["menu"]["rows"].as_array().map(|rows| rows.iter().map(|x| x["label"].as_str().unwrap_or_default().to_string()).collect()).unwrap_or_default()
}

fn positions(h: &Harness<'_, CraftApp>) -> Vec<(f64, f64)> {
    h.state().session.doc.page().unwrap().shapes.iter().map(|s| (s.x, s.y)).collect()
}

#[test]
fn re_layout_lists_the_notations_layouts_first_and_the_main_click_lays_out() {
    use egui_kittest::kittest::Queryable;
    let mut h = from_template("uml.stateMachine");
    run(&mut h, "ui.tab", json!({"tab": "Design"}));
    let choices = vizcraft_ui_egui::notation_layout::layout_choices(h.state());
    assert!(!choices.is_empty(), "the state machine templates declare a layout");
    h.get_by_label("Re-Layout options").click();
    h.run_steps(4);
    let rows = menu_labels(&h);
    assert_eq!(rows[0], choices[0].label, "the notation's layouts come first: {rows:?}");
    assert!(rows.iter().any(|r| r == "Radial") && rows.iter().any(|r| r.starts_with("Tree, ")), "the generic list follows: {rows:?}");
    assert!(rows.iter().position(|r| r == &choices[0].label) < rows.iter().position(|r| r == "Radial"));
    key(&mut h, Key::Escape);
    h.run_steps(2);
    // Scramble, then the main click runs the first notation layout.
    for s in 0..3 {
        let id = h.state().session.doc.page().unwrap().shapes[s].id.clone();
        run(&mut h, "shape.move", json!({"ids": [id], "dx": 300 * (s as i64 + 1), "dy": -200}));
    }
    let scrambled = positions(&h);
    h.get_by_label("Re-Layout").click();
    h.run_steps(4);
    assert_ne!(positions(&h), scrambled, "the main click ran the layout");
    assert_eq!(h.state().session.undo_label(), Some("Re-Layout"));
}

#[test]
fn insert_direction_menu_shows_what_auto_resolves_to_and_sets_the_direction() {
    use egui_kittest::kittest::Queryable;
    let mut h = from_template("flowchart.decision");
    run(&mut h, "ui.tab", json!({"tab": "Design"}));
    h.get_by_label("Insert Direction").click();
    h.run_steps(4);
    let rows = menu_labels(&h);
    assert_eq!(rows, vec!["Auto (down)", "Right", "Down", "Left", "Up"], "a flowchart flows downwards");
    h.get_by_label("Left").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.insert_direction, "left");
}

fn group_labels(app: &CraftApp, tab: &str) -> Vec<&'static str> {
    vizcraft_ui_egui::ribbon::tabs(app).iter().find(|t| t.name == tab).map(|t| t.groups.iter().map(|g| g.label).collect()).unwrap_or_default()
}

#[test]
fn the_process_tab_drops_diagram_validation_where_the_notation_tab_has_it() {
    let uml = from_template("uml.classDiagram");
    assert!(!group_labels(uml.state(), "Process").contains(&"Diagram Validation"), "{:?}", group_labels(uml.state(), "Process"));
    assert!(group_labels(uml.state(), "Process").contains(&"Subprocess"), "Subprocess stays");
    let tab = vizcraft_ui_egui::ribbon::tabs(uml.state()).into_iter().find(|t| t.name == "UML").expect("the UML tab");
    let check = tab.groups.iter().find(|g| g.label == "Check").expect("a Check group");
    let mut ids = Vec::new();
    check.controls.iter().for_each(|c| c.ids(&mut ids));
    for id in ["validate.run", "validate.setProfile", "ui.dock", "validate.next", "validate.previous"] {
        assert!(ids.contains(&id), "{id} in the notation Check group: {ids:?}");
    }
    // A free page keeps validation on the Process tab.
    let sample = harness_with(Session::new(vizcraft_engine::sample::sample_document()));
    assert!(group_labels(sample.state(), "Process").contains(&"Diagram Validation"));
}
