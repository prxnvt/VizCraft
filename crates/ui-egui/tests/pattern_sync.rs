//! The Patterns pane's Sync button under egui_kittest: on an instance of a table-driven pattern it
//! reads the rows back from the diagram into the form, and Apply then regenerates from them.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services, control};

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

fn click(h: &mut Harness<'_, CraftApp>, label: &str) {
    let node = h.query_all_by_role_and_label(egui::accesskit::Role::Button, label).last().unwrap_or_else(|| panic!("no {label} button"));
    node.click();
    h.run_steps(4);
}

#[test]
fn sync_reads_the_lanes_back_into_the_form() {
    let mut a = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
    a.ui.in_window_menus = true;
    a.integrated_titlebar = false;
    let r = a.run("pattern.insert", json!({"pattern": "table.raciLanes", "x": 60, "y": 60})).unwrap();
    let draft = r["members"]["step.001"].as_str().unwrap().to_string();
    let iid = r["instance"].as_str().unwrap().to_string();
    a.run("ui.pane.patterns", json!({"value": true})).unwrap();
    // Move the first step into the Director lane by hand.
    let page = a.session.doc.pages.get_mut(0).unwrap();
    let director = page.shapes.iter().find(|s| s.text == "Director" && s.container.is_some()).unwrap().id.clone();
    let step = page.shapes.iter_mut().find(|s| s.element.as_deref() == Some(draft.as_str())).unwrap();
    step.parent = Some(director);
    let step = step.id.clone();
    a.run("edit.select", json!({"ids": [step]})).unwrap();
    let mut h = harness(a);
    assert!(h.query_by_label("Selected: RACI Swimlanes").is_some());
    assert_eq!(control::inspect(h.state(), &h.ctx)["patterns"]["sync"], true);
    click(&mut h, "Sync");
    let form = h.state().patterns.form.clone().unwrap();
    assert!(form.error.is_empty(), "{}", form.error);
    assert_eq!(form.values["rows"][0][1], "Director", "the form holds the lane read back from the diagram");
    click(&mut h, "Apply");
    let inst = h.state().session.doc.model.pattern(&iid).unwrap().clone();
    assert_eq!(inst.args["rows"][0][1], "Director", "Apply regenerated the instance from the synced rows");
}

#[test]
fn a_workflow_instance_has_no_sync_button() {
    let mut a = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
    a.ui.in_window_menus = true;
    a.integrated_titlebar = false;
    let r = a.run("pattern.insert", json!({"pattern": "workflow.parallelSplit"})).unwrap();
    a.run("ui.pane.patterns", json!({"value": true})).unwrap();
    a.run("edit.select", json!({"ids": [r["group"]]})).unwrap();
    let h = harness(a);
    assert!(h.query_by_label("Selected: Parallel Split").is_some());
    assert!(h.query_all_by_role_and_label(egui::accesskit::Role::Button, "Sync").next().is_none());
    assert_eq!(control::inspect(h.state(), &h.ctx)["patterns"]["sync"], false);
}
