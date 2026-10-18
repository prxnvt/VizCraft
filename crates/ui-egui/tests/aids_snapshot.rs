//! A pixel snapshot of the canvas with the grid, rulers, a guide and page breaks showing, and a
//! selected shape marked on the rulers. Same rig and rules as `tests/snapshots.rs`: it needs a
//! wgpu adapter, passes with a note without one (or with `VIZCRAFT_SKIP_SNAPSHOTS` set), and
//! `UPDATE_SNAPSHOTS=1` rewrites the reference in `tests/snapshots/<os>/`.

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

/// Whether wgpu can render here (see `tests/snapshots.rs`; this is the only test in the binary).
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
fn grid_rulers_and_guides_snapshot() {
    if !wgpu_available() {
        eprintln!("aids snapshot: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
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
    for (id, params) in [
        ("ui.tab", json!({"tab": "View"})),
        ("guide.add", json!({"orientation": "vertical", "position": 550})),
        ("guide.add", json!({"orientation": "horizontal", "position": 200})),
        ("view.pageBreaks", json!({"value": true})),
        ("view.connectionPoints", json!({"value": true})),
        ("edit.select", json!({"ids": ["s7"]})),
    ] {
        h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    h.run_steps(5);
    h.snapshot_options("visual-aids", &SnapshotOptions::new().output_path(output_dir()));
}
