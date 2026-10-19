//! The Tables tab through the app: the list and the matrix read from the model, a cell edit
//! reaching it, a click toggling a relationship, `ui.tables` and `ui.inspect`, and Export writing
//! the picked path, under egui_kittest.

use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_engine::Session;
use vizcraft_ui_egui::dock::DockTab;
use vizcraft_ui_egui::{CraftApp, Services, control};

/// The sample flowchart as model elements and flows on one page.
fn session() -> Session {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    let nodes = [
        ("start", "flowchart.Terminator", "Start"),
        ("gather", "flowchart.Process", "Gather input"),
        ("valid", "flowchart.Decision", "Valid?"),
        ("save", "flowchart.Process", "Save diagram"),
        ("end", "flowchart.Terminator", "End"),
        ("fix", "flowchart.Process", "Fix input"),
    ];
    for (i, (id, ty, name)) in nodes.iter().enumerate() {
        s.run("element.create", &json!({"id": id, "type": ty, "name": name, "x": (i % 3) as f64 * 200.0, "y": (i / 3) as f64 * 150.0})).unwrap();
    }
    for (i, (a, b)) in
        [("start", "gather"), ("gather", "valid"), ("valid", "save"), ("valid", "fix"), ("fix", "gather"), ("save", "end")].iter().enumerate()
    {
        s.run("relationship.create", &json!({"id": format!("f{i}"), "from": a, "to": b})).unwrap();
    }
    s
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
    control::inspect(h.state(), &ctx)["tables"].clone()
}

#[test]
fn ui_tables_shows_a_list_and_a_matrix_and_inspect_reports_them() {
    let mut h = harness(CraftApp::new(session(), Services::default()));
    let r = h.state_mut().run("ui.tables", json!({"type": "flowchart.Process"})).unwrap();
    assert_eq!((r["view"].as_str(), r["count"].as_u64()), (Some("list"), Some(3)));
    h.run_steps(3);
    assert_eq!(h.state().ui.dock.tab, DockTab::Tables);
    assert!(h.state().ui.dock.open);
    h.get_by_label("Gather input");
    h.get_by_label("Save diagram");
    h.get_by_label("Fix input");
    let r = h
        .state_mut()
        .run("ui.tables", json!({"view": "matrix", "rows": "flowchart.Process", "cols": "flowchart.Process", "relationship": "flowchart.Flow"}))
        .unwrap();
    assert_eq!((r["view"].as_str(), r["count"].as_u64()), (Some("matrix"), Some(1)));
    h.run_steps(3);
    let d = inspect(&mut h);
    assert_eq!((d["rows"].as_str(), d["cols"].as_str(), d["count"].as_u64()), (Some("flowchart.Process"), Some("flowchart.Process"), Some(1)));
    assert!(h.state_mut().run("ui.tables", json!({"view": "list", "type": "nope.Thing"})).is_err());
    assert!(h.state_mut().run("ui.tables", json!({"view": "cube"})).is_err());
    assert_eq!(inspect(&mut h)["view"], "matrix", "a refused view changes nothing");
}

#[test]
fn a_cell_edit_reaches_the_model() {
    let mut h = harness(CraftApp::new(session(), Services::default()));
    h.state_mut().run("ui.tables", json!({"type": "flowchart.Process", "columns": ["name"]})).unwrap();
    h.run_steps(3);
    let cell = h.get_all_by_label("Gather input").last().expect("the cell");
    let p = cell.rect().center();
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    }
    h.run_steps(3);
    let field = h.get_all_by_value("Gather input").next().expect("the text field");
    field.focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_all_by_value("Gather input").next().expect("the text field").type_text("Collect input");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().session.doc.model.element("gather").unwrap().name, "Collect input");
    h.get_all_by_label("Collect input").next().expect("the list shows the new name");
}

#[test]
fn a_matrix_click_toggles_a_relationship() {
    let mut h = harness(CraftApp::new(session(), Services::default()));
    h.state_mut()
        .run("ui.tables", json!({"view": "matrix", "rows": "flowchart.Process", "cols": "flowchart.Process", "relationship": "flowchart.Flow"}))
        .unwrap();
    h.run_steps(3);
    let before = h.state().session.doc.model.relationships.len();
    let cell = h.query_all_by_label_contains("Gather input to Save diagram").next().expect("the matrix cell");
    cell.click();
    h.run_steps(4);
    assert_eq!(h.state().session.doc.model.relationships.len(), before + 1);
    assert_eq!(inspect(&mut h)["count"], 2);
}

#[test]
fn export_writes_the_picked_path() {
    let dir = std::env::temp_dir().join(format!("vizcraft-tables-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("processes.csv");
    let asked = Arc::new(Mutex::new(String::new()));
    let seen = asked.clone();
    let target = out.to_string_lossy().to_string();
    let services = Services {
        pick_save: Some(Box::new(move |name| {
            *seen.lock().unwrap() = name.to_string();
            Some(target.clone())
        })),
        ..Services::default()
    };
    let mut h = harness(CraftApp::new(session(), services));
    h.state_mut().run("ui.tables", json!({"type": "flowchart.Process"})).unwrap();
    h.run_steps(3);
    h.get_by_label("Export").click();
    h.run_steps(3);
    h.get_by_label("CSV").click();
    h.run_steps(4);
    assert!(asked.lock().unwrap().ends_with(".csv"), "{}", asked.lock().unwrap());
    let csv = std::fs::read_to_string(&out).unwrap();
    assert!(csv.starts_with("id,tag,name,automated,owner\n"), "{csv}");
    assert_eq!(csv.lines().count(), 4, "a header and one row per Process");
    assert!(csv.contains("gather,S1,Gather input,false,"), "{csv}");
    let _ = std::fs::remove_dir_all(&dir);
}
