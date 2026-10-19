//! Pixel snapshots of a Sequence page: the `uml.sequence` template on the canvas with a lifeline
//! selected (head and stem outlined), the same page mid-way through dragging a message to another
//! row (the settled preview and the insertion line), the Sequence tab at 1280 points wide, and the
//! Inspector's Sequence group for a selected message. Same harness and rules as
//! `nets_snapshots.rs` (needs a wgpu adapter, skipped without one or with
//! `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_diagram::{Document, Point};
use vizcraft_engine::Session;
use vizcraft_ui_egui::canvas::Drag;
use vizcraft_ui_egui::canvas::interact::sequence::SeqDrag;
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

/// The window, `height` tall, on the template page fitted into view.
fn window(height: f32) -> Harness<'static, CraftApp> {
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
    run(&mut h, "template.new", json!({"template": "uml.sequence"}));
    // A ribbon-only window has no canvas to fit.
    if height > 300.0 {
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
fn sequence_snapshots() {
    if !wgpu_available() {
        eprintln!("sequence snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // The canvas: labels above their lines, the alt's tab left of the Client stem and its guards
    // right of it, and Orders API selected (its head and stem outlined).
    let mut h = window(800.0);
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": false}));
    run(&mut h, "view.zoomFit", json!({}));
    run(&mut h, "edit.select", json!({"ids": ["api"]}));
    snap(&mut results, &mut h, "canvas-sequence");

    // Mid-drag: "rows" dragged between the call and the query, drawn from the settled preview.
    run(&mut h, "edit.select", json!({"ids": ["m3"]}));
    let (from, to) = {
        let page = h.state().session.doc.page().unwrap();
        let y = |id: &str| vizcraft_diagram::connector_geometry(page, page.connector(id).unwrap()).polyline[0].y;
        let api = page.shape("api").unwrap();
        let x = api.x + api.w + 40.0;
        (Point::new(x, y("m3")), Point::new(x, (y("m1") + y("m2")) / 2.0))
    };
    h.state_mut().canvas.drag = Some(Drag::Sequence(SeqDrag::Reorder { id: "m3".into(), from, to }));
    h.run_steps(2);
    snap(&mut results, &mut h, "canvas-sequence-reorder");

    // The Sequence tab.
    let mut h = window(230.0);
    run(&mut h, "edit.select", json!({"ids": ["m2"]}));
    run(&mut h, "ui.tab", json!({"tab": "Sequence"}));
    assert!(!Overflow::get(&h.ctx).overflows());
    snap(&mut results, &mut h, "tab-sequence");

    // The Inspector's Sequence group for a message.
    let mut h = window(800.0);
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": true}));
    run(&mut h, "edit.select", json!({"ids": ["m2"]}));
    snap(&mut results, &mut h, "inspector-sequence");
}
