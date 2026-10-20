//! Pixel snapshots of the process pack's front end: the Process Plant tab, the Inspector's Line
//! group on a pipe, the Tables dock's line list and the Tag Formats dialog, on the P&ID example,
//! with the same rig and rules as `tables_snapshots.rs` (needs a wgpu adapter, skipped without
//! one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

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

/// The P&ID example open on its first sheet.
fn app() -> CraftApp {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    s.run("example.open", &json!({"example": "process.pidExample"})).unwrap();
    let mut app = CraftApp::new(s, Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app
}

#[test]
fn the_process_plant_tab_inspector_tables_and_tag_formats() {
    if !wgpu_available() {
        eprintln!("process snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
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
    let opts = SnapshotOptions::new().output_path(output_dir());
    h.state_mut().run("ui.tab", json!({"tab": "Process Plant"})).unwrap();
    h.run_steps(4);
    h.snapshot_options("process-tab", &opts);
    // The Inspector on the first line's pipe.
    let pipe = h.state().session.doc.page().unwrap().connectors.iter().find(|c| c.ext.contains_key("process.line")).map(|c| c.id.clone()).unwrap();
    h.state_mut().run("edit.select", json!({"ids": [pipe]})).unwrap();
    h.run_steps(4);
    h.snapshot_options("process-inspector-line", &opts);
    h.state_mut().run("edit.selectNone", json!({})).unwrap();
    h.state_mut().run("ui.tables", json!({"pack": "process.line"})).unwrap();
    h.run_steps(4);
    h.snapshot_options("process-tables-line-list", &opts);
    h.state_mut().run("identity.setFormat", json!({})).unwrap();
    h.run_steps(4);
    h.snapshot_options("process-tag-formats", &opts);
}
