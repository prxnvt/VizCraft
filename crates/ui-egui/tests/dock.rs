//! The bottom dock through the app: Problems rows, select, fix, suppress, Search, Activity,
//! `ui.dock`, `ui.search` and `ui.inspect`, under egui_kittest.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_engine::{Origin, Session};
use vizcraft_ui_egui::dock::{DockLayout, DockTab, MAX_HEIGHT, MIN_HEIGHT};
use vizcraft_ui_egui::{CraftApp, Services, UI_COMMANDS, control};

/// A page that routes by hand with a connector across a shape and one with a loose end, held to
/// the drawing ruleset: a crossing (found by a check) and an unglued end (found live).
fn drawing() -> CraftApp {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    s.run("page.setRouting", &json!({"avoid": "never"})).unwrap();
    for (id, x) in [("a", 100), ("b", 300), ("c", 500)] {
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": x, "y": 100, "text": id.to_uppercase(), "id": id})).unwrap();
    }
    s.run("connector.add", &json!({"from": {"shape": "a", "point": 1}, "to": {"shape": "c", "point": 3}, "route": "straight", "id": "ac"})).unwrap();
    s.run("connector.add", &json!({"from": {"shape": "c", "point": 2}, "to": {"x": 560, "y": 260}, "id": "loose"})).unwrap();
    s.run("validate.setProfile", &json!({"rulesets": ["drawing.tidy"]})).unwrap();
    CraftApp::new(s, Services::default())
}

fn harness(app: CraftApp) -> Harness<'static, CraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn inspect(h: &mut Harness<'static, CraftApp>) -> Value {
    h.state_mut().run("ui.inspect", json!({})).ok();
    let ctx = egui::Context::default();
    control::inspect(h.state(), &ctx)["dock"].clone()
}

#[test]
fn a_planted_crossing_is_a_row_and_a_click_selects_it() {
    let mut h = harness(drawing());
    h.state_mut().run("ui.dock", json!({"tab": "problems"})).unwrap();
    h.run_steps(3);
    assert_eq!(inspect(&mut h)["rows"], 1, "only the live unglued end before a check");
    h.get_by_label("Check").click();
    h.run_steps(3);
    let d = inspect(&mut h);
    assert_eq!((d["rows"].as_u64(), d["tab"].as_str()), (Some(2), Some("problems")));
    let r = h.state_mut().run("validate.run", json!({})).unwrap();
    let crossing = r["issues"].as_array().unwrap().iter().find(|i| i["rule"] == "drawing.connectorCrossesShape").unwrap().clone();
    let message = crossing["message"].as_str().unwrap().to_string();
    h.get_by_label(&message).click();
    h.run_steps(3);
    let mut sel = h.state().session.sel.items.clone();
    sel.sort();
    let mut want: Vec<String> = crossing["items"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    want.sort();
    assert_eq!(sel, want, "the row selects the issue's items");
    assert!(sel.contains(&"ac".to_string()), "the crossing connector is selected");
}

#[test]
fn a_control_origin_validate_run_fills_the_rows_without_opening_the_dock() {
    let mut h = harness(drawing());
    h.state_mut().ui.dock.open = false;
    assert_eq!(inspect(&mut h)["rows"], 1, "only the live unglued end before a check");
    h.state_mut().run_as(Origin::Control, "validate.run", json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(inspect(&mut h)["rows"], 2, "the control run's findings are rows");
    assert!(!h.state().ui.dock.open, "only a UI-origin run opens the dock");
    h.state_mut().run("validate.run", json!({})).unwrap();
    assert!(h.state().ui.dock.open && h.state().ui.dock.tab == DockTab::Problems);
}

#[test]
fn the_fix_button_runs_its_command_as_one_step_and_clears_the_issue() {
    let mut h = harness(drawing());
    h.state_mut().run("ui.dock", json!({"tab": "problems", "open": true})).unwrap();
    h.run_steps(3);
    h.get_by_label("Check").click();
    h.run_steps(3);
    let r = h.state_mut().run("validate.run", json!({})).unwrap();
    let crossing = r["issues"].as_array().unwrap().iter().find(|i| i["rule"] == "drawing.connectorCrossesShape").unwrap().clone();
    let label = crossing["fixes"][0]["label"].as_str().unwrap().to_string();
    let steps = h.state().session.transactions().len();
    h.get_by_label(&label).click();
    h.run_steps(4);
    assert_eq!(h.state().session.transactions().len(), steps + 1, "one undo step");
    let r = h.state_mut().run("validate.run", json!({})).unwrap();
    assert!(r["issues"].as_array().unwrap().iter().all(|i| i["rule"] != "drawing.connectorCrossesShape"), "the crossing is gone");
    h.run_steps(2);
    assert_eq!(inspect(&mut h)["rows"], 1, "the row left the list");
}

#[test]
fn suppress_hides_exactly_one_row() {
    let mut h = harness(drawing());
    h.state_mut().run("ui.dock", json!({"tab": "problems", "open": true})).unwrap();
    h.run_steps(3);
    h.get_by_label("Check").click();
    h.run_steps(3);
    assert_eq!(inspect(&mut h)["rows"], 2);
    let first = h.query_all_by_label("Suppress").next().unwrap();
    for l in ["Message", "Rule", "Page", "Items", "Explain"] {
        for n in h.query_all_by_label(l) {
            eprintln!("RECT {l} {:?}", n.rect());
        }
    }
    first.click();
    h.run_steps(3);
    h.get_by_label("Confirm").click();
    h.run_steps(4);
    assert_eq!(h.state().session.doc.conformance.suppressions.len(), 1);
    assert_eq!(inspect(&mut h)["rows"], 1);
}

#[test]
fn checking_off_says_so_and_turn_on_enables_it() {
    let mut app = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
    app.run("ui.dock", json!({"tab": "problems"})).unwrap();
    let mut h = harness(app);
    h.get_by_label("Checking is off");
    h.get_by_label("Turn On").click();
    h.run_steps(3);
    assert!(h.state().session.doc.conformance.enabled);
    assert!(h.query_by_label("Checking is off").is_none());
}

#[test]
fn search_lists_a_text_hit_and_selects_it() {
    let mut h = harness(drawing());
    let r = h.state_mut().run("ui.search", json!({"query": "B"})).unwrap();
    assert_eq!(r["hits"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(r["hits"][0]["id"], "b");
    h.run_steps(3);
    let d = inspect(&mut h);
    assert_eq!((d["open"].as_bool(), d["tab"].as_str(), d["rows"].as_u64()), (Some(true), Some("search"), Some(1)));
    h.get_by_label("B").click();
    h.run_steps(3);
    assert_eq!(h.state().session.sel.items, ["b"]);
    assert!(h.state_mut().run("ui.search", json!({"query": "B", "scope": "nowhere"})).is_err());
}

#[test]
fn activity_lists_a_control_batch_and_undoes_it() {
    let mut h = harness(drawing());
    let before = h.state().session.doc.clone();
    let batch = json!({"label": "Agent edit", "commands": [
        {"command": "shape.move", "params": {"ids": ["a"], "dx": 40}},
        {"command": "shape.move", "params": {"ids": ["b"], "dx": 40}},
    ]});
    h.state_mut().run_as(Origin::Control, "edit.batch", batch).unwrap();
    h.state_mut().run("ui.dock", json!({"tab": "activity"})).unwrap();
    h.run_steps(3);
    assert!(h.query_all_by_label("Agent edit").count() >= 1);
    h.get_by_label("control");
    assert_ne!(h.state().session.doc, before);
    h.query_all_by_label("Undo to before this").next().unwrap().click();
    h.run_steps(4);
    assert_eq!(h.state().session.doc, before, "the batch is undone");
    // The undone entry is shown as undone and cannot be undone to again.
    h.get_by_label("undone");
}

#[test]
fn ui_dock_opens_toggles_and_clamps() {
    let mut app = drawing();
    let r = app.run("ui.dock", json!({})).unwrap();
    assert_eq!(r["open"], true, "naming nothing toggles");
    let r = app.run("ui.dock", json!({})).unwrap();
    assert_eq!(r["open"], false);
    let r = app.run("ui.dock", json!({"tab": "tables"})).unwrap();
    assert_eq!((r["open"].as_bool(), r["tab"].as_str()), (Some(true), Some("tables")));
    let r = app.run("ui.dock", json!({"height": 1.0e9})).unwrap();
    assert_eq!(r["height"].as_f64(), Some(MAX_HEIGHT as f64));
    let r = app.run("ui.dock", json!({"height": -5})).unwrap();
    assert_eq!(r["height"].as_f64(), Some(MIN_HEIGHT as f64));
    assert!(app.run("ui.dock", json!({"tab": "ghost"})).is_err());
    for id in ["ui.dock", "ui.search"] {
        assert!(UI_COMMANDS.iter().any(|(i, ..)| *i == id), "{id} is listed");
    }
}

#[test]
fn the_layout_persists_and_junk_falls_back() {
    let mut a = drawing();
    a.run("ui.dock", json!({"tab": "activity", "height": 300})).unwrap();
    let saved = serde_json::to_value(a.prefs()).unwrap();
    assert_eq!(saved["dock"], json!({"open": true, "height": 300.0, "tab": "activity"}));
    let mut b = drawing();
    b.apply_prefs(serde_json::from_value(saved).unwrap());
    assert_eq!(b.ui.dock.tab, DockTab::Activity);
    let junk: DockLayout = serde_json::from_value(json!({"open": true, "tab": "nonsense", "height": 7})).unwrap();
    assert_eq!(junk.tab, DockTab::Problems);
    let mut c = drawing();
    c.run("ui.dock", json!({"open": true})).unwrap();
    c.ui.dock.height = f32::NAN;
    let mut h = harness(c);
    h.run_steps(2);
    assert!(h.state().ui.dock.height.is_finite());
}

#[test]
fn the_ribbon_check_opens_the_dock_on_problems() {
    let mut app = drawing();
    app.run("ui.tab", json!({"tab": "Process"})).unwrap();
    let mut h = harness(app);
    assert!(!h.state().ui.dock.open);
    h.get_by_label("Validate").click();
    h.run_steps(4);
    let d = inspect(&mut h);
    assert_eq!((d["open"].as_bool(), d["tab"].as_str(), d["rows"].as_u64()), (Some(true), Some("problems"), Some(2)));
}
