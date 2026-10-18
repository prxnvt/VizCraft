//! Pixel snapshots of the ribbon's open dropdowns, each showing its current choice with a check
//! mark at the right: Orientation, Size and Connector Style, the Themes gallery with a theme
//! chosen, and the zoom field's list. Same harness and rules as `snapshots.rs` (needs a wgpu
//! adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with
//! `UPDATE_SNAPSHOTS=1`).

use egui_kittest::kittest::Queryable;
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

/// The window on `tab`, 1280 wide and just tall enough for the ribbon and an open dropdown.
fn window(tab: &str) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app.ui.tab = tab.into();
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 440.0)).with_pixels_per_point(1.0).wgpu().build_ui_state(
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
fn menu_snapshots() {
    if !wgpu_available() {
        eprintln!("menu snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // Orientation: the sample page is landscape.
    let mut h = window("Design");
    h.get_by_label("Orientation").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "menu-orientation");

    // Size: A4 in portrait, so the preset that matches the page is marked.
    let mut h = window("Design");
    run(&mut h, "page.setup", json!({"width": 826.8, "height": 1169.3}));
    h.get_by_label("Size").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "menu-size");

    // Connector Style: every connector on the page curved.
    let mut h = window("Design");
    run(&mut h, "design.setConnectorStyle", json!({"style": "curved"}));
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Connector Style").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "menu-connector-style");

    // The Themes gallery with a theme and variant chosen: that cell carries the mark.
    let mut h = window("Design");
    run(&mut h, "design.setTheme", json!({"theme": "orchard", "variant": 1}));
    snap(&mut results, &mut h, "gallery-themes");

    // The zoom field's list at 100%.
    let mut h = window("View");
    h.get_by_label("Zoom Level options").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "combo-zoom");
}
