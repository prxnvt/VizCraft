//! The Developer tab and shape locks on the canvas, driven under egui_kittest on the sample
//! flowchart: the tab fits 1280 px, its Operations menu runs a shape operation, and a locked
//! shape refuses the canvas drags its locks forbid. Assertions read the session, never pixels.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_diagram::Point;
use vizcraft_engine::Session;
use vizcraft_ui_egui::ribbon::Overflow;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness(width: f32, tab: &str) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    app.ui.tab = tab.into();
    let mut h = Harness::builder().with_size(vec2(width, 900.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
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

fn shape(h: &Harness<'_, CraftApp>, id: &str) -> vizcraft_diagram::Shape {
    h.state().session.doc.page().unwrap().shape(id).unwrap().clone()
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

#[test]
fn the_developer_tab_fits_1280_wide_with_its_groups() {
    let h = harness(1280.0, "Developer");
    assert!(!Overflow::get(&h.ctx).overflows(), "{:?}", Overflow::get(&h.ctx));
    for label in ["Operations", "Shape Name", "Behavior", "Protection", "Document Stencil", "Drawing Explorer"] {
        assert!(h.query_by_label(label).is_some(), "{label} is missing from Developer");
    }
}

#[test]
fn the_operations_menu_unions_the_selection() {
    let mut h = harness(1280.0, "Developer");
    let ids: Vec<String> = h.state().session.doc.page().unwrap().shapes.iter().take(2).map(|s| s.id.clone()).collect();
    h.state_mut().run("edit.select", json!({"ids": ids})).unwrap();
    h.run_steps(2);
    h.get_by_label("Operations").click();
    h.run_steps(2);
    h.get_by_label("Union").click();
    h.run_steps(3);
    let page = h.state().session.doc.page().unwrap();
    assert!(ids.iter().all(|id| page.shape(id).is_none()), "the inputs are gone");
    let sel = &h.state().session.sel.items;
    assert_eq!(sel.len(), 1);
    assert_eq!(page.shape(&sel[0]).unwrap().kind, vizcraft_diagram::ShapeKind::Freeform);
}

#[test]
fn a_position_locked_shape_has_no_move_drag() {
    let mut h = harness(1600.0, "Home");
    h.state_mut().run("view.snap", json!({"value": false})).unwrap();
    h.state_mut().run("shape.setLocks", json!({"ids": ["s7"], "position": true})).unwrap();
    h.run_steps(2);
    let before = shape(&h, "s7");
    let from = screen(&h, before.center());
    drag(&mut h, from, from + vec2(60.0, 40.0));
    let after = shape(&h, "s7");
    assert_eq!((after.x, after.y), (before.x, before.y), "the locked shape stayed put");
    assert_eq!(h.state().session.undo_label(), Some("Protection"), "no move was recorded");
    // Unlocked, the same drag moves it.
    h.state_mut().run("shape.setLocks", json!({"ids": ["s7"], "position": false})).unwrap();
    h.run_steps(2);
    drag(&mut h, from, from + vec2(60.0, 40.0));
    assert_ne!(shape(&h, "s7").x, before.x);
}

#[test]
fn a_width_locked_shape_keeps_its_width_and_hides_its_side_handles() {
    let mut h = harness(1600.0, "Home");
    h.state_mut().run("shape.setLocks", json!({"ids": ["s3"], "width": true})).unwrap();
    h.state_mut().run("edit.select", json!({"ids": ["s3"]})).unwrap();
    h.run_steps(2);
    let before = shape(&h, "s3");
    // The right edge handle (index 3) is hidden; a corner changes only the height.
    assert!(!vizcraft_ui_egui::canvas::interact::drag::handle_allowed(&before, 3));
    assert!(vizcraft_ui_egui::canvas::interact::drag::handle_allowed(&before, 4));
    let corner = screen(&h, Point::new(before.x + before.w, before.y + before.h));
    drag(&mut h, corner, corner + vec2(30.0, 20.0));
    let after = shape(&h, "s3");
    assert_eq!(after.w, before.w, "the width is locked");
    assert!((after.h - before.h - 20.0).abs() <= 1.0, "{} -> {}", before.h, after.h);
}

#[test]
fn protect_document_stops_canvas_drags() {
    let mut h = harness(1600.0, "Home");
    h.state_mut().run("view.snap", json!({"value": false})).unwrap();
    h.state_mut().run("doc.protect", json!({"shapes": true})).unwrap();
    h.run_steps(2);
    let before = shape(&h, "s7");
    let from = screen(&h, before.center());
    drag(&mut h, from, from + vec2(60.0, 40.0));
    assert_eq!(shape(&h, "s7").x, before.x);
}

#[test]
fn drawing_explorer_opens_the_navigation_pane_on_the_diagrams_branch() {
    let mut h = harness(1600.0, "Developer");
    h.state_mut().run("ui.tree", json!({"branch": "model"})).unwrap();
    h.run_steps(2);
    h.get_by_label("Drawing Explorer").click();
    h.run_steps(4);
    assert!(h.state().ui.panes.is_open("navigation"));
    let ins = vizcraft_ui_egui::control::inspect(h.state(), &h.ctx);
    assert_eq!(ins["tree"]["branch"], "diagrams");
}
