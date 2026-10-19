//! The Generate dock tab and ribbon tab under egui_kittest: Preview fills the file list and the
//! diff, Generate writes, Check reports, `ui.generate` sets the fields and `ui.inspect` reads them.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_engine::{Origin, Session};
use vizcraft_ui_egui::{CraftApp, Services, control};

/// The sample with three typed elements and a flow beside it (the sample's own shapes are plain
/// drawing, so they have no model).
fn app() -> CraftApp {
    let mut s = Session::new(vizcraft_engine::sample::sample_document());
    for (id, ty, name, x) in [
        ("gin", "flowchart.Process", "Gather input", 40),
        ("ok", "flowchart.Decision", "Valid?", 240),
        ("save", "flowchart.Process", "Save diagram", 440),
    ] {
        s.run("element.create", &json!({"type": ty, "name": name, "id": id, "x": x, "y": 700})).unwrap();
    }
    s.run("relationship.create", &json!({"from": "gin", "to": "ok", "id": "f1"})).unwrap();
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

fn inspect(h: &Harness<'static, CraftApp>) -> Value {
    control::inspect(h.state(), &egui::Context::default())
}

fn out_dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("vizcraft-ui-generate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn preview_fills_the_dock_and_generate_writes() {
    let out = out_dir("preview");
    let mut h = harness(app());
    let r = h.state_mut().run("ui.generate", json!({"target": "markdown-catalog", "output": out.to_string_lossy()})).unwrap();
    assert_eq!(r["target"], "markdown-catalog");
    h.run_steps(3);
    let i = inspect(&h);
    assert_eq!((i["dock"]["open"].as_bool(), i["dock"]["tab"].as_str()), (Some(true), Some("generate")));
    assert_eq!(i["generate"]["files"].as_array().map(Vec::len), Some(0));

    h.get_by_label("Preview").click();
    h.run_steps(3);
    let i = inspect(&h);
    assert_eq!(i["generate"]["files"], json!([{"path": "catalog.md", "action": "create"}]), "{}", i["generate"]);
    assert_eq!(i["dock"]["rows"], 1);
    assert!(h.query_by_label_contains("+| s1 | S1 | Gather input |").is_some(), "the diff shows the new rows");
    assert!(!out.join("catalog.md").exists(), "preview writes nothing");

    // "Generate" also names the ribbon tab and the dock tab; the button comes last.
    h.get_all_by_label("Generate").last().unwrap().click();
    h.run_steps(3);
    assert!(out.join("catalog.md").is_file());
    let i = inspect(&h);
    assert_eq!(i["generate"]["files"], json!([{"path": "catalog.md", "action": "unchanged"}]));
    assert!(i["generate"]["log"].as_str().unwrap().starts_with("Generate: wrote 1"));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn the_ribbon_tab_runs_the_commands_with_the_dock_fields() {
    let out = out_dir("ribbon");
    let mut a = app();
    a.ui.tab = "Generate".into();
    let mut h = harness(a);
    h.state_mut().run("ui.generate", json!({"target": "markdown-catalog", "output": out.to_string_lossy()})).unwrap();
    h.state_mut().run("ui.dock", json!({"open": false})).unwrap();
    h.run_steps(3);
    h.get_by_label("Check Output").click();
    h.run_steps(3);
    let i = inspect(&h);
    assert_eq!((i["dock"]["open"].as_bool(), i["dock"]["tab"].as_str()), (Some(true), Some("generate")));
    assert!(i["generate"]["log"].as_str().unwrap().contains("missing catalog.md"), "{i}");
    assert!(h.query_by_label("Markdown Catalogue").is_some(), "the Targets gallery lists the target");
    // Agents bring their own params and get the engine's answer untouched.
    let r = h.state_mut().run_as(Origin::Control, "generate.check", json!({})).unwrap_err();
    assert!(r.contains("target"), "{r}");
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn ui_generate_refuses_junk_and_runs_on_request() {
    let out = out_dir("junk");
    let mut h = harness(app());
    for bad in [json!({"target": "nope"}), json!({"run": "explode"}), json!({"spec": 5}), json!({"run": 3})] {
        assert!(h.state_mut().run("ui.generate", bad.clone()).is_err(), "{bad}");
    }
    let r = h.state_mut().run("ui.generate", json!({"target": "markdown-catalog", "output": out.to_string_lossy(), "run": "check"})).unwrap();
    assert_eq!(r["result"]["ok"], false);
    let r = h.state_mut().run("ui.generate", json!({"spec": "/no/such/spec.toml", "run": "preview"})).unwrap();
    assert!(r["result"]["error"].is_string(), "{r}");
    assert_eq!(r["spec"], "/no/such/spec.toml");
    h.run_steps(3);
    let _ = std::fs::remove_dir_all(&out);
}
