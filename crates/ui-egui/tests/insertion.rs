//! Insertion on the canvas under egui_kittest with real pointer and key events: hover handles,
//! a handle click (one suggestion inserts at once, several open the popover), the magic
//! connector, Alt+Shift+Arrow chains, the insert picker and the command palette selecting an
//! element. Assertions read the session, never pixels.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_diagram::{End, Point};
use vizcraft_engine::Session;
use vizcraft_engine::suggest::Direction;
use vizcraft_ui_egui::canvas::interact::insertion::{handle_centre, screen_frame};
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

/// A blank document with a Terminator element drawn and the view on it; returns the harness and
/// the shape id.
fn flowchart() -> (Harness<'static, CraftApp>, String) {
    let mut s = Session::new(vizcraft_engine::Document::new());
    let r = s.run("element.create", &json!({"type": "flowchart.Terminator", "name": "Start", "x": 300, "y": 200})).unwrap();
    let shape = r["shape"].as_str().unwrap().to_string();
    s.run("edit.selectNone", &json!({})).unwrap();
    let mut h = harness_with(s);
    let c = h.state().session.doc.page().unwrap().shape(&shape).unwrap().center();
    h.state_mut().run("view.pan", json!({"x": c.x, "y": c.y})).unwrap();
    h.run_steps(2);
    (h, shape)
}

fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

fn centre(h: &Harness<'_, CraftApp>, id: &str) -> Pos2 {
    screen(h, h.state().session.doc.page().unwrap().shape(id).unwrap().center())
}

fn handle(h: &Harness<'_, CraftApp>, id: &str, d: Direction) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).unwrap();
    handle_centre(screen_frame(app, &vp, id).unwrap(), d)
}

/// Rest the pointer on shape `id` long enough for its handles to show.
fn rest_on(h: &mut Harness<'_, CraftApp>, id: &str) {
    let p = centre(h, id);
    h.event(Event::PointerMoved(p));
    h.run_steps(30);
}

fn click(h: &mut Harness<'_, CraftApp>, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

fn key(h: &mut Harness<'_, CraftApp>, k: Key, m: Modifiers) {
    h.event(Event::ModifiersChanged(m));
    h.event(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m });
    h.event(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m });
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

fn type_text(h: &mut Harness<'_, CraftApp>, t: &str) {
    h.event(Event::Text(t.to_string()));
    h.run_steps(2);
}

fn shapes(h: &Harness<'_, CraftApp>) -> usize {
    h.state().session.doc.page().unwrap().shapes.len()
}

#[test]
fn handles_appear_after_the_pointer_rests_on_a_shape() {
    let (mut h, t) = flowchart();
    h.event(Event::PointerMoved(centre(&h, &t)));
    h.run_steps(5);
    assert_eq!(h.state().insertion.shown, None, "not before the delay");
    h.run_steps(25);
    assert_eq!(h.state().insertion.shown.as_deref(), Some(t.as_str()));
    // Moving onto a handle keeps them; moving away to empty canvas hides them.
    let right = handle(&h, &t, Direction::Right);
    h.event(Event::PointerMoved(right));
    h.run_steps(2);
    assert_eq!(h.state().insertion.shown.as_deref(), Some(t.as_str()));
    h.event(Event::PointerMoved(right + vec2(200.0, 200.0)));
    h.run_steps(2);
    assert_eq!(h.state().insertion.shown, None);
    // The AutoConnect aid turns them off.
    h.state_mut().run("view.autoConnect", json!({"value": false})).unwrap();
    rest_on(&mut h, &t);
    assert_eq!(h.state().insertion.shown, None);
}

#[test]
fn a_handle_click_inserts_the_one_suggestion_or_picks_from_the_popover() {
    // A free shape has one suggestion: the click inserts it at once.
    let mut h = harness_with(Session::new(vizcraft_engine::sample::sample_document()));
    let n = shapes(&h);
    let first = h.state().session.doc.page().unwrap().shapes[0].id.clone();
    rest_on(&mut h, &first);
    let p = handle(&h, &first, Direction::Left);
    click(&mut h, p);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.shapes.len(), n + 1);
    let new = page.shapes.last().unwrap();
    assert_eq!(new.stencil, "flowchart.terminator");
    assert!(new.x + new.w <= page.shape(&first).unwrap().x, "to the left");
    let c = page.connectors.last().unwrap();
    assert_eq!((c.start.clone(), c.end.clone()), (End::dynamic(first.clone()), End::dynamic(new.id.clone())));
    assert_eq!(h.state().session.sel.items, vec![new.id.clone()]);

    // A Terminator element has several: the popover lists them and a row inserts that type.
    let (mut h, t) = flowchart();
    rest_on(&mut h, &t);
    let p = handle(&h, &t, Direction::Down);
    click(&mut h, p);
    assert!(h.state().insertion.popover.is_some(), "several suggestions open the popover");
    assert_eq!(shapes(&h), 1);
    // Let the popover settle where it is placed before aiming at a row.
    h.run_steps(2);
    h.get_by_label("Decision (Flow)").click();
    h.run_steps(3);
    let app = h.state();
    assert!(app.insertion.popover.is_none());
    let page = app.session.doc.page().unwrap();
    assert_eq!(page.shapes.len(), 2);
    let new = page.shapes.last().unwrap();
    let e = app.session.doc.model.element(new.element.as_deref().unwrap()).unwrap();
    assert_eq!(e.type_id, "flowchart.Decision");
    assert!(new.y > page.shape(&t).unwrap().y, "below");
    assert_eq!(app.session.doc.model.relationships.len(), 1);
}

#[test]
fn the_magic_connector_connects_two_shapes_with_the_one_legal_choice() {
    let (mut h, t) = flowchart();
    let r = h.state_mut().run("element.create", json!({"type": "flowchart.Process", "x": 600, "y": 200})).unwrap();
    let p = r["shape"].as_str().unwrap().to_string();
    h.state_mut().run("edit.selectNone", json!({})).unwrap();
    h.run_steps(2);
    rest_on(&mut h, &t);
    let from = handle(&h, &t, Direction::Right);
    let to = centre(&h, &p);
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=8 {
        h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
        h.run_steps(1);
    }
    assert!(h.state().insertion.drag.is_some(), "dragging a handle is the magic connector");
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    let app = h.state();
    let page = app.session.doc.page().unwrap();
    assert_eq!(page.shapes.len(), 2, "no new shape");
    let c = page.connectors.last().expect("a connector");
    assert_eq!((c.start.clone(), c.end.clone()), (End::dynamic(t.clone()), End::dynamic(p.clone())));
    assert_eq!(app.session.doc.model.relationships.len(), 1);
}

#[test]
fn alt_shift_right_chains_steps_from_the_selection() {
    let (mut h, t) = flowchart();
    h.state_mut().run("edit.select", json!({"ids": [t]})).unwrap();
    h.run_steps(1);
    let alt_shift = Modifiers { alt: true, shift: true, ..Modifiers::NONE };
    key(&mut h, Key::ArrowRight, alt_shift);
    key(&mut h, Key::ArrowRight, alt_shift);
    let app = h.state();
    let page = app.session.doc.page().unwrap();
    assert_eq!((page.shapes.len(), page.connectors.len()), (3, 2));
    let xs: Vec<f64> = page.shapes.iter().map(|s| s.x).collect();
    assert!(xs[0] < xs[1] && xs[1] < xs[2], "each step to the right of the last: {xs:?}");
    assert_eq!(app.session.sel.items, vec![page.shapes[2].id.clone()], "the newest step is selected");
    assert_eq!(app.session.doc.model.relationships.len(), 2);
}

#[test]
fn double_click_on_empty_canvas_opens_the_picker_and_enter_inserts() {
    let (mut h, t) = flowchart();
    let spot = centre(&h, &t) + vec2(0.0, 250.0);
    h.event(Event::PointerMoved(spot));
    h.run_steps(1);
    for _ in 0..2 {
        h.event(Event::PointerButton { pos: spot, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.run_steps(1);
        h.event(Event::PointerButton { pos: spot, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
    assert!(h.state().insertion.picker.is_some(), "the picker opens");
    type_text(&mut h, "decis");
    key(&mut h, Key::Enter, Modifiers::NONE);
    let app = h.state();
    assert!(app.insertion.picker.is_none(), "Enter closes it");
    let page = app.session.doc.page().unwrap();
    assert_eq!(page.shapes.len(), 2);
    let new = page.shapes.last().unwrap();
    assert_eq!(new.stencil, "flowchart.decision");
    let at = screen(&h, new.center());
    assert!((at - spot).length() < 30.0, "inserted at the point: {at:?} vs {spot:?}");
    // Escape closes without inserting; the command opens it with a query.
    h.state_mut().run("ui.insertPicker", json!({"query": "ellipse"})).unwrap();
    h.run_steps(2);
    assert!(h.state().insertion.picker.is_some());
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert!(h.state().insertion.picker.is_none());
    assert_eq!(shapes(&h), 2);
}

#[test]
fn the_palette_selects_and_reveals_an_element_on_another_page() {
    let (mut h, _) = flowchart();
    let page2 = h.state_mut().run("page.add", json!({"name": "Second"})).unwrap();
    let page2 = page2["page"].as_str().unwrap().to_string();
    let r = h
        .state_mut()
        .run("element.create", json!({"type": "flowchart.Process", "name": "Ship it", "tag": "SHIP-9", "x": 4000, "y": 4000, "page": page2}))
        .unwrap();
    let shape = r["shape"].as_str().unwrap().to_string();
    h.state_mut().run("page.setActive", json!({"page": 0})).unwrap();
    h.run_steps(2);
    key(&mut h, Key::K, Modifiers::COMMAND);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("commands"), "Mod+K opens the palette");
    type_text(&mut h, "ship-9");
    key(&mut h, Key::Enter, Modifiers::NONE);
    let app = h.state();
    assert!(app.dialog.is_none());
    assert_eq!(app.session.doc.page().unwrap().id, page2, "switched to the element's page");
    assert_eq!(app.session.sel.items, vec![shape.clone()]);
    let c = app.session.doc.page().unwrap().shape(&shape).unwrap().center();
    assert!((f64::from(app.session.view.pan[0]) - c.x).abs() < 1.0, "revealed");
}
