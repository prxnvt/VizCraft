//! Sequence pages under egui_kittest, on the `uml.sequence` template: lifelines hit on the stem
//! and through their bars, reordering messages and columns by dragging, drawing a message from
//! stem to stem, dropping a lifeline master, wrapping from the Sequence tab, editing a guard in
//! place, the Alt+arrow moves (and that they stay out of a label being typed), the Inspector's
//! Sort field, no resize or rotate handles on what settle places, and the tab showing only on a
//! Sequence page. Assertions read the session and `ui.inspect`, never pixels.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_diagram::sequence::Part;
use vizcraft_diagram::{Hit, Point, Shape};
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::canvas::interact::{self, drag};
use vizcraft_ui_egui::{CraftApp, Services};

fn harness(sequence: bool) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(Document::new()), Services::default());
    app.run("ui.pane", json!({"name": "shapeData", "value": true})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    if sequence {
        run(&mut h, "template.new", json!({"template": "uml.sequence"}));
        run(&mut h, "view.zoomFit", json!({}));
        h.run_steps(4);
    }
    h
}

fn run(h: &mut Harness<'_, CraftApp>, id: &str, v: Value) -> Value {
    let r = h.state_mut().run(id, v).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(2);
    r
}

fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

fn shape(h: &Harness<'_, CraftApp>, id: &str) -> Shape {
    h.state().session.doc.page().unwrap().shape(id).unwrap().clone()
}

fn cx(s: &Shape) -> f64 {
    s.x + s.w / 2.0
}

/// The row a message is drawn on.
fn row_of(h: &Harness<'_, CraftApp>, id: &str) -> f64 {
    let page = h.state().session.doc.page().unwrap();
    vizcraft_diagram::connector_geometry(page, page.connector(id).unwrap()).polyline[0].y
}

fn order(h: &mut Harness<'_, CraftApp>) -> Vec<String> {
    let o = h.state_mut().run("sequence.order", json!({})).unwrap();
    o["events"].as_array().unwrap().iter().map(|e| format!("{} {}", e["kind"].as_str().unwrap(), e["id"].as_str().unwrap())).collect()
}

fn press(h: &mut Harness<'_, CraftApp>, p: Pos2, pressed: bool) {
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.run_steps(1);
}

/// Drag from `from` to `to`; `mid` sees the app halfway, with the button still down.
fn drag_with(h: &mut Harness<'_, CraftApp>, from: Point, to: Point, mid: impl FnOnce(&CraftApp)) {
    let (from, to) = (screen(h, from), screen(h, to));
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    press(h, from, true);
    for i in 1..=8 {
        h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
        h.run_steps(1);
    }
    mid(h.state());
    press(h, to, false);
    h.run_steps(2);
}

fn drag_to(h: &mut Harness<'_, CraftApp>, from: Point, to: Point) {
    drag_with(h, from, to, |_| {});
}

fn click(h: &mut Harness<'_, CraftApp>, p: Point) {
    let p = screen(h, p);
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    press(h, p, true);
    press(h, p, false);
    h.run_steps(2);
}

fn double_click(h: &mut Harness<'_, CraftApp>, p: Point) {
    let p = screen(h, p);
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    for _ in 0..2 {
        press(h, p, true);
        press(h, p, false);
    }
    h.run_steps(2);
}

fn key(h: &mut Harness<'_, CraftApp>, k: Key, m: Modifiers) {
    h.event(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m });
    h.event(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m });
    h.run_steps(2);
}

fn dragging(app: &CraftApp) -> Value {
    vizcraft_ui_egui::canvas::summary(app)["dragging"].clone()
}

#[test]
fn a_stem_or_a_bar_selects_its_lifeline_and_lifelines_offer_no_points() {
    let mut h = harness(true);
    let api = shape(&h, "api");
    // Low on the stem, below the fragment and the bar.
    click(&mut h, Point::new(cx(&api) + 1.0, api.y + api.h - 4.0));
    assert_eq!(h.state().session.sel.items, vec!["api".to_string()]);
    run(&mut h, "edit.selectNone", json!({}));
    let page = h.state().session.doc.page().unwrap().clone();
    let bar = page.shapes.iter().find(|s| matches!(&s.sequence, Some(Part::Activation { lifeline, .. }) if lifeline == "api")).unwrap();
    let y = (row_of(&h, "m2") + row_of(&h, "m3")) / 2.0;
    click(&mut h, Point::new(cx(bar), y));
    assert_eq!(h.state().session.sel.items, vec!["api".to_string()]);
    let db = shape(&h, "db");
    for p in db.connection_points_world() {
        assert_eq!(vizcraft_diagram::connection_point_at(&db, p, 10.0), None);
    }
    assert_eq!(vizcraft_ui_egui::canvas::summary(h.state())["sequence"]["active"], true);
}

#[test]
fn dragging_a_message_reorders_it_in_one_undo_step() {
    let mut h = harness(true);
    let before = order(&mut h);
    let page = h.state().session.doc.page().unwrap().clone();
    let g = vizcraft_diagram::connector_geometry(&page, page.connector("m3").unwrap());
    let from = Point::new((g.polyline[0].x + g.polyline[1].x) / 2.0, g.polyline[0].y);
    let to = Point::new(from.x, (row_of(&h, "m1") + row_of(&h, "m2")) / 2.0);
    drag_with(&mut h, from, to, |app| {
        assert_eq!(dragging(app), "reorder");
        let drop = &vizcraft_ui_egui::canvas::summary(app)["sequence"]["drop"];
        assert_eq!(drop["before"], "m2", "{drop}");
    });
    assert_eq!(order(&mut h)[..3], ["message m1", "message m3", "message m2"].map(String::from));
    run(&mut h, "edit.undo", json!({}));
    assert_eq!(order(&mut h), before);
}

#[test]
fn dragging_a_lifeline_swaps_columns_and_keeps_heads_on_their_row() {
    let mut h = harness(true);
    let (api, db) = (shape(&h, "api"), shape(&h, "db"));
    let heads: Vec<f64> = ["client", "api", "db"].iter().map(|id| shape(&h, id).y).collect();
    let from = Point::new(cx(&db), db.y + 20.0);
    drag_with(&mut h, from, Point::new(cx(&api) - 60.0, from.y + 15.0), |app| assert_eq!(dragging(app), "column"));
    let (api, db) = (shape(&h, "api"), shape(&h, "db"));
    assert!(cx(&db) < cx(&api), "{} {}", cx(&db), cx(&api));
    let after: Vec<f64> = ["client", "api", "db"].iter().map(|id| shape(&h, id).y).collect();
    assert_eq!(after, heads);
    // A press on a head corner, where a resize handle would be, moves the column too.
    let client = shape(&h, "client");
    run(&mut h, "edit.select", json!({"ids": ["client"]}));
    drag_with(&mut h, Point::new(client.x, client.y), Point::new(client.x - 30.0, client.y), |app| assert_eq!(dragging(app), "column"));
}

#[test]
fn the_connector_tool_draws_a_message_from_stem_to_stem() {
    let mut h = harness(true);
    run(&mut h, "ui.tool", json!({"tool": "connector", "kind": "uml.asyncMessage"}));
    let (api, db) = (shape(&h, "api"), shape(&h, "db"));
    let y = (row_of(&h, "m1") + row_of(&h, "m2")) / 2.0;
    drag_with(&mut h, Point::new(cx(&api), y), Point::new(cx(&db) + 1.0, y), |app| assert_eq!(dragging(app), "message"));
    let o = h.state_mut().run("sequence.order", json!({})).unwrap();
    let e = &o["events"][1];
    assert_eq!(
        (e["kind"].as_str(), e["from"].as_str(), e["to"].as_str(), e["sort"].as_str()),
        (Some("message"), Some("api"), Some("db"), Some("async"))
    );
    let id = e["id"].as_str().unwrap().to_string();
    assert_eq!(h.state().canvas.editing.as_ref().map(|ed| ed.id().to_string()), Some(id), "the label editor opens");
    // A release on empty page adds nothing.
    let n = order(&mut h).len();
    key(&mut h, Key::Escape, Modifiers::NONE);
    drag_to(&mut h, Point::new(cx(&api), y), Point::new(cx(&db) + 200.0, y));
    assert_eq!(order(&mut h).len(), n);
}

#[test]
fn a_dropped_lifeline_master_joins_at_its_column_and_edits_its_name() {
    let mut h = harness(true);
    let (api, db) = (shape(&h, "api"), shape(&h, "db"));
    vizcraft_ui_egui::canvas::insert_master(h.state_mut(), "uml.lifeline", Point::new((cx(&api) + cx(&db)) / 2.0, api.y + 20.0));
    h.run_steps(2);
    let page = h.state().session.doc.page().unwrap().clone();
    let new: Vec<&Shape> = page
        .shapes
        .iter()
        .filter(|s| matches!(s.sequence, Some(Part::Lifeline { .. })) && !["client", "api", "db"].contains(&s.id.as_str()))
        .collect();
    let [one] = new.as_slice() else { panic!("one new lifeline: {new:?}") };
    let (api, db) = (shape(&h, "api"), shape(&h, "db"));
    assert!(cx(&api) < cx(one) && cx(one) < cx(&db));
    assert_eq!(h.state().canvas.editing.as_ref().map(|e| e.id().to_string()), Some(one.id.clone()));
}

#[test]
fn wrap_from_the_sequence_tab_puts_an_opt_around_the_selection() {
    let mut h = harness(true);
    run(&mut h, "edit.select", json!({"ids": ["m2", "m3"]}));
    h.get_by_label("Sequence tab").click();
    h.run_steps(3);
    h.get_by_label("Wrap in Fragment").click();
    h.run_steps(3);
    h.get_by_label("opt").click();
    h.run_steps(3);
    let o = order(&mut h);
    let at = o.iter().position(|e| e == "message m2").unwrap();
    assert!(o[at - 1].starts_with("open ") && o[at + 1] == "message m3" && o[at + 2].starts_with("close "), "{o:?}");
    let f = o[at - 1].trim_start_matches("open ").to_string();
    assert!(matches!(shape(&h, &f).sequence, Some(Part::Fragment { operator: vizcraft_diagram::sequence::Operator::Opt, .. })));
}

#[test]
fn a_double_click_edits_the_guard_under_the_pointer() {
    let mut h = harness(true);
    let f = shape(&h, "f1");
    let Some(Part::Fragment { operands, .. }) = &f.sequence else { panic!() };
    let margin = f.container.as_ref().unwrap().margin;
    let p = Point::new(f.x + margin + 10.0, f.y + operands[1].at + 10.0);
    double_click(&mut h, p);
    assert_eq!(h.state().canvas.sequence.guard.as_ref().map(|g| g.operand), Some(1));
    h.event(Event::Text("missing".into()));
    h.run_steps(2);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.state().canvas.sequence.guard.is_none());
    let Some(Part::Fragment { operands, .. }) = shape(&h, "f1").sequence else { panic!() };
    assert_eq!(operands.iter().map(|o| o.guard.as_str()).collect::<Vec<_>>(), ["found", "missing"]);
    // Escape cancels.
    double_click(&mut h, p);
    h.event(Event::Text("nope".into()));
    h.run_steps(2);
    key(&mut h, Key::Escape, Modifiers::NONE);
    let Some(Part::Fragment { operands, .. }) = shape(&h, "f1").sequence else { panic!() };
    assert_eq!(operands[1].guard, "missing");
}

#[test]
fn alt_arrows_move_the_selection_but_not_while_a_label_is_typed() {
    let mut h = harness(true);
    run(&mut h, "edit.select", json!({"ids": ["m1"]}));
    key(&mut h, Key::ArrowDown, Modifiers::ALT);
    assert_eq!(order(&mut h)[..2], ["message m2", "message m1"].map(String::from));
    key(&mut h, Key::ArrowUp, Modifiers::ALT);
    assert_eq!(order(&mut h)[..2], ["message m1", "message m2"].map(String::from));
    run(&mut h, "edit.select", json!({"ids": ["db"]}));
    key(&mut h, Key::ArrowLeft, Modifiers::ALT);
    assert!(cx(&shape(&h, "db")) < cx(&shape(&h, "api")));
    // While a label is being typed the field keeps Alt+Down.
    interact::start_editing(h.state_mut(), Hit::Connector("m1".into()));
    h.run_steps(3);
    let before = order(&mut h);
    key(&mut h, Key::ArrowDown, Modifiers::ALT);
    assert_eq!(order(&mut h), before);
    assert!(h.state().canvas.editing.is_some());
}

#[test]
fn the_inspector_sort_field_runs_set_sort() {
    let mut h = harness(true);
    run(&mut h, "edit.select", json!({"ids": ["m2"]}));
    h.run_steps(3);
    // The Sort field is the one combo box the Inspector shows for a message; it reads "sync".
    h.get_by_role(Role::ComboBox).click();
    h.run_steps(3);
    h.get_by_label("async").click();
    h.run_steps(3);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.connector("m2").unwrap().message.as_ref().map(|m| m.sort.name()), Some("async"));
    assert_eq!(h.state().session.undo_label(), Some("Message Sort"));
}

#[test]
fn lifelines_and_fragments_have_no_resize_or_rotate_handles() {
    let h = harness(true);
    for id in ["api", "client", "f1"] {
        let s = shape(&h, id);
        assert!(!drag::rotate_allowed(&s), "{id}");
        assert!((0..interact::HANDLES.len()).all(|i| !drag::handle_allowed(&s, i)), "{id}");
    }
    let page = h.state().session.doc.page().unwrap();
    let bar = page.shapes.iter().find(|s| matches!(s.sequence, Some(Part::Activation { .. }))).unwrap();
    assert!(!drag::handle_allowed(bar, 4));
}

#[test]
fn the_sequence_tab_shows_only_on_a_sequence_page() {
    let mut h = harness(false);
    assert!(h.query_by_label("Sequence tab").is_none());
    run(&mut h, "template.new", json!({"template": "uml.sequence"}));
    h.run_steps(3);
    assert!(h.query_by_label("Sequence tab").is_some());
    // The UI commands say why they do nothing off a Sequence page.
    let mut h = harness(false);
    assert_eq!(run(&mut h, "ui.sequenceLifeline", json!({})), json!({"id": null}));
    let mut h = harness(true);
    run(&mut h, "edit.select", json!({"ids": ["api"]}));
    let r = run(&mut h, "ui.sequenceNote", json!({}));
    let id = r["id"].as_str().unwrap().to_string();
    assert!(matches!(shape(&h, &id).sequence, Some(Part::Note { .. })));
}
