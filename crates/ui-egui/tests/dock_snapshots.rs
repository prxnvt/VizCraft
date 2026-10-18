//! Pixel snapshots of each dock tab, with the same rig and rules as `routing_snapshot.rs` (needs a
//! wgpu adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with
//! `UPDATE_SNAPSHOTS=1`).

use egui_kittest::kittest::Queryable;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions};
use serde_json::json;
use vizcraft_engine::{Origin, Session};
use vizcraft_ui_egui::{CraftApp, Services};

fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

/// Whether wgpu can render here (see `snapshots.rs`).
fn wgpu_available() -> bool {
    if std::env::var_os("VIZCRAFT_SKIP_SNAPSHOTS").is_some() {
        return false;
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let probe = std::panic::catch_unwind(WgpuTestRenderer::new);
    std::panic::set_hook(hook);
    probe.is_ok() && (std::path::Path::new(&output_dir()).is_dir() || std::env::var_os("UPDATE_SNAPSHOTS").is_some())
}

/// A hand-routed page with a connector across a shape and a loose end, held to the drawing rules.
fn app() -> CraftApp {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    s.run("page.setRouting", &json!({"avoid": "never"})).unwrap();
    for (id, x) in [("a", 100), ("b", 300), ("c", 500)] {
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": x, "y": 100, "text": id.to_uppercase(), "id": id})).unwrap();
    }
    s.run("connector.add", &json!({"from": {"shape": "a", "point": 1}, "to": {"shape": "c", "point": 3}, "route": "straight", "id": "ac"})).unwrap();
    s.run("connector.add", &json!({"from": {"shape": "c", "point": 2}, "to": {"x": 560, "y": 260}, "id": "loose"})).unwrap();
    s.run("validate.setProfile", &json!({"rulesets": ["drawing.tidy"]})).unwrap();
    let mut app = CraftApp::new(s, Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app
}

#[test]
fn each_dock_tab() {
    if !wgpu_available() {
        eprintln!("dock snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_pixels_per_point(1.0).wgpu().build_ui_state(
        |ui, app: &mut CraftApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app(),
    );
    h.input_mut().max_texture_side = Some(8192);
    h.run_steps(6);
    h.state_mut().run("ui.collapsePanes", json!({"side": "left", "value": true})).unwrap();
    h.state_mut().run("ui.collapsePanes", json!({"side": "right", "value": true})).unwrap();
    h.state_mut().run("ui.dock", json!({"tab": "problems"})).unwrap();
    h.run_steps(4);
    h.get_by_label("Check").click();
    h.run_steps(4);
    let opts = SnapshotOptions::new().output_path(output_dir());
    h.snapshot_options("dock-problems", &opts);

    h.state_mut().run("ui.search", json!({"query": "B", "scope": "document"})).unwrap();
    h.run_steps(4);
    h.snapshot_options("dock-search", &opts);

    let batch = json!({"label": "Agent edit", "commands": [
        {"command": "shape.move", "params": {"ids": ["a"], "dx": 40}},
        {"command": "shape.move", "params": {"ids": ["b"], "dx": 40}},
    ]});
    h.state_mut().run_as(Origin::Control, "edit.batch", batch).unwrap();
    h.state_mut().run("ui.dock", json!({"tab": "activity"})).unwrap();
    h.run_steps(4);
    h.snapshot_options("dock-activity", &opts);

    h.state_mut().run("ui.dock", json!({"tab": "tables"})).unwrap();
    h.run_steps(3);
    h.snapshot_options("dock-tables", &opts);
    h.state_mut().run("ui.dock", json!({"tab": "generate"})).unwrap();
    h.run_steps(3);
    h.snapshot_options("dock-generate", &opts);
}
