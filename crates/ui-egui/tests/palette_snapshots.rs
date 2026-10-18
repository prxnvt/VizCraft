//! Pixel snapshots at 1280 points wide: the scoped palette on a Flowchart page, the palette on a
//! page with no view, the Flowchart (notation) tab and the Model tab. Same harness and rules as
//! `ribbon_snapshots.rs` (needs a wgpu adapter, skipped without one or with
//! `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

/// Whether wgpu can render here (see `ribbon_snapshots.rs`).
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

fn window(tab: &str, height: f32, flowchart: bool) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::Document::new()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app.ui.tab = tab.into();
    if flowchart {
        app.run("model.setViewpoint", json!({"viewpoint": "flowchart.Flowchart"})).unwrap();
    }
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, height)).with_pixels_per_point(1.0).wgpu().build_ui_state(
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

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

#[test]
fn palette_and_notation_snapshots() {
    if !wgpu_available() {
        eprintln!("palette snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // The palette on a Flowchart page: Steps, Data, Links, Connectors (Flow in use), Drawing.
    let mut h = window("Home", 900.0, true);
    h.state_mut().run("ui.pane", json!({"name": "navigation", "value": false})).unwrap();
    h.state_mut().run("ui.tool", json!({"tool": "connector", "kind": "flowchart.flow"})).unwrap();
    snap(&mut results, &mut h, "palette-flowchart");

    // A page with no view: each pack closed under its name, Drawing open.
    let mut h = window("Home", 900.0, false);
    h.state_mut().run("ui.pane", json!({"name": "navigation", "value": false})).unwrap();
    snap(&mut results, &mut h, "palette-no-view");

    // The notation tab, named after the pack.
    let mut h = window("Flowchart", 230.0, true);
    snap(&mut results, &mut h, "tab-notation");

    // The Model tab.
    let mut h = window("Model", 230.0, true);
    snap(&mut results, &mut h, "tab-model");
}
