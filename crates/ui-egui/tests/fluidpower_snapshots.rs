//! Pixel snapshots of fluid power pages: the notation tab with its Circuit groups (Symbols,
//! Lines, Codes, Sequence, Analyze) in a window wide enough for every group, the hydraulic and pneumatic examples on
//! the canvas, and the Inspector's Step Chart group for the pneumatic example's chart. Same
//! harness and rules as `nets_snapshots.rs` (needs a wgpu adapter, skipped without one or with
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

/// The window, `height` tall, with example `example` open.
fn window(width: f32, height: f32, example: &str) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(Document::new()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    let mut h = Harness::builder().with_size(egui::vec2(width, height)).with_pixels_per_point(1.0).wgpu().build_ui_state(
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
    if height > 300.0 {
        run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
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
fn fluidpower_snapshots() {
    if !wgpu_available() {
        eprintln!("fluid power snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // The notation tab, named after the pack, with the Circuit groups after the generic ones; a
    // wide window so every group shows without scrolling, the Pilot line armed.
    let mut h = window(2500.0, 230.0, "fluidpower.hydraulicExample");
    run(&mut h, "ui.tab", json!({"tab": "Fluid Power"}));
    run(&mut h, "ui.tool", json!({"tool": "pipe", "line": "fluidpower.pilot"}));
    snap(&mut results, &mut h, "tab-fluidpower");

    // Both examples on the canvas.
    let mut h = window(1280.0, 800.0, "fluidpower.hydraulicExample");
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": false}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "canvas-fluidpower-hydraulic");
    let mut h = window(1280.0, 800.0, "fluidpower.pneumaticExample");
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": false}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "canvas-fluidpower-pneumatic");

    // The Inspector's Step Chart group for the pneumatic example's chart.
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": true}));
    let chart = {
        let doc = &h.state().session.doc;
        let e = doc.model.elements.iter().find(|e| e.type_id == "fluidpower.StepChart").unwrap().id.clone();
        doc.page().unwrap().shapes.iter().find(|s| s.element.as_deref() == Some(e.as_str())).unwrap().id.clone()
    };
    run(&mut h, "edit.select", json!({"ids": [chart]}));
    snap(&mut results, &mut h, "inspector-step-chart");
}
