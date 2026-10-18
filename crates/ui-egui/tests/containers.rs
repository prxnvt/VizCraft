//! Containers on the canvas, driven with real pointer events under egui_kittest: dragging a shape
//! into a container makes it a member in the same undo step as the move, dragging it fully out
//! takes it out, a click on a member selects the member, dragging the container carries its
//! members, and the contextual Container and Callout tabs follow the selection.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_diagram::Point;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

/// Rectangles `a` (100,100 80x60) and `b` (600,140 80x60) and a container `k` (60,60 400x300)
/// holding `a`.
fn harness() -> Harness<'static, CraftApp> {
    let mut session = Session::new(vizcraft_diagram::Document::new());
    for (id, x, y) in [("a", 100, 100), ("b", 600, 140)] {
        session.run("shape.add", &json!({"stencil": "basic.rectangle", "id": id, "x": x, "y": y, "w": 80, "h": 60})).unwrap();
    }
    session.run("container.add", &json!({"x": 60, "y": 60, "w": 400, "h": 300, "id": "k", "heading": "Team"})).unwrap();
    session.run("container.addMember", &json!({"container": "k", "ids": ["a"]})).unwrap();
    session.run("edit.selectNone", &json!({})).unwrap();
    // Snapping would nudge the drops; these tests are about membership.
    session.view.aids.snap_grid = false;
    session.view.aids.snap_shapes = false;
    session.view.aids.snap_points = false;
    let mut app = CraftApp::new(session, Services::default());
    app.ui.panes.close("history");
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

fn centre(h: &Harness<'_, CraftApp>, id: &str) -> Pos2 {
    screen(h, h.state().session.doc.page().unwrap().shape(id).unwrap().center())
}

fn parent(h: &Harness<'_, CraftApp>, id: &str) -> Option<String> {
    h.state().session.doc.page().unwrap().shape(id).unwrap().parent.clone()
}

fn zoom(h: &Harness<'_, CraftApp>) -> f32 {
    let app = h.state();
    app.canvas.viewport(&app.session.view).map_or(1.0, |vp| vp.zoom)
}

fn press_release(h: &mut Harness<'_, CraftApp>, from: Pos2, to: Pos2) {
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    if from != to {
        for i in 1..=8 {
            h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
            h.run_steps(1);
        }
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.run_steps(30);
}

#[test]
fn drag_in_joins_drag_out_leaves_and_the_container_carries_its_members() {
    let mut h = harness();
    let z = zoom(&h);
    // `b` dragged to the middle of the container joins it, in the move's own undo step.
    let from = centre(&h, "b");
    press_release(&mut h, from, from - vec2(320.0 * z, 0.0));
    assert_eq!(parent(&h, "b").as_deref(), Some("k"));
    assert_eq!(h.state().session.undo_label(), Some("Move"));
    let ids: Vec<String> = h.state().session.doc.page().unwrap().shapes.iter().map(|s| s.id.clone()).collect();
    assert_eq!(ids.last().map(String::as_str), Some("b"), "a new member draws above the container");
    // A click on a member selects the member, not the container.
    let b = centre(&h, "b");
    press_release(&mut h, b, b);
    assert_eq!(h.state().session.sel.items, ["b"]);
    // Dragging the container by its heading carries both members.
    let head = screen(&h, Point::new(260.0, 70.0));
    let (ax, bx) = (h.state().session.doc.shape("a").unwrap().x, h.state().session.doc.shape("b").unwrap().x);
    press_release(&mut h, head, head + vec2(0.0, 40.0 * z));
    assert_eq!(h.state().session.sel.items, ["k"]);
    assert_eq!(h.state().session.doc.shape("a").unwrap().x, ax);
    assert!(h.state().session.doc.shape("a").unwrap().y > 100.0 + 20.0);
    assert_eq!(h.state().session.doc.shape("b").unwrap().x, bx);
    // `b` dragged fully out leaves.
    h.state_mut().session.run("edit.selectNone", &json!({})).unwrap();
    h.run_steps(2);
    let b = centre(&h, "b");
    press_release(&mut h, b, b + vec2(400.0 * z, 0.0));
    assert_eq!(parent(&h, "b"), None);
    assert_eq!(parent(&h, "a").as_deref(), Some("k"));
}

#[test]
fn contextual_tabs_follow_the_selection_and_insert_offers_the_parts() {
    let mut h = harness();
    h.state_mut().run("ui.tab", json!({"tab": "Insert"})).unwrap();
    h.run_steps(3);
    for label in ["Container", "Callout", "Swimlanes"] {
        assert!(h.query_all_by_label_contains(label).next().is_some(), "Insert > Diagram Parts has {label}");
    }
    let tabs = |h: &Harness<'_, CraftApp>| {
        vizcraft_ui_egui::ribbon::tabs(h.state()).into_iter().filter(|t| t.visible(h.state())).map(|t| t.name).collect::<Vec<_>>()
    };
    assert!(!tabs(&h).contains(&"Container"));
    h.state_mut().run("edit.select", json!({"ids": ["k"]})).unwrap();
    h.run_steps(3);
    assert!(tabs(&h).contains(&"Container"));
    assert!(!tabs(&h).contains(&"Callout"));
    h.state_mut().run("callout.add", json!({"target": "a", "id": "co"})).unwrap();
    h.run_steps(3);
    assert!(tabs(&h).contains(&"Callout"));
    assert!(!tabs(&h).contains(&"Container"));
    // Every command the new tabs and group run is registered.
    for t in vizcraft_ui_egui::ribbon::tabs(h.state()) {
        for id in t.ids() {
            assert!(
                h.state().session.registry.get(id).is_some() || id.starts_with("ui.") || id.starts_with("app."),
                "{}: {id} is not a command",
                t.name
            );
        }
    }
}
