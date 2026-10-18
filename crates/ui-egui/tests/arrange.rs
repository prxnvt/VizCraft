//! Home > Arrange and groups on the canvas, driven with real pointer events under egui_kittest:
//! the menus run their `arrange.*` commands, a click on a group member selects the group and a
//! second click the member, and dragging a group moves its members. Assertions read the session.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use serde_json::json;
use vizcraft_diagram::Point;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

/// A blank document with two rectangles `a` (100,100 80x60) and `b` (300,100 80x60).
fn harness() -> Harness<'static, CraftApp> {
    let mut session = Session::new(vizcraft_diagram::Document::new());
    for (id, x) in [("a", 100), ("b", 300)] {
        session.run("shape.add", &json!({"stencil": "basic.rectangle", "id": id, "x": x, "y": 100, "w": 80, "h": 60})).unwrap();
    }
    session.run("edit.selectNone", &json!({})).unwrap();
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

fn x_of(h: &Harness<'_, CraftApp>, id: &str) -> f64 {
    h.state().session.doc.page().unwrap().shape(id).unwrap().x
}

fn selection(h: &Harness<'_, CraftApp>) -> Vec<String> {
    h.state().session.sel.items.clone()
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
    // Longer than a double click, so the next click is a fresh one.
    h.run_steps(30);
}

fn menu(h: &mut Harness<'_, CraftApp>, label: &str, entry: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
    // Menu items carry an empty shortcut column after the label; the popup comes after the ribbon
    // button of the same name.
    let item = h.query_all_by_label_contains(entry).rfind(|n| n.accesskit_node().label().is_some_and(|l| l.trim() == entry));
    item.unwrap_or_else(|| panic!("no menu item {entry}")).click();
    h.run_steps(3);
}

#[test]
fn arrange_menus_run_their_commands() {
    let mut h = harness();
    h.state_mut().session.run("edit.select", &json!({"ids": ["b", "a"]})).unwrap();
    h.run_steps(2);
    menu(&mut h, "Align", "Align Left");
    assert_eq!(x_of(&h, "a"), 300.0, "aligned to the first selected shape");
    menu(&mut h, "Group", "Group");
    let g = selection(&h);
    assert_eq!(g.len(), 1);
    assert!(vizcraft_diagram::arrange::is_group(h.state().session.doc.page().unwrap().shape(&g[0]).unwrap()));
    menu(&mut h, "Rotate", "Rotate Right 90");
    assert_eq!(h.state().session.doc.page().unwrap().shape("a").unwrap().angle_deg, 90.0);
    menu(&mut h, "Send to Back", "Send to Back");
    menu(&mut h, "Bring to Front", "Bring Forward");
    menu(&mut h, "Group", "Ungroup");
    assert_eq!(h.state().session.doc.page().unwrap().shapes.len(), 2);
    menu(&mut h, "Position", "Auto Space");
    menu(&mut h, "Rotate", "Flip Horizontal");
    assert_eq!(h.state().session.undo_label(), Some("Flip"));
}

#[test]
fn clicking_a_member_selects_the_group_then_the_member_and_dragging_moves_the_group() {
    let mut h = harness();
    h.state_mut().session.run("arrange.group", &json!({"ids": ["a", "b"], "id": "g"})).unwrap();
    h.state_mut().session.run("edit.selectNone", &json!({})).unwrap();
    h.run_steps(2);
    let a = centre(&h, "a");
    press_release(&mut h, a, a);
    assert_eq!(selection(&h), ["g"], "the first click selects the group");
    press_release(&mut h, a, a);
    assert_eq!(selection(&h), ["a"], "the second click selects the member");
    let b = centre(&h, "b");
    press_release(&mut h, b, b);
    assert_eq!(selection(&h), ["b"], "inside the group a sibling is picked directly");
    // Back out, then drag a member: the whole group moves, in one undo step.
    h.state_mut().session.run("edit.selectNone", &json!({})).unwrap();
    h.run_steps(2);
    let (ax, bx) = (x_of(&h, "a"), x_of(&h, "b"));
    let from = centre(&h, "a");
    press_release(&mut h, from, from + vec2(60.0, 0.0));
    assert_eq!(selection(&h), ["g"]);
    let moved = x_of(&h, "a") - ax;
    assert!(moved > 10.0, "a moved by {moved}");
    assert!((x_of(&h, "b") - bx - moved).abs() < 1e-9, "b moved with it");
    assert_eq!(h.state().session.undo_label(), Some("Move"));
}
