//! A pixel snapshot of the canvas with the insertion handles showing around a flowchart step the
//! pointer rests on. Same rig and rules as `tests/snapshots.rs`: it needs a wgpu adapter, passes
//! with a note without one (or with `VIZCRAFT_SKIP_SNAPSHOTS` set), and `UPDATE_SNAPSHOTS=1`
//! rewrites the reference in `tests/snapshots/<os>/`.

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

/// Whether wgpu can render here (see `tests/snapshots.rs`; this is the only test in the binary).
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

#[test]
fn insertion_handles_snapshot() {
    if !wgpu_available() {
        eprintln!("insertion snapshot: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut s = Session::new(vizcraft_engine::Document::new());
    for (id, ty, name, y) in [("start", "flowchart.Terminator", "Start", 0), ("work", "flowchart.Process", "Do the work", 140)] {
        s.run("element.create", &json!({"type": ty, "name": name, "id": id, "x": 0, "y": y})).unwrap();
    }
    s.run("relationship.create", &json!({"from": "start", "to": "work"})).unwrap();
    s.run("edit.selectNone", &json!({})).unwrap();
    let mut app = CraftApp::new(s, Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_pixels_per_point(1.0).with_step_dt(1.0 / 60.0).wgpu().build_ui_state(
        |ui, app: &mut CraftApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.input_mut().max_texture_side = Some(8192);
    h.run_steps(6);
    let shape = h.state().session.doc.page().unwrap().shapes.iter().find(|s| s.element.as_deref() == Some("work")).unwrap().clone();
    let c = shape.center();
    h.state_mut().run("view.pan", json!({"x": c.x, "y": c.y})).unwrap();
    h.run_steps(2);
    let vp = h.state().canvas.viewport(&h.state().session.view).unwrap();
    let p = vp.world_to_screen(egui::pos2(c.x as f32, c.y as f32));
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(30);
    assert_eq!(h.state().insertion.shown.as_deref(), Some(shape.id.as_str()), "the handles show");
    h.snapshot_options("insertion-handles", &SnapshotOptions::new().output_path(output_dir()));
}
