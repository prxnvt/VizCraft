//! Pixel snapshots of analysis on the canvas and in the dock: a Downstream result (halos with
//! level chips, the rest dimmed, the Analysis tab's rows) and a compare against the saved file
//! (a new node in green, a renamed one in amber, a deleted one as a dashed red ghost, the
//! Added, Removed and Changed rows). Same harness and rules as `dock_snapshots.rs` (needs a wgpu
//! adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with
//! `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::{Value, json};
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

fn run(h: &mut Harness<'static, CraftApp>, id: &str, p: Value) -> Value {
    let r = h.state_mut().run(id, p).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
    r
}

/// A small flowchart: Start, Do, Ok? in a column, a side branch Log off Do, and an island Lone.
fn window() -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app.ui.tab = "Model".into();
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
    run(&mut h, "ui.collapsePanes", json!({"side": "left", "value": true}));
    run(&mut h, "ui.collapsePanes", json!({"side": "right", "value": true}));
    for (ty, name, id, x, y) in [
        ("flowchart.Terminator", "Start", "start", 200, 40),
        ("flowchart.Process", "Do", "do", 200, 160),
        ("flowchart.Decision", "Ok?", "ok", 200, 280),
        ("flowchart.Process", "Log", "log", 420, 160),
        ("flowchart.Process", "Lone", "lone", 640, 40),
    ] {
        run(&mut h, "element.create", json!({"type": ty, "name": name, "id": id, "x": x, "y": y}));
    }
    for (from, to, id) in [("start", "do", "f1"), ("do", "ok", "f2"), ("do", "log", "f3")] {
        run(&mut h, "relationship.create", json!({"from": from, "to": to, "id": id}));
    }
    run(&mut h, "edit.selectNone", json!({}));
    run(&mut h, "view.zoom", json!({"value": 100}));
    run(&mut h, "view.pan", json!({"x": 420, "y": 230}));
    h
}

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(3);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

#[test]
fn analysis_snapshots() {
    if !wgpu_available() {
        eprintln!("analysis snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // Downstream of Start: halos with level chips, the island dimmed; the dock lists the rows.
    let mut h = window();
    run(&mut h, "analyze.dependencies", json!({"id": "start"}));
    run(&mut h, "ui.dock", json!({"open": false}));
    snap(&mut results, &mut h, "canvas-analysis-dependencies");
    run(&mut h, "ui.dock", json!({"tab": "analysis", "height": 260}));
    snap(&mut results, &mut h, "dock-analysis");

    // Compare with the saved file after a rename, a delete and an add.
    let mut h = window();
    let dir = std::env::temp_dir().join(format!("vizcraft-analysis-snap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    run(&mut h, "file.saveAs", json!({"path": dir.join("flow.vizcraft").to_string_lossy()}));
    run(&mut h, "element.rename", json!({"id": "do", "name": "Done"}));
    run(&mut h, "model.delete", json!({"ids": ["log"]}));
    run(&mut h, "element.create", json!({"type": "flowchart.Process", "name": "Audit", "id": "audit", "x": 420, "y": 280}));
    run(&mut h, "relationship.create", json!({"from": "ok", "to": "audit", "id": "f4"}));
    run(&mut h, "edit.selectNone", json!({}));
    run(&mut h, "model.diff", json!({}));
    // The save message names a temporary folder that differs between runs.
    h.state_mut().status_msg = None;
    run(&mut h, "ui.dock", json!({"open": false}));
    snap(&mut results, &mut h, "canvas-analysis-diff");
    run(&mut h, "ui.dock", json!({"tab": "analysis", "height": 260}));
    snap(&mut results, &mut h, "dock-analysis-diff");
    std::fs::remove_dir_all(&dir).unwrap();
}
