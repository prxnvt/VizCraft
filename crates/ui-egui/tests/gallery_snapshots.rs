//! Pixel snapshots of the template gallery and the engineering Home tab at 1280 by 800: the New
//! page in both themes, a search, the details panel with an ASME B sheet chosen, Add Page, and
//! the Home tab with the engineering workspace. Same harness and rules as `snapshots.rs` (needs a
//! wgpu adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with
//! `UPDATE_SNAPSHOTS=1`). Previews are settled (none pending) before every snapshot.

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

fn window(dark: bool) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app.session.view.dark_mode = dark;
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

/// Build every preview (one per frame), with a cap, then snapshot.
fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    for _ in 0..200 {
        h.run_steps(1);
        if h.state().gallery.pending == 0 {
            break;
        }
    }
    assert_eq!(h.state().gallery.pending, 0, "{name}: previews settled");
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

#[test]
fn gallery_snapshots() {
    if !wgpu_available() {
        eprintln!("gallery snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    let mut h = window(false);
    run(&mut h, "ui.gallery", json!({"mode": "new"}));
    snap(&mut results, &mut h, "gallery-light");
    let mut h = window(true);
    run(&mut h, "ui.gallery", json!({"mode": "new"}));
    snap(&mut results, &mut h, "gallery-dark");

    let mut h = window(false);
    run(&mut h, "ui.gallery", json!({"query": "er"}));
    snap(&mut results, &mut h, "gallery-search-er");

    let mut h = window(false);
    run(&mut h, "ui.gallery", json!({"sheet": "asme.b", "select": "network.physical"}));
    snap(&mut results, &mut h, "gallery-details-network-asme-b");

    let mut h = window(false);
    run(&mut h, "ui.gallery", json!({"mode": "page"}));
    snap(&mut results, &mut h, "gallery-add-page");

    let mut h = window(false);
    run(&mut h, "ui.preset", json!({"name": "engineering"}));
    snap(&mut results, &mut h, "home-engineering");
}
