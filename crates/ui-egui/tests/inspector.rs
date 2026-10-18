//! The Inspector pane through the app: typed editors from the element schema, the live tag
//! check, Standard and Expert modes, the Views and Relationships lists, the Format tab and the
//! Document section, under egui_kittest.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_diagram::Document;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness() -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(Document::new()), Services::default());
    app.run("ui.pane", json!({"name": "shapeData", "value": true})).unwrap();
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

fn run(h: &mut Harness<'static, CraftApp>, id: &str, params: Value) -> Value {
    let r = h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
    r
}

/// A flowchart with two steps (S1 Mix, S2 Heat) and a third tagged `S`, the second selected.
fn flowchart(h: &mut Harness<'static, CraftApp>) -> String {
    run(h, "element.create", json!({"type": "flowchart.Process", "name": "Mix", "tag": "S1", "id": "mix", "x": 0, "y": 0}));
    run(h, "element.create", json!({"type": "flowchart.Process", "name": "Heat", "tag": "S2", "id": "heat", "x": 0, "y": 150}));
    run(h, "element.create", json!({"type": "flowchart.PredefinedProcess", "name": "Rinse", "tag": "S", "id": "rinse", "x": 0, "y": 300}));
    run(h, "relationship.create", json!({"from": "mix", "to": "heat", "id": "f1"}));
    let shape = run(h, "element.get", json!({"id": "heat"}))["views"][0]["items"][0].as_str().unwrap_or_default().to_string();
    run(h, "edit.select", json!({"ids": [shape]}));
    shape
}

#[test]
fn a_process_shows_typed_editors_from_its_schema() {
    let mut h = harness();
    flowchart(&mut h);
    for label in ["Identity", "Relationships (1)", "Views", "Shape", "Document properties", "Owner"] {
        h.get_by_label(label);
    }
    assert_eq!(h.query_all_by_label("Properties").count(), 2, "the tab and the section");
    // The bool property is a checkbox; ticking it runs element.setProperty.
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Automated").click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc.model.element("heat").and_then(|e| e.prop("automated")).and_then(|v| v.as_bool()), Some(true));
    // A required property is marked.
    let shape = run(&mut h, "element.get", json!({"id": "rinse"}))["views"][0]["items"][0].as_str().unwrap_or_default().to_string();
    run(&mut h, "edit.select", json!({"ids": [shape]}));
    h.get_by_label("Reference *");
    // The relationship names the other element.
    run(&mut h, "edit.select", json!({"ids": []}));
    h.get_by_label("Select a shape to inspect it.");
}

#[test]
fn typing_a_taken_tag_flags_the_field_and_a_free_one_commits() {
    let mut h = harness();
    let _ = flowchart(&mut h);
    let shape = run(&mut h, "element.get", json!({"id": "rinse"}))["views"][0]["items"][0].as_str().unwrap_or_default().to_string();
    run(&mut h, "edit.select", json!({"ids": [shape]}));
    assert!(h.query_by_label_contains("In use by").is_none());
    h.get_all_by_value("S").next().expect("the tag field").focus();
    h.run_steps(1);
    h.get_all_by_value("S").next().expect("the tag field").type_text("1");
    h.run_steps(3);
    h.get_by_label("In use by Mix");
    assert_eq!(h.state().session.doc.model.element("rinse").map(|e| e.tag.as_str()), Some("S"), "a taken tag is not stored");
    // A free tag is stored on Enter.
    h.key_press(egui::Key::Backspace);
    h.run_steps(2);
    h.get_all_by_value("S").next().expect("the tag field").type_text("9");
    h.run_steps(2);
    assert!(h.query_by_label_contains("In use by").is_none());
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().session.doc.model.element("rinse").map(|e| e.tag.as_str()), Some("S9"));
}

#[test]
fn expert_mode_shows_ids_and_standard_hides_them() {
    let mut h = harness();
    flowchart(&mut h);
    assert!(h.query_by_label("heat").is_none(), "Standard hides the id");
    assert!(h.query_by_label("Tag format").is_none());
    let r = run(&mut h, "ui.inspector", json!({"mode": "expert"}));
    assert_eq!(r["mode"], "expert");
    h.get_by_label("heat");
    h.get_by_label("Tag format");
    assert_eq!(vizcraft_ui_egui::control::inspect(h.state(), &h.ctx)["inspector"]["mode"], "expert");
    run(&mut h, "ui.inspector", json!({"mode": "standard"}));
    assert!(h.query_by_label("heat").is_none());
}

#[test]
fn the_format_tab_runs_the_style_commands() {
    let mut h = harness();
    let shape = flowchart(&mut h);
    run(&mut h, "ui.inspector", json!({"tab": "format"}));
    h.get_by_label("Fill and line");
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Bold").click();
    h.run_steps(3);
    assert!(h.state().session.doc.shape(&shape).is_some_and(|s| s.style.bold));
    run(&mut h, "ui.inspector", json!({"tab": "properties"}));
    h.get_by_label("Identity");
}

#[test]
fn the_properties_pane_command_opens_the_document_section() {
    let mut h = harness();
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": false}));
    run(&mut h, "ui.pane.properties", json!({}));
    assert!(h.state().ui.panes.is_open("shapeData"));
    h.get_by_label("Document title");
}
