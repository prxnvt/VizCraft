//! Pixel snapshots of the Tables tab as a list and as a matrix, with the same rig and rules as
//! `dock_snapshots.rs` (needs a wgpu adapter, skipped without one or with
//! `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions};
use serde_json::json;
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

/// The sample flowchart as model elements and flows, with owners on the processes.
fn app() -> CraftApp {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    let nodes = [
        ("start", "flowchart.Terminator", "Start", ""),
        ("gather", "flowchart.Process", "Gather input", "Intake"),
        ("valid", "flowchart.Decision", "Valid?", ""),
        ("save", "flowchart.Process", "Save diagram", "Storage"),
        ("end", "flowchart.Terminator", "End", ""),
        ("fix", "flowchart.Process", "Fix input", "Intake"),
    ];
    for (i, (id, ty, name, owner)) in nodes.iter().enumerate() {
        let mut p = json!({"id": id, "type": ty, "name": name, "x": (i % 3) as f64 * 200.0, "y": (i / 3) as f64 * 150.0});
        if !owner.is_empty() {
            p["properties"] = json!({"owner": owner});
        }
        s.run("element.create", &p).unwrap();
    }
    for (i, (a, b)) in
        [("start", "gather"), ("gather", "valid"), ("valid", "save"), ("valid", "fix"), ("fix", "gather"), ("save", "end")].iter().enumerate()
    {
        s.run("relationship.create", &json!({"id": format!("f{i}"), "from": a, "to": b})).unwrap();
    }
    let mut app = CraftApp::new(s, Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app
}

#[test]
fn a_list_and_a_matrix() {
    if !wgpu_available() {
        eprintln!("tables snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
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
    h.state_mut().run("ui.tables", json!({"view": "list", "type": "flowchart.Process"})).unwrap();
    h.run_steps(4);
    let opts = SnapshotOptions::new().output_path(output_dir());
    h.snapshot_options("tables-list", &opts);
    h.state_mut()
        .run("ui.tables", json!({"view": "matrix", "rows": "flowchart.Process", "cols": "flowchart.Process", "relationship": "flowchart.Flow"}))
        .unwrap();
    h.run_steps(4);
    h.snapshot_options("tables-matrix", &opts);
}
