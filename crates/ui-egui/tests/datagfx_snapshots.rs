//! Pixel snapshots of data graphics: a shape with a number row showing a graphic of all four
//! kinds (a callout below, a bar on the right, an icon in the top right corner and a colour by
//! value) on the canvas, and the Edit Data Graphic dialog. The test also checks that the canvas
//! leaves the shape's style alone. Same harness and rules as `snapshots.rs` (needs a wgpu
//! adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with
//! `UPDATE_SNAPSHOTS=1`).

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

fn window() -> Harness<'static, CraftApp> {
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

/// The sample with a four-kind graphic on its second shape; returns that shape's id.
fn with_graphic(h: &mut Harness<'static, CraftApp>) -> String {
    let id = h.state().session.doc.page().unwrap().shapes[1].id.clone();
    run(h, "data.set", json!({"ids": [id], "key": "cost", "value": 75}));
    run(
        h,
        "datagraphic.define",
        json!({"name": "Cost", "items": [
            {"kind": "text", "key": "cost", "label": "Cost", "format": "$#,##0.00"},
            {"kind": "bar", "key": "cost", "position": "right", "color": "#16a34a"},
            {"kind": "icons", "key": "cost", "position": "topRight", "rules": [{"op": ">", "value": 50, "icon": "check"}, {"op": "any", "icon": "close"}]},
            {"kind": "color", "key": "cost", "rules": [{"op": "between", "value": 50, "to": 100, "fill": "#fef3c7"}]},
        ]}),
    );
    run(h, "datagraphic.apply", json!({"ids": [id], "graphic": "dg1"}));
    id
}

#[test]
fn data_graphic_snapshots() {
    if !wgpu_available() {
        eprintln!("data graphic snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // The four kinds on the canvas, zoomed in on the shape.
    let mut h = window();
    let id = with_graphic(&mut h);
    let style = h.state().session.doc.shape(&id).unwrap().style.clone();
    run(&mut h, "ui.tab", json!({"tab": "Data"}));
    run(&mut h, "view.zoom", json!({"value": 200}));
    let c = h.state().session.doc.shape(&id).unwrap().center();
    h.state_mut().session.view.pan = [c.x as f32 + 40.0, c.y as f32 + 10.0];
    snap(&mut results, &mut h, "canvas-data-graphic");
    assert_eq!(h.state().session.doc.shape(&id).unwrap().style, style, "drawing the colour by value changes no style");

    // The Edit Data Graphic dialog on that graphic.
    run(&mut h, "edit.select", json!({"ids": [id]}));
    run(&mut h, "ui.dialog", json!({"name": "dataGraphic"}));
    snap(&mut results, &mut h, "dialog-data-graphic");
}
