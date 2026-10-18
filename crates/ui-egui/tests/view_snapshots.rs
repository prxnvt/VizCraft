//! Pixel snapshots for the View tab extras: the tab at 1280 points wide (Page Width, Task Panes,
//! Views, Window and Macros), the Add-Ons menu open, the Pan and Zoom pane on a zoomed canvas and
//! presentation mode. Same harness and rules as `ribbon_snapshots.rs` (needs a wgpu adapter, skipped
//! without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::kittest::Queryable;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::ribbon::Overflow;
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

fn window(tab: &str, height: f32) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app.ui.tab = tab.into();
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

fn run(h: &mut Harness<'static, CraftApp>, id: &str, params: serde_json::Value) {
    h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
}

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

#[test]
fn view_extras_snapshots() {
    if !wgpu_available() {
        eprintln!("view snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // The View tab at 1280, nothing overflowing.
    let mut h = window("View", 230.0);
    assert!(!Overflow::get(&h.ctx).overflows());
    snap(&mut results, &mut h, "tab-view");

    // The Add-Ons menu open under the Macros group.
    let mut h = window("View", 330.0);
    h.get_by_label("Add-Ons").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "menu-addons");

    // The Pan and Zoom pane beside a zoomed canvas.
    let mut h = window("Home", 800.0);
    h.state_mut().ui.panes.close("history");
    run(&mut h, "ui.pane.panZoom", json!({"value": true}));
    run(&mut h, "view.zoom", json!({"value": 200}));
    run(&mut h, "view.pan", json!({"x": 330, "y": 260}));
    snap(&mut results, &mut h, "pane-pan-zoom");

    // Presentation mode: the drawing and nothing else.
    let mut h = window("Home", 800.0);
    run(&mut h, "ui.present", json!({"on": true}));
    h.run_steps(4);
    snap(&mut results, &mut h, "presentation");
}
