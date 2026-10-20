//! Pixel snapshots of a multi-selection's group frame: the sample flowchart with two steps and
//! the connector between them selected, and a contact and a coil with the wire between them on
//! an electrical page, each with one frame, eight resize handles and the rotate handle. Same
//! harness and rules as `snapshots.rs` (needs a wgpu adapter, skipped without one or with
//! `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_engine::{Document, Session};
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

fn window(session: Session) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(session, Services::default());
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
fn group_transform_snapshots() {
    if !wgpu_available() {
        eprintln!("group transform snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // Two flowchart steps and the connector between them.
    let mut h = window(Session::new(vizcraft_engine::sample::sample_document()));
    let between = {
        let page = h.state().session.doc.page().unwrap();
        page.connectors.iter().find(|c| c.start.shape() == Some("s2") && c.end.shape() == Some("s3")).map(|c| c.id.clone())
    };
    let mut ids = vec![json!("s2"), json!("s3")];
    ids.extend(between.map(|c| json!(c)));
    run(&mut h, "edit.select", json!({"ids": ids}));
    snap(&mut results, &mut h, "canvas-group-frame");

    // A contact and a coil with the wire between them on an electrical page.
    let mut session = Session::new(Document::new());
    session.run("template.new", &json!({"template": "electrical.controlCircuit"})).unwrap();
    let mut h = window(session);
    run(&mut h, "ui.collapsePanes", json!({"side": "left", "value": true}));
    run(&mut h, "ui.collapsePanes", json!({"side": "right", "value": true}));
    let c = {
        let app = h.state();
        let vp = app.canvas.viewport(&app.session.view).unwrap();
        vp.screen_to_world(app.canvas.rect.unwrap().center())
    };
    let (x, y) = (f64::from(c.x).round(), f64::from(c.y).round());
    let a = run(&mut h, "glyph.insert", json!({"builder": "contact", "x": x - 160.0, "y": y - 40.0}))["id"].clone();
    let b = run(&mut h, "glyph.insert", json!({"builder": "coil", "x": x + 80.0, "y": y + 20.0}))["id"].clone();
    let w = run(&mut h, "wire.draw", json!({"from": {"shape": a, "point": 1}, "to": {"shape": b, "point": 0}}))["id"].clone();
    run(&mut h, "edit.select", json!({"ids": [a, b, w]}));
    snap(&mut results, &mut h, "canvas-group-frame-electrical");
}
