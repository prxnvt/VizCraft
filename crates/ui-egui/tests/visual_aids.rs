//! Grid, rulers, guides and snapping on the canvas, driven with real pointer events under
//! egui_kittest on the sample flowchart: a drag out of a ruler makes a guide, dragging a guide
//! moves it and dragging it off deletes it, and shape drags snap to guides and the grid.
//! Assertions read the session, never pixels.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;
use vizcraft_diagram::{GuideOrientation, Point, snap};
use vizcraft_engine::Session;
use vizcraft_ui_egui::canvas::rulers::RULER_W;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness() -> Harness<'static, CraftApp> {
    let app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
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

fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

fn canvas(h: &Harness<'_, CraftApp>) -> egui::Rect {
    h.state().canvas.rect.expect("the canvas has been laid out")
}

fn zoom(h: &Harness<'_, CraftApp>) -> f64 {
    f64::from(h.state().session.view.zoom)
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
    h.run_steps(3);
}

fn guides(h: &Harness<'_, CraftApp>) -> Vec<vizcraft_diagram::Guide> {
    h.state().session.doc.page().unwrap().guides.clone()
}

#[test]
fn rulers_take_room_from_the_canvas_only_while_shown() {
    let mut h = harness();
    let with = canvas(&h);
    h.state_mut().run("view.rulers", json!({"value": false})).unwrap();
    h.run_steps(2);
    let without = canvas(&h);
    assert_eq!(with.min, without.min + vec2(RULER_W, RULER_W));
    assert_eq!(with.max, without.max);
}

#[test]
fn guides_come_out_of_the_rulers_move_and_go_back() {
    let mut h = harness();
    let c = canvas(&h);
    // Out of the top ruler, down to page y = 300: a horizontal guide there (on the grid).
    let target = screen(&h, Point::new(400.0, 300.0));
    drag(&mut h, pos2(target.x, c.min.y - RULER_W / 2.0), target);
    let g = guides(&h);
    assert_eq!(g.len(), 1, "one guide made");
    assert_eq!(g[0].orientation, GuideOrientation::Horizontal);
    assert!((g[0].position - 300.0).abs() < 13.0, "at {}", g[0].position);
    assert_eq!(h.state().session.undo_label(), Some("Add Guide"));

    // Out of the left ruler: a vertical guide.
    let target = screen(&h, Point::new(250.0, 400.0));
    drag(&mut h, pos2(c.min.x - RULER_W / 2.0, target.y), target);
    let g = guides(&h);
    assert_eq!(g.len(), 2);
    assert_eq!(g[1].orientation, GuideOrientation::Vertical);

    // Dragging the first guide moves it.
    let at = screen(&h, Point::new(900.0, g[0].position));
    let to = screen(&h, Point::new(900.0, 500.0));
    drag(&mut h, at, to);
    let g = guides(&h);
    assert!((g[0].position - 500.0).abs() < 13.0, "moved to {}", g[0].position);
    assert_eq!(h.state().session.undo_label(), Some("Move Guide"));

    // Dragging it off the canvas, back onto the ruler, deletes it.
    let at = screen(&h, Point::new(900.0, g[0].position));
    drag(&mut h, at, pos2(at.x, c.min.y - RULER_W / 2.0));
    let g = guides(&h);
    assert_eq!(g.len(), 1, "deleted");
    assert_eq!(g[0].orientation, GuideOrientation::Vertical);
    assert_eq!(h.state().session.undo_label(), Some("Delete Guide"));
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(guides(&h).len(), 2, "undo brings it back");
}

#[test]
fn shapes_snap_to_a_guide_within_reach() {
    let mut h = harness();
    // Guides only, so nothing else competes.
    h.state_mut().run("view.snap", json!({"grid": false, "shapes": false, "points": false})).unwrap();
    let before = h.state().session.doc.page().unwrap().shape("s7").unwrap().clone();
    let z = zoom(&h);
    // A vertical guide three screen points past where a 40 point drag leaves the left edge.
    let at = before.x + 40.0 / z + 3.0 / z;
    h.state_mut().run("guide.add", json!({"orientation": "vertical", "position": at})).unwrap();
    h.run_steps(2);
    let from = screen(&h, before.center());
    drag(&mut h, from, from + vec2(40.0, 0.0));
    let after = h.state().session.doc.page().unwrap().shape("s7").unwrap().clone();
    assert!((after.x - at).abs() <= 0.5, "left edge {} should sit on the guide at {at}", after.x);
}

#[test]
fn shapes_snap_to_the_grid_and_not_with_snap_off() {
    let mut h = harness();
    h.state_mut().run("view.snap", json!({"shapes": false, "points": false})).unwrap();
    let from = screen(&h, h.state().session.doc.page().unwrap().shape("s7").unwrap().center());
    drag(&mut h, from, from + vec2(37.0, 23.0));
    let page = h.state().session.doc.page().unwrap();
    let s = page.shape("s7").unwrap().bounds();
    let step = snap::grid_step(page.units, zoom(&h), vizcraft_ui_egui::canvas::draw::overlays::GRID_MIN_PX);
    let on_grid = |v: f64| ((v / step).round() * step - v).abs() < 0.51;
    assert!([s.x0, s.center().x, s.x1].into_iter().any(on_grid), "x {s:?} on a {step} grid");
    assert!([s.y0, s.center().y, s.y1].into_iter().any(on_grid), "y {s:?} on a {step} grid");

    // With snapping off the move is exactly the drag.
    h.state_mut().run("view.snap", json!({"value": false})).unwrap();
    let before = h.state().session.doc.page().unwrap().shape("s7").unwrap().clone();
    let from = screen(&h, before.center());
    drag(&mut h, from, from + vec2(37.0, 23.0));
    let after = h.state().session.doc.page().unwrap().shape("s7").unwrap().clone();
    let z = zoom(&h);
    assert!((after.x - before.x - 37.0 / z).abs() < 1.0 && (after.y - before.y - 23.0 / z).abs() < 1.0);
}
