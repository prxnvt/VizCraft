//! Pixel snapshots of the wave 4 mechanical, reliability, IDEF0 and annotation work: the
//! Mechanical, Reliability and IDEF0 tabs and the Sheet tab with its Annotate group (ribbon
//! strips), and the four-bar, fault tree, IDEF0 A0 and bracket examples on the canvas. Same
//! harness and rules as `sequence_snapshots.rs` (needs a wgpu adapter, skipped without one or with
//! `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

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

/// The window, `height` tall, with `example` open on tab `tab`.
fn window(height: f32, example: &str, tab: &str) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(Document::new()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
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
    run(&mut h, "example.open", json!({"example": example}));
    h.state_mut().ui.tab = tab.into();
    if height > 300.0 {
        run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
        run(&mut h, "ui.pane", json!({"name": "shapeData", "value": false}));
        run(&mut h, "view.zoomFit", json!({}));
    }
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
fn mechanical_snapshots() {
    if !wgpu_available() {
        eprintln!("mechanical snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();
    for (example, tab, name) in [
        ("mechanical.fourBar", "Mechanical", "ribbon_mechanical"),
        ("reliability.pumpFmea", "Reliability", "ribbon_reliability"),
        ("idef0.makeWidget", "IDEF0", "ribbon_idef0"),
        ("annotation.bracket", "Sheet", "ribbon_sheet_annotate"),
    ] {
        let mut h = window(170.0, example, tab);
        snap(&mut results, &mut h, name);
    }
    for (example, page, name) in [
        ("mechanical.fourBar", 0usize, "example_four_bar"),
        ("reliability.pumpFaultTree", 0, "example_fault_tree"),
        ("idef0.makeWidget", 1, "example_idef0_a0"),
        ("annotation.bracket", 0, "example_gdt_bracket"),
    ] {
        let mut h = window(800.0, example, "Home");
        run(&mut h, "page.setActive", json!({"page": page}));
        run(&mut h, "view.zoomFit", json!({}));
        snap(&mut results, &mut h, name);
    }
}
