//! The Patterns pane and the pattern form through the app, under egui_kittest: `ui.patterns`
//! opens the form, its Insert button inserts the pattern, the pane lists the patterns and shows
//! the selected instance's arguments with an Apply button that regenerates it.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::dialogs::Dialog;
use vizcraft_ui_egui::{CraftApp, Services, control};

fn app() -> CraftApp {
    let mut a = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
    a.ui.in_window_menus = true;
    a.integrated_titlebar = false;
    a
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

/// Click the last button labelled `label` (dialogs and panes draw after the ribbon).
fn click(h: &mut Harness<'_, CraftApp>, label: &str) {
    let node = h.query_all_by_role_and_label(egui::accesskit::Role::Button, label).last().unwrap_or_else(|| panic!("no {label} button"));
    node.click();
    h.run_steps(4);
}

#[test]
fn the_form_inserts_the_pattern_with_its_values() {
    let mut a = app();
    let r = a.run("ui.patterns", json!({"pattern": "workflow.parallelSplit"})).unwrap();
    assert_eq!(r["open"], false, "opening the form does not open the pane");
    assert_eq!(a.dialog.as_ref().map(|d| d.name()), Some("pattern"));
    if let Some(Dialog::Pattern(s)) = &mut a.dialog {
        s.values.insert("branches".into(), json!(4));
    }
    let mut h = harness(a);
    assert!(h.query_by_label("Branches").is_some(), "the form shows the count field");
    click(&mut h, "Insert");
    let app = h.state();
    assert!(app.dialog.is_none(), "Insert closes the form");
    assert_eq!(app.session.doc.model.elements.len(), 6, "a split, four branches and a join");
    assert_eq!(app.session.doc.model.patterns.len(), 1);
    assert_eq!(app.session.undo_label(), Some("Insert Pattern"));
}

#[test]
fn a_refused_form_stays_open_with_the_reason() {
    let mut a = app();
    a.run("ui.patterns", json!({"pattern": "workflow.nOfMJoin"})).unwrap();
    if let Some(Dialog::Pattern(s)) = &mut a.dialog {
        s.values.insert("required".into(), json!(9));
    }
    let mut h = harness(a);
    click(&mut h, "Insert");
    let Some(Dialog::Pattern(s)) = &h.state().dialog else { panic!("the form closed") };
    assert!(s.error.contains("more than"), "{}", s.error);
    assert!(h.state().session.doc.model.elements.is_empty());
}

#[test]
fn the_pane_lists_patterns_and_shows_the_selected_instance_args() {
    let mut a = app();
    let r = a.run("pattern.insert", json!({"pattern": "workflow.parallelSplit", "args": {"branches": 4}})).unwrap();
    let group = r["group"].as_str().unwrap().to_string();
    a.run("ui.pane.patterns", json!({"value": true})).unwrap();
    a.run("edit.select", json!({"ids": [group]})).unwrap();
    let mut h = harness(a);
    assert!(h.query_by_label("Exclusive Choice").is_some(), "the pane lists the starter patterns");
    assert!(h.query_by_label("Selected: Parallel Split").is_some(), "the selected instance's form is shown");
    let inspect = control::inspect(h.state(), &h.ctx);
    assert_eq!(inspect["patterns"]["args"]["branches"], 4);
    assert!(inspect["patterns"]["instance"].is_string());
    let form = h.state().patterns.form.clone().unwrap();
    assert_eq!(form.values["branches"], 4, "the form starts from the instance's args");
    h.state_mut().patterns.form.as_mut().unwrap().values.insert("branches".into(), json!(5));
    click(&mut h, "Apply");
    assert_eq!(h.state().session.doc.model.elements.len(), 7, "Apply regenerated the instance with five branches");
    assert_eq!(h.state().session.doc.model.patterns[0].args["branches"], 5);

    // The search filters the list.
    h.state_mut().run("ui.patterns", json!({"query": "cancel"})).unwrap();
    h.run_steps(4);
    assert!(h.query_by_label("Cancel Region").is_some());
    assert!(h.query_by_label("Exclusive Choice").is_none());
}

#[test]
fn ui_patterns_refuses_junk() {
    let mut a = app();
    assert!(a.run("ui.patterns", json!({"pattern": "nope"})).is_err());
    assert!(a.run("ui.patterns", json!({"pattern": 5})).is_err());
    assert!(a.run("ui.patterns", json!({"query": []})).is_err());
    let r = a.run("ui.patterns", json!({"query": "x".repeat(10_000)})).unwrap();
    assert_eq!(r["query"].as_str().unwrap().chars().count(), 256);
    assert_eq!(r["open"], true, "a query opens the pane");
}

#[test]
fn the_palette_finds_and_inserts_a_pattern() {
    let a = app();
    let hits = vizcraft_ui_egui::command_palette::hits(&a, "parallel split");
    let hit = hits.iter().find(|h| h.kind == vizcraft_engine::suggest::HitKind::Pattern).expect("a pattern hit").clone();
    let mut h = harness(a);
    let ctx = h.ctx.clone();
    vizcraft_ui_egui::command_palette::act(h.state_mut(), &ctx, &hit);
    assert_eq!(h.state().session.doc.model.patterns.len(), 1, "acting on the hit inserts the pattern with its defaults");
}
