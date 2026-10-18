//! Pixel snapshots of containers, swimlanes and callouts: a three-lane cross-functional
//! flowchart with a phase separator, a container and a callout on the canvas, and the contextual
//! Container tab. Same harness and rules as `snapshots.rs` (needs a wgpu adapter, skipped without
//! one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

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

/// The window at 1280x800 over a blank document.
fn window() -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
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

/// Build the cross-functional flowchart: lanes Customer, Sales and Warehouse, one step or two in
/// each, connectors between them, a phase separator, a callout and a container.
fn flowchart(h: &mut Harness<'static, CraftApp>) {
    run(h, "lane.addSet", json!({"x": 40, "y": 40, "lanes": 3, "title": "Order handling", "id": "set"}));
    let lanes: Vec<String> = vizcraft_diagram::container::lanes(h.state().session.doc.page().unwrap(), "set");
    for (lane, title) in lanes.iter().zip(["Customer", "Sales", "Warehouse"]) {
        run(h, "container.set", json!({"container": lane, "heading": title}));
    }
    // Lane bands start below the 32 high title; each is 140 high.
    let steps = [
        ("start", "flowchart.terminator", "Place order", 110, 92),
        ("check", "flowchart.decision", "In stock?", 300, 212),
        ("bill", "flowchart.process", "Send invoice", 520, 222),
        ("pick", "flowchart.process", "Pick and pack", 300, 362),
        ("ship", "flowchart.terminator", "Ship", 540, 372),
    ];
    for (id, stencil, text, x, y) in steps {
        run(h, "shape.add", json!({"stencil": stencil, "x": x, "y": y, "text": text, "id": id}));
        run(h, "container.drop", json!({"ids": [id]}));
    }
    for (a, b) in [("start", "check"), ("check", "bill"), ("check", "pick"), ("pick", "ship")] {
        run(h, "connector.add", json!({"from": {"shape": a}, "to": {"shape": b}, "route": "orthogonal"}));
    }
    run(h, "lane.addPhase", json!({"set": "set", "at": 470, "title": "Fulfilment"}));
    run(h, "callout.add", json!({"target": "check", "text": "Checked against the live stock list", "x": 820, "y": 120, "style": "note", "id": "co"}));
    run(h, "shape.add", json!({"stencil": "basic.rectangle", "x": 870, "y": 400, "w": 90, "h": 50, "text": "Courier", "id": "courier"}));
    run(h, "container.add", json!({"ids": ["courier"], "heading": "Partners", "style": "sage", "id": "partners"}));
    run(h, "edit.selectNone", json!({}));
    run(h, "ui.collapsePanes", json!({"side": "left", "value": true}));
    run(h, "ui.collapsePanes", json!({"side": "right", "value": true}));
    run(h, "view.zoomFit", json!({}));
}

#[test]
fn container_snapshots() {
    if !wgpu_available() {
        eprintln!("container snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();
    let mut h = window();
    flowchart(&mut h);
    snap(&mut results, &mut h, "containers-flowchart");
    // The Container tab, for the selected container.
    run(&mut h, "edit.select", json!({"ids": ["partners"]}));
    run(&mut h, "ui.tab", json!({"tab": "Container"}));
    snap(&mut results, &mut h, "ribbon-container");
}
