//! Pixel snapshot of the Model Tree pane in light mode. Same harness and rules as `snapshots.rs`
//! (needs a wgpu adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with
//! `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services, panes};

fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

/// Whether wgpu can render here (see `snapshots.rs`; this is the only test in this binary).
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

#[test]
fn model_tree_snapshot() {
    if !wgpu_available() {
        eprintln!("model tree snapshot: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut a = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
    let mut run = |id: &str, p: serde_json::Value| a.run(id, p).unwrap_or_else(|e| panic!("{id}: {e}"));
    run("element.create", json!({"type": "flowchart.Terminator", "name": "Start", "id": "start", "x": 0, "y": 0}));
    let made = run("element.create", json!({"type": "flowchart.Process", "name": "Collect order", "id": "collect", "x": 0, "y": 150}));
    run("element.create", json!({"type": "flowchart.Process", "name": "Review", "id": "review", "x": 0, "y": 300}));
    run("element.create", json!({"type": "flowchart.Decision", "name": "Approved?", "id": "ok", "x": 0, "y": 450}));
    run("relationship.create", json!({"from": "start", "to": "collect", "id": "f1"}));
    run("relationship.create", json!({"from": "collect", "to": "review", "id": "f2"}));
    run("edit.select", json!({"ids": [made["shape"]]}));
    let mut h = Harness::builder().with_size(egui::vec2(300.0, 420.0)).with_pixels_per_point(1.0).wgpu().build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            let spec = panes::spec("navigation").expect("the pane exists");
            (spec.show)(app, ui);
        },
        a,
    );
    h.run_steps(6);
    let mut results = SnapshotResults::new();
    results.add(h.try_snapshot_options("pane-model-tree", &SnapshotOptions::new().output_path(output_dir())));
}
