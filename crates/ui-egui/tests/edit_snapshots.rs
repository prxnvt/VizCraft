//! Pixel snapshots of the editing tools: the Find and Replace dialog over the sample after a
//! search, and the sample's connectors drawn with dash patterns, round caps, transparency and
//! rounded bends. Same harness and rules as `snapshots.rs` (needs a wgpu adapter, skipped without
//! one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::dialogs::Dialog;
use vizcraft_ui_egui::{CraftApp, Services, find_dialog};

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
fn edit_snapshots() {
    if !wgpu_available() {
        eprintln!("edit snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // Connectors with every line style field.
    let mut h = window();
    let ids: Vec<String> = h.state().session.doc.page().unwrap().connectors.iter().map(|c| c.id.clone()).collect();
    let styles = [
        json!({"dash": "dashed", "lineWidth": 2}),
        json!({"dash": "dotted", "lineWidth": 3, "lineCap": "round", "line": "#c0392b"}),
        json!({"dash": "dashDot", "lineWidth": 2, "lineTransparency": 0.5}),
        json!({"dash": "longDash", "lineWidth": 2.5, "cornerRadius": 12, "line": "#2f62a3"}),
    ];
    for (id, st) in ids.iter().zip(styles.iter().cycle()) {
        let mut p = st.clone();
        p["ids"] = json!([id]);
        run(&mut h, "shape.setStyle", p);
    }
    run(&mut h, "edit.selectNone", json!({}));
    run(&mut h, "ui.collapsePanes", json!({"side": "left", "value": true}));
    run(&mut h, "ui.collapsePanes", json!({"side": "right", "value": true}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "connectors-dashed");

    // The Find dialog after a search that found a hit.
    let mut h = window();
    run(&mut h, "ui.find", json!({}));
    let mut state = find_dialog::State::default();
    state.text = "diagram".into();
    state.with = "drawing".into();
    state.document = true;
    find_dialog::find_next(h.state_mut(), &mut state);
    h.state_mut().dialog = Some(Dialog::Find(state));
    h.run_steps(3);
    snap(&mut results, &mut h, "dialog-find");
}
