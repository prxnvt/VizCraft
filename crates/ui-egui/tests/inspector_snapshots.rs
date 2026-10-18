//! Pixel snapshots of the Inspector on a flowchart Process in Standard and Expert mode, and its
//! Format tab. Same harness and rules as `snapshots.rs` (needs a wgpu adapter, skipped without
//! one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

/// Whether wgpu can render here (see `snapshots.rs`; this is the only test in this binary too).
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

fn window() -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_pixels_per_point(1.0).wgpu().build_ui_state(
        |ui, app: &mut CraftApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.input_mut().max_texture_side = Some(8192);
    h.run_steps(6);
    h
}

fn run(h: &mut Harness<'static, CraftApp>, id: &str, params: serde_json::Value) {
    h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
}

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

#[test]
fn inspector_snapshots() {
    if !wgpu_available() {
        eprintln!("inspector snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();
    let mut h = window();
    run(&mut h, "file.new", json!({}));
    run(
        &mut h,
        "element.create",
        json!({"type": "flowchart.Process", "name": "Mix", "tag": "S1", "id": "mix", "x": 80, "y": 80, "properties": {"owner": "Plant A"}}),
    );
    run(&mut h, "element.create", json!({"type": "flowchart.Process", "name": "Heat", "tag": "S2", "id": "heat", "x": 80, "y": 230}));
    run(&mut h, "relationship.create", json!({"from": "mix", "to": "heat", "id": "f1"}));
    run(&mut h, "page.add", json!({"name": "Detail"}));
    run(&mut h, "element.show", json!({"id": "mix", "page": "Detail", "x": 40, "y": 40}));
    run(&mut h, "page.setActive", json!({"page": 0}));
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": true}));
    let shape = h.state().session.doc.page().and_then(|p| p.shapes.iter().find(|s| s.element.as_deref() == Some("mix")).map(|s| s.id.clone()));
    run(&mut h, "edit.select", json!({"ids": [shape.unwrap_or_default()]}));
    snap(&mut results, &mut h, "inspector-standard");
    run(&mut h, "ui.inspector", json!({"mode": "expert"}));
    snap(&mut results, &mut h, "inspector-expert");
    run(&mut h, "ui.inspector", json!({"mode": "standard", "tab": "format"}));
    snap(&mut results, &mut h, "inspector-format");
}
