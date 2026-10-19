//! Pixel snapshots of the Generate dock tab after a preview and of the Generate ribbon tab, at
//! 1280 px, with the same rig and rules as `dock_snapshots.rs` (needs a wgpu adapter, skipped
//! without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::ribbon::Overflow;
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

/// The sample with three typed elements and a flow, saved in a fresh folder so a relative output
/// (the same text on every machine) resolves beside it.
fn app() -> CraftApp {
    let dir = std::env::temp_dir().join(format!("vizcraft-generate-snapshots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::new(vizcraft_engine::sample::sample_document());
    for (id, ty, name, x) in [
        ("gin", "flowchart.Process", "Gather input", 40),
        ("ok", "flowchart.Decision", "Valid?", 240),
        ("save", "flowchart.Process", "Save diagram", 440),
    ] {
        s.run("element.create", &json!({"type": ty, "name": name, "id": id, "x": x, "y": 700})).unwrap();
    }
    s.run("relationship.create", &json!({"from": "gin", "to": "ok", "id": "f1"})).unwrap();
    s.path = Some(dir.join("sample.vizcraft"));
    let mut app = CraftApp::new(s, Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app
}

#[test]
fn generate_dock_and_ribbon_tab() {
    if !wgpu_available() {
        eprintln!("generate snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_pixels_per_point(1.0).wgpu().build_ui_state(
        |ui, app: &mut CraftApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app(),
    );
    h.input_mut().max_texture_side = Some(8192);
    h.run_steps(6);
    h.state_mut().run("ui.collapsePanes", json!({"side": "left", "value": true})).unwrap();
    h.state_mut().run("ui.collapsePanes", json!({"side": "right", "value": true})).unwrap();
    h.state_mut().run("ui.dock", json!({"tab": "generate", "height": 320})).unwrap();
    h.state_mut().run("ui.generate", json!({"target": "markdown-catalog", "output": "generated", "run": "preview"})).unwrap();
    h.state_mut().run("ui.tab", json!({"tab": "Generate"})).unwrap();
    h.run_steps(6);
    assert!(!Overflow::get(&h.ctx).overflows(), "the Generate tab fits 1280 wide");
    let opts = SnapshotOptions::new().output_path(output_dir());
    h.snapshot_options("generate-dock-preview", &opts);
    if let Some(dir) = h.state().session.path.as_ref().and_then(|p| p.parent()) {
        let _ = std::fs::remove_dir_all(dir);
    }
}
