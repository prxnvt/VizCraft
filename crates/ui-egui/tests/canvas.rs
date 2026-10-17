//! The diagram canvas driven with real pointer and key events under egui_kittest, on the sample
//! flowchart: selection, marquee, move, resize, connectors, in-place text, Delete, and the
//! Shapes pane. Assertions read the session, never pixels.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_diagram::{End, Point};
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness() -> Harness<'static, CraftApp> {
    let app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    let mut h = Harness::builder()
        .with_size(vec2(1600.0, 1000.0))
        .with_max_steps(64)
        // Frames 1/60 s apart, as on screen, so two clicks a few frames apart are a double click.
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, app: &mut CraftApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
    h.run_steps(4);
    h
}

/// Screen position of a world point in the last frame.
fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

fn shape_centre(h: &Harness<'_, CraftApp>, id: &str) -> Pos2 {
    let c = h.state().session.doc.page().unwrap().shape(id).unwrap().center();
    screen(h, c)
}

fn shape(h: &Harness<'_, CraftApp>, id: &str) -> vizcraft_diagram::Shape {
    h.state().session.doc.page().unwrap().shape(id).unwrap().clone()
}

fn selection(h: &Harness<'_, CraftApp>) -> Vec<String> {
    h.state().session.sel.items.clone()
}

/// Let more time pass than a double click allows, so the next click is a fresh one.
fn settle(h: &mut Harness<'_, CraftApp>) {
    h.run_steps(30);
}

fn click(h: &mut Harness<'_, CraftApp>, p: Pos2, m: Modifiers) {
    h.event(Event::ModifiersChanged(m));
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: m });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: m });
    h.run_steps(1);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
}

fn double_click(h: &mut Harness<'_, CraftApp>, p: Pos2) {
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

fn drag(h: &mut Harness<'_, CraftApp>, from: Pos2, to: Pos2, m: Modifiers) {
    h.event(Event::ModifiersChanged(m));
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: m });
    h.run_steps(1);
    for i in 1..=8 {
        h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: m });
    h.run_steps(1);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

fn key(h: &mut Harness<'_, CraftApp>, k: Key, m: Modifiers) {
    h.event(Event::ModifiersChanged(m));
    h.event(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m });
    h.event(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m });
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

/// An empty spot on the page, away from every shape and connector.
const EMPTY: Point = Point { x: 150.0, y: 700.0 };

#[test]
fn the_view_starts_on_the_page() {
    let h = harness();
    let app = h.state();
    let canvas = app.canvas.rect.unwrap();
    // Every sample shape is in view at 100%.
    for s in &app.session.doc.page().unwrap().shapes {
        let c = screen(&h, s.center());
        assert!(canvas.contains(c), "{} at {c:?} is outside the canvas {canvas:?}", s.id);
    }
}

#[test]
fn click_selects_and_shift_click_toggles() {
    let mut h = harness();
    let p = shape_centre(&h, "s3");
    click(&mut h, p, Modifiers::NONE);
    assert_eq!(selection(&h), ["s3"]);
    settle(&mut h);
    let p = shape_centre(&h, "s4");
    click(&mut h, p, Modifiers::SHIFT);
    assert_eq!(selection(&h), ["s3", "s4"]);
    settle(&mut h);
    let p = shape_centre(&h, "s3");
    click(&mut h, p, Modifiers::SHIFT);
    assert_eq!(selection(&h), ["s4"], "Shift-click on a selected shape removes it");
    settle(&mut h);
    let p = screen(&h, EMPTY);
    click(&mut h, p, Modifiers::NONE);
    assert!(selection(&h).is_empty(), "a click on empty page clears the selection");
    // Selection is not an undo step.
    assert!(!h.state().session.can_undo());
}

#[test]
fn marquee_selects_what_it_touches() {
    let mut h = harness();
    let (a, b) = (screen(&h, Point::new(250.0, 10.0)), screen(&h, Point::new(560.0, 200.0)));
    drag(&mut h, a, b, Modifiers::NONE);
    let sel = selection(&h);
    assert!(sel.contains(&"s2".to_string()) && sel.contains(&"s3".to_string()), "{sel:?}");
    assert!(!sel.contains(&"s6".to_string()), "End is far below the marquee: {sel:?}");
    assert!(sel.iter().any(|id| id.starts_with('c')), "the connector between them is touched: {sel:?}");
    assert!(h.state().canvas.drag.is_none());
}

#[test]
fn dragging_a_shape_moves_it_in_one_undo_step() {
    let mut h = harness();
    let before = shape(&h, "s7");
    let route_before = {
        let page = h.state().session.doc.page().unwrap();
        vizcraft_diagram::route_polyline(page, page.connector("c11").unwrap())
    };
    let from = shape_centre(&h, "s7");
    drag(&mut h, from, from + vec2(40.0, 30.0), Modifiers::NONE);
    let after = shape(&h, "s7");
    let zoom = f64::from(h.state().session.view.zoom);
    assert!((after.x - before.x - 40.0 / zoom).abs() < 1.0, "{} -> {}", before.x, after.x);
    assert!((after.y - before.y - 30.0 / zoom).abs() < 1.0);
    assert_eq!(selection(&h), ["s7"], "dragging an unselected shape selects it");
    assert_eq!(h.state().session.undo_label(), Some("Move"));
    // The glued connector followed.
    let page = h.state().session.doc.page().unwrap();
    assert_ne!(vizcraft_diagram::route_polyline(page, page.connector("c11").unwrap()), route_before);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(shape(&h, "s7"), before, "one undo puts it back");
    assert!(!h.state().session.can_undo(), "the whole drag was one step");
}

#[test]
fn resize_handle_drag_changes_the_size() {
    let mut h = harness();
    h.state_mut().run("edit.select", json!({"ids": ["s3"]})).unwrap();
    h.run_steps(2);
    let before = shape(&h, "s3");
    // Bottom-right handle.
    let handle = screen(&h, Point::new(before.x + before.w, before.y + before.h));
    drag(&mut h, handle, handle + vec2(30.0, 20.0), Modifiers::NONE);
    let after = shape(&h, "s3");
    assert!((after.w - before.w - 30.0).abs() <= 1.0, "{} -> {}", before.w, after.w);
    assert!((after.h - before.h - 20.0).abs() <= 1.0, "{} -> {}", before.h, after.h);
    assert_eq!((after.x, after.y), (before.x, before.y), "the opposite corner stays put");
    assert_eq!(h.state().session.undo_label(), Some("Resize"));
}

#[test]
fn rotate_handle_drag_rotates() {
    let mut h = harness();
    h.state_mut().run("edit.select", json!({"ids": ["s7"]})).unwrap();
    h.run_steps(2);
    let s = shape(&h, "s7");
    let handle = screen(&h, Point::new(s.x + s.w / 2.0, s.y)) - vec2(0.0, 22.0);
    let centre = shape_centre(&h, "s7");
    // Drag the handle to the right of the centre: a quarter turn, snapped with Shift.
    drag(&mut h, handle, centre + vec2(120.0, 3.0), Modifiers::SHIFT);
    assert_eq!(shape(&h, "s7").angle_deg, 90.0);
}

#[test]
fn dragging_from_a_connection_point_glues_a_new_connector() {
    let mut h = harness();
    let n = h.state().session.doc.page().unwrap().connectors.len();
    let fix = shape(&h, "s7");
    let end = shape(&h, "s6");
    // From Fix input's bottom point (2) to End's right point (1).
    let from = screen(&h, fix.connection_point_world(2).unwrap());
    let to = screen(&h, end.connection_point_world(1).unwrap());
    h.event(Event::PointerMoved(from));
    h.run_steps(2);
    assert_eq!(h.state().canvas.hover_shape.as_deref(), Some("s7"), "hovering shows the shape's connection points");
    drag(&mut h, from, to, Modifiers::NONE);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.connectors.len(), n + 1);
    let c = page.connectors.last().unwrap();
    assert_eq!(c.start, End::point("s7", 2));
    assert_eq!(c.end, End::point("s6", 1));
    assert_eq!(h.state().session.undo_label(), Some("Add Connector"));

    // Released on a shape body: dynamic glue.
    settle(&mut h);
    let from = screen(&h, fix.connection_point_world(0).unwrap());
    let to = shape_centre(&h, "s2");
    drag(&mut h, from, to, Modifiers::NONE);
    let c = h.state().session.doc.page().unwrap().connectors.last().unwrap().clone();
    assert_eq!(c.end, End::dynamic("s2"));

    // Released on empty page: a free end.
    settle(&mut h);
    let from = screen(&h, fix.connection_point_world(1).unwrap());
    let to = screen(&h, Point::new(1000.0, 650.0));
    drag(&mut h, from, to, Modifiers::NONE);
    let c = h.state().session.doc.page().unwrap().connectors.last().unwrap().clone();
    assert!(matches!(c.end, End::Free { .. }), "{:?}", c.end);
}

#[test]
fn connector_tool_draws_from_a_shape_body() {
    let mut h = harness();
    h.state_mut().run("ui.tool", json!({"tool": "connector"})).unwrap();
    let n = h.state().session.doc.page().unwrap().connectors.len();
    let (a, b) = (shape_centre(&h, "s7"), shape_centre(&h, "s5"));
    drag(&mut h, a, b, Modifiers::NONE);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.connectors.len(), n + 1);
    let c = page.connectors.last().unwrap();
    assert_eq!((c.start.shape(), c.end.shape()), (Some("s7"), Some("s5")));
}

#[test]
fn dragging_a_connector_end_reglues_it() {
    let mut h = harness();
    // c11 runs from Valid? (s4) right to Fix input (s7) left; move its end onto Save diagram.
    h.state_mut().run("edit.select", json!({"ids": ["c11"]})).unwrap();
    h.run_steps(2);
    let page = h.state().session.doc.page().unwrap();
    let ends = vizcraft_diagram::resolve_ends(page, page.connector("c11").unwrap());
    let from = screen(&h, ends.end.pos);
    let target = shape(&h, "s5");
    let to = screen(&h, target.connection_point_world(1).unwrap());
    drag(&mut h, from, to, Modifiers::NONE);
    let c = h.state().session.doc.page().unwrap().connector("c11").unwrap().clone();
    assert_eq!(c.end, End::point("s5", 1));
    assert_eq!(h.state().session.undo_label(), Some("Glue Connector"));
}

#[test]
fn double_click_edits_text_in_place() {
    let mut h = harness();
    let p = shape_centre(&h, "s4");
    double_click(&mut h, p);
    assert_eq!(h.state().canvas.editing.as_ref().map(|e| e.id().to_string()), Some("s4".into()));
    // The whole text starts selected, so typing replaces it; Enter commits.
    h.event(Event::Text("Is it valid?".into()));
    h.run_steps(2);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.state().canvas.editing.is_none());
    assert_eq!(shape(&h, "s4").text, "Is it valid?");
    assert_eq!(h.state().session.undo_label(), Some("Edit Text"));

    // Escape cancels.
    settle(&mut h);
    double_click(&mut h, p);
    h.event(Event::Text("thrown away".into()));
    h.run_steps(2);
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert!(h.state().canvas.editing.is_none());
    assert_eq!(shape(&h, "s4").text, "Is it valid?");

    // Clicking elsewhere commits.
    settle(&mut h);
    let p = shape_centre(&h, "s3");
    double_click(&mut h, p);
    h.event(Event::Text("Collect".into()));
    h.run_steps(2);
    settle(&mut h);
    let away = screen(&h, EMPTY);
    click(&mut h, away, Modifiers::NONE);
    h.run_steps(2);
    assert!(h.state().canvas.editing.is_none());
    assert_eq!(shape(&h, "s3").text, "Collect");
}

#[test]
fn text_tool_click_edits_a_connector_label() {
    let mut h = harness();
    h.state_mut().run("ui.tool", json!({"tool": "text"})).unwrap();
    let page = h.state().session.doc.page().unwrap();
    let g = vizcraft_diagram::connector_geometry(page, page.connector("c10").unwrap());
    let p = screen(&h, g.label_pos);
    click(&mut h, p, Modifiers::NONE);
    assert_eq!(h.state().canvas.editing.as_ref().map(|e| e.id().to_string()), Some("c10".into()));
    h.event(Event::Text("OK".into()));
    h.run_steps(2);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(h.state().session.doc.page().unwrap().connector("c10").unwrap().text, "OK");
}

#[test]
fn delete_key_removes_the_selection_and_escape_clears_it() {
    let mut h = harness();
    let p = shape_centre(&h, "s7");
    click(&mut h, p, Modifiers::NONE);
    assert_eq!(selection(&h), ["s7"]);
    key(&mut h, Key::Delete, Modifiers::NONE);
    assert!(h.state().session.doc.page().unwrap().shape("s7").is_none());
    assert!(selection(&h).is_empty());
    // Backspace too, and Escape clears without deleting.
    settle(&mut h);
    let p = shape_centre(&h, "s6");
    click(&mut h, p, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert!(selection(&h).is_empty());
    assert!(h.state().session.doc.page().unwrap().shape("s6").is_some());
    settle(&mut h);
    click(&mut h, p, Modifiers::NONE);
    key(&mut h, Key::Backspace, Modifiers::NONE);
    assert!(h.state().session.doc.page().unwrap().shape("s6").is_none());
}

#[test]
fn arrow_keys_nudge_the_selection() {
    let mut h = harness();
    let p = shape_centre(&h, "s7");
    click(&mut h, p, Modifiers::NONE);
    let before = shape(&h, "s7");
    key(&mut h, Key::ArrowRight, Modifiers::NONE);
    key(&mut h, Key::ArrowDown, Modifiers::SHIFT);
    let after = shape(&h, "s7");
    assert_eq!((after.x - before.x, after.y - before.y), (1.0, 10.0));
}

#[test]
fn shapes_pane_click_inserts_in_view() {
    let mut h = harness();
    let n = h.state().session.doc.page().unwrap().shapes.len();
    h.get_by_label("Rectangle").click();
    h.run_steps(3);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.shapes.len(), n + 1);
    let s = page.shapes.last().unwrap().clone();
    assert_eq!(s.stencil, "basic.rectangle");
    assert_eq!(selection(&h), std::slice::from_ref(&s.id), "the new shape is selected");
    let canvas = h.state().canvas.rect.unwrap();
    assert!(canvas.contains(screen(&h, s.center())), "inserted where it can be seen");
    assert!(page.rect().contains_rect(&s.bounds()), "inserted on the page");
}

#[test]
fn shapes_pane_drag_drops_at_the_pointer() {
    let mut h = harness();
    let tile = h.get_by_label("Decision").rect().center();
    let drop = screen(&h, Point::new(250.0, 600.0));
    h.event(Event::PointerMoved(tile));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: tile, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=10 {
        h.event(Event::PointerMoved(tile + (drop - tile) * (i as f32 / 10.0)));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: drop, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    let s = h.state().session.doc.page().unwrap().shapes.last().unwrap().clone();
    assert_eq!(s.stencil, "flowchart.decision");
    assert!(s.center().distance(Point::new(250.0, 600.0)) < 1.5, "{:?}", s.center());
}

#[test]
fn hostile_view_state_never_panics() {
    let mut h = harness();
    for (zoom, pan) in [(f32::NAN, [0.0, 0.0]), (1e-9, [1e30, -1e30]), (1e9, [f32::INFINITY, 0.0]), (1.0, [f32::NAN, f32::NAN])] {
        h.state_mut().session.view.zoom = zoom;
        h.state_mut().session.view.pan = pan;
        h.run_steps(2);
        let p = pos2(700.0, 400.0);
        drag(&mut h, p, p + vec2(10.0, 10.0), Modifiers::NONE);
        click(&mut h, p, Modifiers::NONE);
    }
}
