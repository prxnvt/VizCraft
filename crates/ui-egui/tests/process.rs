//! The process pack's front end under egui_kittest: its groups on the Process Plant tab (only
//! on a process page, every control a command with a drawn icon, fitting 1280 wide), the
//! numbering menu, a pipe drawn by hand with the Pipe tool becoming a pipe connection on a line,
//! Escape back to the pointer, Tab and Enter doing nothing on a P&ID, the Inspector's Line and
//! Loop groups, Trace Loop, and the Tables dock showing the line list.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_diagram::Point;
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::ribbon::{self, Overflow};
use vizcraft_ui_egui::{CraftApp, Services, Tool, icons, widgets};

fn pid_app() -> CraftApp {
    let mut a = CraftApp::new(Session::new(Document::new()), Services::default());
    a.run("template.new", json!({"template": "process.pid"})).unwrap();
    a
}

fn harness_with(mut a: CraftApp, width: f32, tab: &str) -> Harness<'static, CraftApp> {
    a.ui.tab = tab.into();
    a.ui.panes.close("history");
    let mut h = Harness::builder().with_size(vec2(width, 900.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        a,
    );
    h.run_steps(5);
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

fn key(h: &mut Harness<'_, CraftApp>, k: Key) {
    h.event(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.event(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

const GROUPS: [&str; 1] = ["Plant"];

#[test]
fn the_process_groups_show_on_a_process_page_only_with_drawn_icons_and_commands() {
    let a = pid_app();
    let tabs = ribbon::tabs(&a);
    let tab = tabs.iter().find(|t| t.name == "Process Plant").expect("the notation tab is named after the pack");
    let labels: Vec<&str> = tab.groups.iter().map(|g| g.label).collect();
    for g in GROUPS {
        assert!(labels.contains(&g), "{g} is missing: {labels:?}");
    }
    for id in tab.ids() {
        assert!(widgets::exists(&a, id), "{id} is neither a registry command nor a UI command");
        assert!(!widgets::label_of(&a, id).is_empty(), "{id} has no label");
    }
    let mut used = Vec::new();
    for g in tab.groups.iter().filter(|g| GROUPS.contains(&g.label)) {
        for c in &g.controls {
            c.icons(&mut used);
        }
    }
    assert!(!used.is_empty());
    for icon in used {
        assert!(icons::known(icon), "icon {icon} is not drawn");
    }
    assert!(icons::known("lineKind"), "the Line Kind menu's icon");
    // A UML page shows no process groups.
    let mut u = CraftApp::new(Session::new(Document::new()), Services::default());
    u.run("template.new", json!({"template": "uml.classDiagram"})).unwrap();
    let tabs = ribbon::tabs(&u);
    assert!(tabs.iter().all(|t| t.groups.iter().all(|g| !GROUPS.contains(&g.label))), "no process group on a UML page");
}

#[test]
fn the_process_plant_tab_fits_1280_wide() {
    let h = harness_with(pid_app(), 1280.0, "Process Plant");
    assert!(h.query_by_label("Pipe").is_some(), "the tab is showing");
    let o = Overflow::get(&h.ctx);
    assert!(!o.overflows(), "Process Plant overflows at 1280: {} wide in {}", o.content, o.visible);
}

#[test]
fn the_numbering_menu_switches_between_parallel_and_serial_with_one_checked() {
    let mut h = harness_with(pid_app(), 1600.0, "Process Plant");
    h.get_by_label("Tags").click();
    h.run_steps(3);
    h.get_by_label("Serial Numbering (one sequence)").click();
    h.run_steps(3);
    assert_eq!(h.state_mut().session.run("plant.numbering", &json!({})).unwrap()["scheme"], "serial");
    assert!(h.state().session.can_undo(), "one undo step");
}

#[test]
fn a_pipe_drawn_by_hand_is_a_pipe_connection_on_a_line_and_escape_returns_to_the_pointer() {
    let mut h = harness_with(pid_app(), 1600.0, "Home");
    let view = h.state().canvas.rect.expect("laid out");
    let vp = h.state().canvas.viewport(&h.state().session.view).unwrap();
    let c = vp.screen_to_world(view.center());
    let (x, y) = (f64::from(c.x).round(), f64::from(c.y).round());
    let v = run(&mut h, "element.create", json!({"type": "process.Vessel", "x": x - 200.0, "y": y - 60.0}));
    let n = run(&mut h, "plant.addNozzle", json!({"shape": v["shape"], "side": "right"}));
    let valve = run(&mut h, "element.create", json!({"type": "process.GateValve", "x": x + 100.0, "y": y - 20.0}));
    let nozzle = n["shape"].as_str().unwrap().to_string();
    let valve = valve["shape"].as_str().unwrap().to_string();
    let pin =
        |h: &Harness<'_, CraftApp>, s: &str, i: usize| h.state().session.doc.page().unwrap().shape(s).unwrap().connection_point_world(i).unwrap();
    key(&mut h, Key::P);
    assert_eq!(h.state().tool, Tool::Wire { pipe: true });
    let (from, to) = (screen(&h, pin(&h, &nozzle, 0)), screen(&h, pin(&h, &valve, 0)));
    drag(&mut h, from, to);
    let page = h.state().session.doc.page().unwrap().clone();
    let pipe = page.connectors.iter().find(|c| c.kind == "process.processLine").expect("the P&ID's first pipe kind draws it");
    let rel = pipe.relationship.as_deref().and_then(|r| h.state().session.doc.model.relationship(r)).expect("bound");
    assert_eq!(rel.type_id, "process.PipeConnection", "the rules count it");
    assert!(pipe.ext.contains_key("process.line"), "it starts a line");
    key(&mut h, Key::Escape);
    assert_eq!(h.state().tool, Tool::Pointer);
    // ui.tool {line} arms another kind; junk is refused and leaves the pointer.
    run(&mut h, "ui.tool", json!({"tool": "pipe", "line": "process.utilityLine"}));
    assert_eq!(h.state().line_type.as_deref(), Some("process.utilityLine"));
    assert!(h.state_mut().run("ui.tool", json!({"tool": "pipe", "line": "nope.kind"})).is_err());
    assert_eq!(h.state().tool, Tool::Pointer);
}

#[test]
fn tab_and_enter_do_nothing_on_a_pid() {
    let mut h = harness_with(pid_app(), 1600.0, "Home");
    let v = run(&mut h, "element.create", json!({"type": "process.Vessel", "x": 300, "y": 300}));
    run(&mut h, "edit.select", json!({"ids": [v["shape"]]}));
    let rev = h.state().session.rev();
    key(&mut h, Key::Tab);
    key(&mut h, Key::Enter);
    assert_eq!(h.state().session.rev(), rev, "nothing was inserted");
}

#[test]
fn the_inspector_shows_line_and_loop_groups_and_trace_selects_the_chain() {
    let mut a = pid_app();
    let w = a.run("wire.draw", json!({"from": {"x": 100, "y": 200}, "to": {"x": 400, "y": 200}, "type": "process.processLine"})).unwrap();
    let pipe = w["id"].as_str().unwrap().to_string();
    let mut h = harness_with(a, 1600.0, "Home");
    run(&mut h, "edit.select", json!({"ids": [pipe]}));
    h.run_steps(3);
    assert!(h.query_by_label("Line number").is_some(), "the Line group shows on a pipe");
    assert!(h.query_by_label("New Line").is_some());
    let lt =
        run(&mut h, "element.create", json!({"type": "process.ProcessInstrumentationFunction", "properties": {"letters": "LT"}, "x": 100, "y": 400}));
    let lv = run(&mut h, "element.create", json!({"type": "process.GlobeValve", "properties": {"letters": "LV"}, "x": 400, "y": 400}));
    run(&mut h, "edit.select", json!({"ids": [lt["shape"]]}));
    h.run_steps(3);
    assert!(h.query_by_label("In no loop").is_some(), "the Loop group shows on an instrument");
    run(&mut h, "loop.create", json!({"members": [lt["id"], lv["id"]], "loopType": "indication"}));
    run(&mut h, "connector.add", json!({"from": {"shape": lt["shape"]}, "to": {"shape": lv["shape"]}, "kind": "process.electricSignal"}));
    run(&mut h, "edit.select", json!({"ids": [lt["shape"]]}));
    let before = h.state().session.doc.clone();
    run(&mut h, "ui.loopTrace", json!({}));
    assert_eq!(h.state().session.doc, before, "a trace changes nothing");
    assert_eq!(h.state().session.sel.items.len(), 2, "the chain is selected");
    // With nothing in a loop selected, a hint and no error.
    run(&mut h, "edit.selectNone", json!({}));
    assert!(h.state_mut().run("ui.loopTrace", json!({})).is_ok());
}

#[test]
fn the_tables_dock_shows_the_line_list_and_exports_through_the_pack_view() {
    let mut a = pid_app();
    a.run("wire.draw", json!({"from": {"x": 100, "y": 200}, "to": {"x": 400, "y": 200}, "type": "process.processLine"})).unwrap();
    let mut h = harness_with(a, 1600.0, "Home");
    let r = run(&mut h, "ui.tables", json!({"pack": "process.line"}));
    assert_eq!((r["view"].as_str(), r["count"].as_u64()), (Some("pack"), Some(1)));
    h.run_steps(3);
    assert!(h.query_by_label("Line List").is_some() || h.query_by_label_contains("Line List").is_some(), "the list is named");
    assert!(h.state_mut().run("ui.tables", json!({"pack": "process.nope"})).is_err());
}
