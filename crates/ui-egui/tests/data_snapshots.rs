//! Pixel snapshots of shape data: the Data tab's ribbon, and the Shape Data pane on a shape with
//! typed rows (a number with a unit, a checkbox, a date, an enum, a URL and an empty required
//! key) plus page keys. Same harness and rules as `snapshots.rs` (needs a wgpu adapter, skipped
//! without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

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
fn shape_data_snapshots() {
    if !wgpu_available() {
        eprintln!("shape data snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // The Data tab's ribbon.
    let mut h = window();
    run(&mut h, "ui.tab", json!({"tab": "Data"}));
    snap(&mut results, &mut h, "ribbon-data");

    // The Shape Data pane on one shape with a row of each type and two page keys.
    let mut h = window();
    let id = h.state().session.doc.page().unwrap().shapes[1].id.clone();
    run(&mut h, "data.defineKey", json!({"key": "cost", "label": "Cost", "type": "number", "unit": "USD", "ids": [id]}));
    run(
        &mut h,
        "data.defineKey",
        json!({"key": "status", "label": "Status", "type": "enum", "values": ["Open", "In progress", "Done"], "default": "Open"}),
    );
    run(&mut h, "data.set", json!({"ids": [id], "key": "cost", "value": 1250}));
    run(&mut h, "data.set", json!({"ids": [id], "key": "approved", "label": "Approved", "value": true}));
    run(&mut h, "data.set", json!({"ids": [id], "key": "due", "label": "Due date", "type": "date", "value": "2026-11-30"}));
    run(&mut h, "data.set", json!({"ids": [id], "key": "spec", "label": "Spec", "type": "url", "value": "https://example.com/spec"}));
    run(&mut h, "data.set", json!({"ids": [id], "key": "owner", "label": "Owner", "value": "", "required": true}));
    run(&mut h, "edit.select", json!({"ids": [id]}));
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "properties", "value": false}));
    run(&mut h, "ui.tab", json!({"tab": "Data"}));
    run(&mut h, "view.shapeDataPane", json!({"value": true}));
    snap(&mut results, &mut h, "pane-shape-data");

    // The Define Shape Data dialog.
    let mut h = window();
    run(&mut h, "data.defineDialog", json!({}));
    snap(&mut results, &mut h, "dialog-define-shape-data");
}
