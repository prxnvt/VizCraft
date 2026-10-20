//! A multi-selection transformed as one object under egui_kittest: the group frame's corner
//! handle scales the shapes and the wire bends between them, the rotate handle turns positions
//! and angles about the frame's centre, a move carries the wire's bends, each gesture is one
//! undo step, and a single selection keeps its own handles with no second frame. Assertions read
//! the session and the canvas state, never pixels.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;
use vizcraft_diagram::{Point, resolve_ends};
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::canvas::interact::{self, Drag};
use vizcraft_ui_egui::{CraftApp, Services};

fn harness(session: Session) -> Harness<'static, CraftApp> {
    let app = CraftApp::new(session, Services::default());
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

/// The page point at the middle of the canvas.
fn centre(h: &Harness<'_, CraftApp>) -> Point {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).unwrap();
    let c = vp.screen_to_world(app.canvas.rect.unwrap().center());
    Point::new(f64::from(c.x).round(), f64::from(c.y).round())
}

fn zoom(h: &Harness<'_, CraftApp>) -> f64 {
    f64::from(h.state().session.view.zoom)
}

fn run(h: &mut Harness<'_, CraftApp>, id: &str, v: serde_json::Value) -> serde_json::Value {
    let r = h.state_mut().run(id, v).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(2);
    r
}

/// Press at `from`, move to `to` in steps (checking the drag in progress with `during` before the
/// release), release.
fn drag_with(h: &mut Harness<'_, CraftApp>, from: Pos2, to: Pos2, m: Modifiers, during: impl FnOnce(&CraftApp)) {
    h.event(Event::ModifiersChanged(m));
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: m });
    h.run_steps(1);
    for i in 1..=8 {
        h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
        h.run_steps(1);
    }
    during(h.state());
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: m });
    h.run_steps(1);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

fn drag(h: &mut Harness<'_, CraftApp>, from: Pos2, to: Pos2, m: Modifiers) {
    drag_with(h, from, to, m, |_| {});
}

fn shape(h: &Harness<'_, CraftApp>, id: &str) -> vizcraft_diagram::Shape {
    h.state().session.doc.page().unwrap().shape(id).unwrap().clone()
}

/// Two 40 by 20 boxes with pins on their left and right middles, `a` at `at` and `b` 200 right
/// and 100 down, and a wire from `a`'s right pin to `b`'s left pin bent twice at x + 120.
/// Returns the wire's id.
fn circuit(h: &mut Harness<'_, CraftApp>, at: Point) -> String {
    for (id, dx, dy) in [("a", 0.0, 0.0), ("b", 200.0, 100.0)] {
        run(h, "shape.add", json!({"stencil": "basic.rectangle", "id": id, "x": at.x + dx, "y": at.y + dy, "w": 40, "h": 20}));
        h.state_mut().session.doc.page_mut().unwrap().shape_mut(id).unwrap().connection_points = vec![Point::new(0.0, 0.5), Point::new(1.0, 0.5)];
    }
    let bends = [Point::new(at.x + 120.0, at.y + 10.0), Point::new(at.x + 120.0, at.y + 110.0)];
    let r = run(
        h,
        "wire.draw",
        json!({"from": {"shape": "a", "point": 1}, "to": {"shape": "b", "point": 0}, "points": [{"x": bends[0].x, "y": bends[0].y}, {"x": bends[1].x, "y": bends[1].y}]}),
    );
    run(h, "edit.selectNone", json!({}));
    r["id"].as_str().unwrap().to_string()
}

fn bends(h: &Harness<'_, CraftApp>, wire: &str) -> Vec<Point> {
    h.state().session.doc.page().unwrap().connector(wire).unwrap().waypoints.clone()
}

/// The wire still runs from `a`'s right pin to `b`'s left pin.
fn wire_between(h: &Harness<'_, CraftApp>, wire: &str) {
    let page = h.state().session.doc.page().unwrap();
    let c = page.connector(wire).unwrap();
    assert_eq!((c.start.shape(), c.end.shape()), (Some("a"), Some("b")));
    let ends = resolve_ends(page, c);
    let pa = page.shape("a").unwrap().connection_point_world(1).unwrap();
    let pb = page.shape("b").unwrap().connection_point_world(0).unwrap();
    assert!(ends.start.pos.distance(pa) < 1e-6 && ends.end.pos.distance(pb) < 1e-6, "{ends:?}");
}

/// Undo once and check the document is back where it was, then redo.
fn one_step(h: &mut Harness<'_, CraftApp>, before: &Document) {
    assert_ne!(&h.state().session.doc, before, "the gesture changed the document");
    run(h, "edit.undo", json!({}));
    assert_eq!(&h.state().session.doc, before, "one undo puts everything back");
    run(h, "edit.redo", json!({}));
}

#[test]
fn a_marquee_then_the_corner_handle_scales_two_shapes_and_the_wire_bends_on_an_engineering_page() {
    let mut session = Session::new(Document::new());
    session.run("template.new", &json!({"template": "electrical.controlCircuit"})).unwrap();
    let mut h = harness(session);
    let module = vizcraft_diagram::snap::module_step(h.state().session.doc.page().unwrap()).expect("an engineering page has a module");
    let c = centre(&h);
    let at = Point::new(((c.x - 150.0) / module).round() * module, ((c.y - 100.0) / module).round() * module);
    let wire = circuit(&mut h, at);
    // Marquee around the two boxes and the wire.
    let (m0, m1) = (screen(&h, Point::new(at.x - 20.0, at.y - 20.0)), screen(&h, Point::new(at.x + 260.0, at.y + 140.0)));
    drag(&mut h, m0, m1, Modifiers::NONE);
    let mut sel = h.state().session.sel.items.clone();
    sel.sort();
    let mut want = vec!["a".to_string(), "b".to_string(), wire.clone()];
    want.sort();
    assert_eq!(sel, want, "the marquee picks both boxes and the wire");
    let page = h.state().session.doc.page().unwrap();
    let g = interact::group::selection(h.state(), page).expect("a multi-selection has a group frame");
    let before = h.state().session.doc.clone();
    let (bends0, a0, b0) = (bends(&h, &wire), shape(&h, "a"), shape(&h, "b"));
    // The bottom-right handle, dragged out so the frame doubles.
    let corner = Point::new(g.bounds.x1, g.bounds.y1);
    let to = Point::new(g.bounds.x0 + 2.0 * g.bounds.width(), g.bounds.y0 + 2.0 * g.bounds.height());
    let (from, to) = (screen(&h, corner), screen(&h, to));
    drag_with(&mut h, from, to, Modifiers::NONE, |app| {
        assert!(matches!(app.canvas.drag, Some(Drag::ResizeGroup { handle: 4, .. })), "{:?}", app.canvas.drag);
        assert_eq!(vizcraft_ui_egui::canvas::summary(app)["dragging"], "resize");
    });
    let (a1, b1) = (shape(&h, "a"), shape(&h, "b"));
    assert!((a1.w / a0.w - 2.0).abs() < 0.05 && (b1.h / b0.h - 2.0).abs() < 0.05, "both boxes doubled: {} {}", a1.w, b1.h);
    let origin = Point::new(g.bounds.x0, g.bounds.y0);
    for (old, new) in bends0.iter().zip(bends(&h, &wire)) {
        let want = Point::new(origin.x + 2.0 * (old.x - origin.x), origin.y + 2.0 * (old.y - origin.y));
        assert!(new.distance(want) <= module + 2.0, "bend {old:?} went to {new:?}, wanted near {want:?}");
    }
    wire_between(&h, &wire);
    assert_eq!(h.state().session.undo_label(), Some("Resize"), "Undo names the gesture");
    one_step(&mut h, &before);
}

#[test]
fn the_rotate_handle_turns_a_flowchart_selection_about_its_centre() {
    let mut h = harness(Session::new(vizcraft_engine::sample::sample_document()));
    run(&mut h, "view.snap", json!({"value": false}));
    run(&mut h, "edit.select", json!({"ids": ["s3", "s4"]}));
    let page = h.state().session.doc.page().unwrap();
    let g = interact::group::selection(h.state(), page).unwrap();
    let (s3, s4) = (shape(&h, "s3"), shape(&h, "s4"));
    let before = h.state().session.doc.clone();
    let pivot = g.bounds.center();
    let handle = screen(&h, Point::new(pivot.x, g.bounds.y0)) - vec2(0.0, 22.0);
    // To the right of the centre: a quarter turn, snapped with Shift.
    let to = screen(&h, pivot) + vec2(200.0, 3.0);
    drag_with(&mut h, handle, to, Modifiers::SHIFT, |app| {
        assert!(matches!(app.canvas.drag, Some(Drag::RotateGroup { .. })), "{:?}", app.canvas.drag);
        assert!(vizcraft_ui_egui::canvas::status_items(app).iter().any(|s| s == "Angle 90\u{b0}"), "the status bar says the angle");
    });
    assert_eq!(h.state().session.undo_label(), Some("Rotate"));
    let turn = |p: Point| Point::new(pivot.x - (p.y - pivot.y), pivot.y + (p.x - pivot.x));
    for (old, id) in [(s3, "s3"), (s4, "s4")] {
        let new = shape(&h, id);
        assert!(new.center().distance(turn(old.center())) < 1e-6, "{id}: {:?} -> {:?}", old.center(), new.center());
        assert_eq!(new.angle_deg, (old.angle_deg + 90.0).rem_euclid(360.0));
    }
    one_step(&mut h, &before);
}

#[test]
fn dragging_the_group_across_the_page_carries_the_wire_bends() {
    let mut h = harness(Session::new(Document::new()));
    run(&mut h, "view.snap", json!({"value": false}));
    let c = centre(&h);
    let wire = circuit(&mut h, Point::new(c.x - 300.0, c.y - 100.0));
    run(&mut h, "edit.select", json!({"ids": ["a", "b", wire]}));
    let before = h.state().session.doc.clone();
    let (bends0, a0) = (bends(&h, &wire), shape(&h, "a"));
    let from = screen(&h, a0.center());
    drag(&mut h, from, from + vec2(400.0, 200.0), Modifiers::NONE);
    let (dx, dy) = (shape(&h, "a").x - a0.x, shape(&h, "a").y - a0.y);
    assert!((dx - 400.0 / zoom(&h)).abs() < 1.0 && (dy - 200.0 / zoom(&h)).abs() < 1.0, "moved {dx}, {dy}");
    let moved: Vec<Point> = bends0.iter().map(|p| Point::new(p.x + dx, p.y + dy)).collect();
    assert_eq!(bends(&h, &wire), moved, "the bends came along");
    assert_eq!(h.state().session.undo_label(), Some("Move"));
    wire_between(&h, &wire);
    one_step(&mut h, &before);
}

#[test]
fn a_single_selection_keeps_its_own_handles_and_no_group_frame() {
    let mut h = harness(Session::new(vizcraft_engine::sample::sample_document()));
    run(&mut h, "edit.select", json!({"ids": ["s3"]}));
    let page = h.state().session.doc.page().unwrap();
    assert!(interact::group::selection(h.state(), page).is_none(), "one shape has no group frame");
    let s = shape(&h, "s3");
    let handle = screen(&h, Point::new(s.x + s.w, s.y + s.h));
    drag_with(&mut h, handle, handle + vec2(30.0, 20.0), Modifiers::NONE, |app| {
        assert!(matches!(app.canvas.drag, Some(Drag::Resize { .. })), "{:?}", app.canvas.drag);
    });
    assert_eq!(h.state().session.undo_label(), Some("Resize"));
    // A selected group is one item: its own frame, not a group frame around it.
    run(&mut h, "edit.select", json!({"ids": ["s3", "s4"]}));
    run(&mut h, "arrange.group", json!({}));
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(h.state().session.sel.items.len(), 1);
    assert!(interact::group::selection(h.state(), page).is_none());
}
