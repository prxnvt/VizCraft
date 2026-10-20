//! The electrical pack in the app under egui_kittest: its groups on the notation tab (only on an
//! electrical page, every control a command, every icon drawn, the tab fitting 1280 wide), the
//! Ladder tool (a drag is one `ladder.insert`, Escape returns to the pointer), a device dropped
//! in a ladder set on a rung in one undo step, the Wire tool drawing the pack's wire line, the
//! Inspector's terminal grid adding a terminal, and the Tables wire list. Assertions read the
//! session and the app state, never pixels.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_diagram::Point;
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::ribbon::{self, Overflow};
use vizcraft_ui_egui::{CraftApp, Services, Tool, icons, widgets};

/// A control circuit document on the given tab, `w` wide.
fn harness(w: f32, tab: &str) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(Document::new()), Services::default());
    app.session.run("template.new", &json!({"template": "electrical.controlCircuit"})).unwrap();
    app.ui.tab = tab.into();
    let mut h = Harness::builder().with_size(vec2(w, 900.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

fn run(h: &mut Harness<'_, CraftApp>, id: &str, v: serde_json::Value) -> serde_json::Value {
    let r = h.state_mut().run(id, v).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(2);
    r
}

fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

fn centre(h: &Harness<'_, CraftApp>) -> Point {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).unwrap();
    let c = vp.screen_to_world(app.canvas.rect.unwrap().center());
    Point::new(f64::from(c.x).round(), f64::from(c.y).round())
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

fn count(h: &Harness<'_, CraftApp>, ty: &str) -> usize {
    h.state().session.doc.model.elements.iter().filter(|e| e.type_id == ty).count()
}

#[test]
fn the_electrical_groups_show_only_on_an_electrical_page_and_resolve() {
    let h = harness(1280.0, "Electrical");
    let tabs = ribbon::tabs(h.state());
    let tab = tabs.iter().find(|t| t.name == "Electrical").expect("the notation tab is named after the pack");
    for g in ["Standard", "Ladder", "Devices", "Lists"] {
        assert!(tab.groups.iter().any(|x| x.label == g), "{g} group missing");
    }
    for id in tab.ids() {
        assert!(widgets::exists(h.state(), id), "{id} is not a command");
    }
    let mut used = Vec::new();
    for g in &tab.groups {
        for c in &g.controls {
            c.icons(&mut used);
        }
    }
    for icon in used {
        assert!(icons::known(icon), "icon {icon} is not drawn");
    }
    assert!(!Overflow::get(&h.ctx).overflows(), "the Electrical tab overflows at 1280");
    // A flowchart page's notation tab has none of them.
    let mut app = CraftApp::new(Session::new(Document::new()), Services::default());
    app.session.run("template.new", &json!({"template": "flowchart.flowchart"})).unwrap();
    let tabs = ribbon::tabs(&app);
    assert!(tabs.iter().all(|t| !t.groups.iter().any(|g| g.label == "Ladder")));
}

#[test]
fn the_symbols_menu_switches_the_standard_in_one_step() {
    let mut h = harness(1440.0, "Electrical");
    run(&mut h, "element.create", json!({"type": "electrical.Coil", "x": 200, "y": 200}));
    let depth = h.state().session.undo_depth();
    h.get_by_label("Symbol Set").click();
    h.run_steps(3);
    h.get_by_label("JIC").click();
    h.run_steps(4);
    let s = &h.state().session;
    assert_eq!(s.doc.standards.symbols, "jic");
    assert_eq!(s.undo_depth(), depth + 1);
}

#[test]
fn a_ladder_tool_drag_inserts_one_ladder_and_escape_returns_to_the_pointer() {
    let mut h = harness(1440.0, "Electrical");
    run(&mut h, "ui.tool", json!({"tool": "ladder"}));
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().tool, Tool::Pointer, "Escape returns to the pointer");
    run(&mut h, "ui.tool", json!({"tool": "ladder"}));
    let c = centre(&h);
    let depth = h.state().session.undo_depth();
    let (a, b) = (screen(&h, Point::new(c.x - 200.0, c.y - 150.0)), screen(&h, Point::new(c.x + 200.0, c.y + 150.0)));
    drag(&mut h, a, b);
    assert_eq!(count(&h, "electrical.Ladder"), 1);
    assert_eq!(h.state().session.undo_depth(), depth + 1, "one undo step");
    assert_eq!(h.state().tool, Tool::Pointer);
    let rails = h.state().session.doc.model.nets.iter().filter(|n| n.name == "L1" || n.name == "L2").count();
    assert_eq!(rails, 2);
}

#[test]
fn a_device_dropped_in_a_ladder_sits_on_a_rung_in_one_undo_step() {
    let mut h = harness(1440.0, "Electrical");
    let r = run(&mut h, "ladder.insert", json!({"x": 100, "y": 100, "rungs": 5}));
    let frame = h.state().session.doc.page().unwrap().shape(r["shape"].as_str().unwrap()).unwrap().clone();
    let depth = h.state().session.undo_depth();
    let y = frame.y + frame.h * 0.5;
    vizcraft_ui_egui::canvas::interact::membership::insert(h.state_mut(), "electrical.coil", frame.x + 150.0, y - 20.0, 40.0, 40.0);
    h.run_steps(3);
    let s = &h.state().session;
    assert_eq!(s.undo_depth(), depth + 1, "add and place are one step");
    let coil = s.doc.model.elements.iter().find(|e| e.type_id == "electrical.Coil").unwrap();
    assert_eq!(coil.prop("rung").and_then(|v| v.as_f64()), Some(3.0), "the middle rung of five");
}

#[test]
fn the_wire_tool_draws_the_packs_wire_and_an_armed_line() {
    let mut h = harness(1440.0, "Electrical");
    let c = centre(&h);
    run(&mut h, "ui.wire", json!({}));
    let (a, b) = (screen(&h, Point::new(c.x - 100.0, c.y)), screen(&h, Point::new(c.x + 100.0, c.y)));
    drag(&mut h, a, b);
    let kinds = |h: &Harness<'_, CraftApp>| h.state().session.doc.page().unwrap().connectors.iter().map(|c| c.kind.clone()).collect::<Vec<_>>();
    assert_eq!(kinds(&h), ["electrical.wire"]);
    let r = run(&mut h, "ui.tool", json!({"tool": "wire", "line": "electrical.bus"}));
    assert_eq!(r["line"], "electrical.bus");
    let (a, b) = (screen(&h, Point::new(c.x - 100.0, c.y + 100.0)), screen(&h, Point::new(c.x + 100.0, c.y + 100.0)));
    drag(&mut h, a, b);
    assert!(kinds(&h).contains(&"electrical.bus".to_string()));
    assert!(h.state_mut().run("ui.tool", json!({"tool": "wire", "line": "basic.arrow"})).is_err(), "a line must draw a net");
}

#[test]
fn the_inspector_adds_a_terminal_and_the_tables_show_the_wire_list() {
    let mut h = harness(1440.0, "Electrical");
    run(&mut h, "ui.pane", json!({"pane": "shapeData", "open": true}));
    let r = run(&mut h, "terminal.strip", json!({"x": 200, "y": 200, "count": 3}));
    run(&mut h, "edit.select", json!({"ids": [r["shape"]]}));
    h.run_steps(4);
    let depth = h.state().session.undo_depth();
    // An accessibility click: the button sits below the fold of the Inspector on a short or
    // differently metricked window (the Linux runner), where a pointer click would miss it.
    h.get_by_label("Add Terminal").click_accesskit();
    h.run_steps(4);
    assert_eq!(count(&h, "electrical.Terminal"), 4);
    assert_eq!(h.state().session.undo_depth(), depth + 1);
    run(&mut h, "wire.draw", json!({"from": {"x": 600, "y": 600}, "to": {"x": 700, "y": 600}}));
    let t = run(&mut h, "ui.tables", json!({"subject": "connections", "kind": "wire"}));
    assert_eq!((t["subject"].as_str(), t["count"].as_u64()), (Some("connections"), Some(1)));
}

#[test]
fn tab_and_enter_do_nothing_on_an_electrical_page() {
    let mut h = harness(1440.0, "Electrical");
    let r = run(&mut h, "element.create", json!({"type": "electrical.Coil", "x": 200, "y": 200}));
    run(&mut h, "edit.select", json!({"ids": [r["shape"]]}));
    let before = h.state().session.doc.model.elements.len();
    for id in ["ui.insertNext", "ui.insertSibling"] {
        let out = run(&mut h, id, json!({}));
        assert_eq!(out["done"], false, "{id}: {out}");
    }
    assert_eq!(h.state().session.doc.model.elements.len(), before);
}
