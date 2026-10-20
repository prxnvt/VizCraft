//! The fluid power groups of the notation tab under egui_kittest: they show only on a fluid
//! power page; a Lines button arms the Pipe tool with its line type and a drag commits one
//! `wire.draw` of that type; P with no line armed draws the pack's working line; the Inspector's
//! step chart grid runs `fluid.setStep`; every icon the tab uses is drawn, and the tab fits
//! 1280 points wide. Assertions read the session and `ui.inspect`.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_diagram::Point;
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::{CraftApp, Services};

fn harness() -> Harness<'static, CraftApp> {
    harness_w(1600.0)
}

fn harness_w(width: f32) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(Document::new()), Services::default());
    app.run("ui.pane", json!({"name": "shapeData", "value": true})).unwrap();
    let mut h = Harness::builder().with_size(vec2(width, 1000.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn run(h: &mut Harness<'_, CraftApp>, id: &str, v: Value) -> Value {
    let r = h.state_mut().run(id, v).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(2);
    r
}

fn tab_ids(h: &Harness<'_, CraftApp>) -> Vec<&'static str> {
    vizcraft_ui_egui::ribbon::tabs(h.state()).iter().filter(|t| t.visible(h.state())).flat_map(|t| t.ids()).collect()
}

fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

fn drag(h: &mut Harness<'_, CraftApp>, from: Point, to: Point) {
    let (from, to) = (screen(h, from), screen(h, to));
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

/// Where connection point `i` of `shape` is on the page.
fn pin(h: &Harness<'_, CraftApp>, shape: &str, i: usize) -> Point {
    h.state().session.doc.page().unwrap().shape(shape).unwrap().connection_point_world(i).unwrap()
}

#[test]
fn the_circuit_groups_show_only_on_a_fluid_power_page() {
    let mut h = harness();
    run(&mut h, "template.new", json!({"template": "flowchart.flowchart"}));
    assert!(!tab_ids(&h).contains(&"fluid.renumber"));
    run(&mut h, "template.new", json!({"template": "fluidpower.hydraulicCircuit"}));
    let ids = tab_ids(&h);
    for id in ["fluid.renumber", "fluid.trace", "fluid.stepChart", "fluid.powerUnit", "fluid.setPortSet", "ui.valve"] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
}

#[test]
fn a_line_button_arms_the_pipe_tool_and_a_drag_draws_that_line() {
    let mut h = harness();
    run(&mut h, "template.new", json!({"template": "fluidpower.hydraulicCircuit"}));
    let pump = run(&mut h, "fluid.insert", json!({"type": "fluidpower.HydraulicPump", "x": 300, "y": 500}));
    let valve = run(&mut h, "fluid.insert", json!({"type": "fluidpower.HydraulicValve", "x": 500, "y": 250}));
    run(&mut h, "view.zoomFit", json!({}));
    let (pump, valve) = (pump["shape"].as_str().unwrap().to_string(), valve["shape"].as_str().unwrap().to_string());
    let r = run(&mut h, "ui.tool", json!({"tool": "pipe", "line": "fluidpower.pilot"}));
    assert_eq!(r["line"], "fluidpower.pilot");
    assert!(h.state_mut().run("ui.tool", json!({"tool": "pipe", "line": "nope.nope"})).is_err());
    run(&mut h, "ui.tool", json!({"tool": "pipe", "line": "fluidpower.pilot"}));
    let before = h.state().session.doc.page().unwrap().connectors.len();
    let (a, b) = (pin(&h, &pump, 0), pin(&h, &valve, 0));
    drag(&mut h, a, b);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.connectors.len(), before + 1);
    let line = page.connectors.last().unwrap();
    assert_eq!(line.kind, "fluidpower.pilot");
    // The hand-drawn line is the model object the rules count.
    let rel = h.state().session.doc.model.relationship(line.relationship.as_deref().unwrap()).unwrap();
    assert_eq!(rel.type_id, "fluidpower.PilotLine");
    // Escape returns to the pointer.
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().tool, vizcraft_ui_egui::Tool::Pointer);
    // P with no line armed draws the pack's first fluid line type, the working line.
    run(&mut h, "ui.pipe", json!({}));
    assert!(h.state().line_type.is_none());
    let s = pin(&h, &pump, 1);
    drag(&mut h, s, Point::new(s.x, s.y + 80.0));
    assert_eq!(h.state().session.doc.page().unwrap().connectors.last().unwrap().kind, "fluidpower.working");
}

#[test]
fn the_step_chart_grid_runs_set_step() {
    let mut h = harness();
    run(&mut h, "example.open", json!({"example": "fluidpower.pneumaticExample"}));
    let chart = {
        let doc = &h.state().session.doc;
        let e = doc.model.elements.iter().find(|e| e.type_id == "fluidpower.StepChart").unwrap().id.clone();
        doc.page().unwrap().shapes.iter().find(|s| s.element.as_deref() == Some(e.as_str())).unwrap().id.clone()
    };
    run(&mut h, "edit.select", json!({"ids": [chart]}));
    h.run_steps(4);
    let report = vizcraft_ui_egui::control::inspect(h.state(), &h.ctx);
    assert_eq!(report["inspector"]["fields"]["stepChart"]["rows"][0]["states"], json!([0, 1, 0]));
    // An accessibility click: the grid cell can sit below the fold of the Inspector where a pointer click would miss it.
    h.get_by_label("1A1 at step 3").click_accesskit();
    h.run_steps(4);
    let report = vizcraft_ui_egui::control::inspect(h.state(), &h.ctx);
    assert_eq!(report["inspector"]["fields"]["stepChart"]["rows"][0]["states"], json!([0, 1, 1]));
    let last = h.state().session.transactions().back().unwrap().commands.clone();
    assert_eq!(last, ["fluid.setStep"]);
}

#[test]
fn the_fluid_power_tab_draws_every_icon_and_fits_1280_wide() {
    let mut h = harness_w(1280.0);
    run(&mut h, "template.new", json!({"template": "fluidpower.pneumaticCircuit"}));
    run(&mut h, "ui.tab", json!({"tab": "Fluid Power"}));
    h.run_steps(4);
    let tabs = vizcraft_ui_egui::ribbon::tabs(h.state());
    let tab = tabs.iter().find(|t| t.name == "Fluid Power").unwrap();
    let mut used = Vec::new();
    for g in &tab.groups {
        for c in &g.controls {
            c.icons(&mut used);
        }
    }
    for icon in used {
        assert!(vizcraft_ui_egui::icons::known(icon), "icon {icon} is not drawn");
    }
    let labels: Vec<&str> = tab.groups.iter().map(|g| g.label).collect();
    assert_eq!(labels[labels.len() - 5..], ["Symbols", "Lines", "Codes", "Sequence", "Analyze"]);
    assert!(!labels.contains(&"Key"), "{labels:?}");
    let o = vizcraft_ui_egui::ribbon::Overflow::get(&h.ctx);
    assert!(!o.overflows(), "the Fluid Power tab overflows at 1280: {o:?}");
}
