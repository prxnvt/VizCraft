//! Pixel snapshots of shape styles and the Design tab: a page with gradients, dash patterns,
//! shadows, glow, soft edges, rounded corners and a decorated background on the canvas, and the
//! Design tab's ribbon with a theme applied. Same harness and rules as `snapshots.rs` (needs a
//! wgpu adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with
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

#[test]
fn style_snapshots() {
    if !wgpu_available() {
        eprintln!("style snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // The sample flowchart with every effect on some shape, over a decorated page.
    let mut h = window();
    let ids: Vec<String> = h.state().session.doc.page().unwrap().shapes.iter().map(|s| s.id.clone()).collect();
    let styles = [
        json!({"gradient": "linear", "cornerRadius": 10, "shadow": {"dx": 4, "dy": 4, "blur": 6}}),
        json!({"dash": "dashed", "lineWidth": 2.5, "lineCap": "round", "gradient": "radial"}),
        json!({"fill": "#f8ded9", "dash": "dotted", "lineWidth": 3, "glow": 8}),
        json!({"gradient": {"kind": "linear", "angle": 0, "stops": [{"offset": 0, "color": "#2f62a3"}, {"offset": 0.5, "color": "#2f62a3"}, {"offset": 0.5, "color": "#dde8f6"}, {"offset": 1, "color": "#dde8f6"}]}, "cornerRadius": 6}),
        json!({"softEdges": 6, "fill": "#e1efdb", "dash": "dashDot"}),
        json!({"fillTransparency": 0.5, "shadow": true, "dash": "longDash", "cornerRadius": 14}),
    ];
    for (id, st) in ids.iter().zip(styles.iter().cycle()) {
        let mut p = st.clone();
        p["ids"] = json!([id]);
        run(&mut h, "shape.setStyle", p);
    }
    run(&mut h, "shape.rotate", json!({"ids": [ids[0].clone()], "angle": 12}));
    run(&mut h, "design.setBackground", json!({"kind": "pattern"}));
    run(&mut h, "design.setBorderTitle", json!({"kind": "frame"}));
    run(&mut h, "edit.selectNone", json!({}));
    run(&mut h, "ui.collapsePanes", json!({"side": "left", "value": true}));
    run(&mut h, "ui.collapsePanes", json!({"side": "right", "value": true}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "styles-canvas");

    // The Design tab with a theme and variant chosen.
    let mut h = window();
    run(&mut h, "design.setTheme", json!({"theme": "orchard", "variant": 1}));
    run(&mut h, "ui.tab", json!({"tab": "Design"}));
    snap(&mut results, &mut h, "ribbon-design");
}
