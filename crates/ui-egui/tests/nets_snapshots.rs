//! Pixel snapshot of engineering drawing on the canvas: a contact and a coil placed by
//! `glyph.insert`, a numbered wire between their pins with a branch onto it at a junction dot,
//! a pipe with a line marker, and the contact turned and mirrored with its terminal numbers
//! upright. Same harness and rules as `snapshots.rs` (needs a wgpu adapter, skipped without one
//! or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

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
fn nets_snapshots() {
    if !wgpu_available() {
        eprintln!("nets snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();
    let mut doc = Document::new();
    if let Some(page) = doc.page_mut() {
        page.board = vizcraft_diagram::Board::Fixed;
        page.width = 700.0;
        page.height = 400.0;
    }
    let mut h = window(doc);
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "properties", "value": false}));
    let k = run(&mut h, "glyph.insert", json!({"builder": "contact", "x": 120, "y": 100}))["id"].as_str().unwrap().to_string();
    let q = run(&mut h, "glyph.insert", json!({"builder": "coil", "x": 420, "y": 100}))["id"].as_str().unwrap().to_string();
    let w = run(
        &mut h,
        "wire.draw",
        json!({"from": {"shape": k, "point": 1}, "to": {"shape": q, "point": 1}, "points": [{"x": 130, "y": 260}, {"x": 430, "y": 260}]}),
    );
    let (wire, net) = (w["id"].as_str().unwrap().to_string(), w["net"].as_str().unwrap().to_string());
    run(&mut h, "net.set", json!({"net": net, "number": "2003"}));
    run(&mut h, "wire.draw", json!({"from": {"x": 280, "y": 340}, "to": {"connector": wire, "x": 280, "y": 260}}));
    let p = run(&mut h, "wire.draw", json!({"from": {"x": 80, "y": 360}, "to": {"x": 600, "y": 360}, "kind": "pipe"}));
    // No command sets a marker yet (pack connector types do); set it on the document directly.
    let pid = p["id"].as_str().unwrap().to_string();
    if let Some(c) = h.state_mut().session.doc.page_mut().and_then(|pg| pg.connector_mut(&pid)) {
        c.marker = vizcraft_diagram::line_marks::LineMarker::DoubleSlash;
    }
    run(&mut h, "shape.rotate", json!({"ids": [k], "turns": 1}));
    run(&mut h, "shape.mirror", json!({"ids": [k], "axis": "x"}));
    run(&mut h, "edit.selectNone", json!({}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "canvas-wire-number");
}
