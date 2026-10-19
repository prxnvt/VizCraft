//! Pixel snapshots of the Patterns pane with a selected instance and of the pattern form, with
//! the same rig and rules as `tables_snapshots.rs` (needs a wgpu adapter, skipped without one or
//! with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions};
use serde_json::json;
use vizcraft_engine::Session;
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

/// A blank document with a parallel split of four and a cancel region inserted, the split
/// selected and the Patterns pane open.
fn app() -> CraftApp {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    let r = s.run("pattern.insert", &json!({"pattern": "workflow.parallelSplit", "args": {"branches": 4}, "x": 60, "y": 60})).unwrap();
    s.run("pattern.insert", &json!({"pattern": "workflow.cancelRegion", "x": 60, "y": 60})).unwrap();
    s.run("edit.select", &json!({"ids": [r["group"]]})).unwrap();
    let mut app = CraftApp::new(s, Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app
}

#[test]
fn the_patterns_pane_and_the_form() {
    if !wgpu_available() {
        eprintln!("patterns snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
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
    h.state_mut().run("ui.tab", json!({"tab": "Insert"})).unwrap();
    h.state_mut().run("ui.pane", json!({"name": "shapes", "value": false})).unwrap();
    h.state_mut().run("ui.pane.patterns", json!({"value": true})).unwrap();
    h.state_mut().run("ui.collapsePanes", json!({"side": "right", "value": true})).unwrap();
    h.run_steps(4);
    let opts = SnapshotOptions::new().output_path(output_dir());
    h.snapshot_options("patterns-pane", &opts);
    h.state_mut().run("ui.patterns", json!({"pattern": "workflow.exclusiveChoice"})).unwrap();
    h.run_steps(4);
    h.snapshot_options("patterns-form", &opts);
}
