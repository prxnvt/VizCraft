//! Pixel snapshots of the electrical pack in the app: the Electrical notation tab, a ladder page
//! (the ladder example), the Inspector's Electrical group on a terminal strip, and the Tables
//! dock's wire list of the control circuit example. Same harness and rules as `snapshots.rs`
//! (needs a wgpu adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with
//! `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_diagram::Document;
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

fn window(doc: Document) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(doc), Services::default());
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

fn run(h: &mut Harness<'static, CraftApp>, id: &str, params: serde_json::Value) -> serde_json::Value {
    let r = h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
    r
}

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

#[test]
fn electrical_snapshots() {
    if !wgpu_available() {
        eprintln!("electrical snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();
    let mut h = window(Document::new());
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "example.open", json!({"example": "electrical.ladderSample"}));
    run(&mut h, "ui.tab", json!({"tab": "Electrical"}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "electrical-ladder-page");
    run(&mut h, "example.open", json!({"example": "electrical.controlCircuitSample"}));
    run(&mut h, "ui.tab", json!({"tab": "Electrical"}));
    let r = run(&mut h, "terminal.strip", json!({"x": 900, "y": 120, "count": 4}));
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": true}));
    run(&mut h, "edit.select", json!({"ids": [r["shape"]]}));
    snap(&mut results, &mut h, "electrical-inspector-strip");
    run(&mut h, "edit.selectNone", json!({}));
    run(&mut h, "ui.tables", json!({"subject": "connections", "kind": "wire"}));
    snap(&mut results, &mut h, "electrical-wire-list");
}
