//! A drag mid-way: a wall dragged across a connector's line. The drag preview reroutes the
//! connector around the wall every frame, so the wall is never seen being crossed while it
//! moves. Checked by the routing oracle on the preview page, and as a pixel snapshot with the
//! same rig and rules as `snapshots.rs` (needs a wgpu adapter, skipped without one or with
//! `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions};
use serde_json::json;
use vizcraft_diagram::autoroute::crossings;
use vizcraft_diagram::{Connector, Document, End, Page, Point, Shape, ShapeKind};
use vizcraft_engine::Session;
use vizcraft_ui_egui::canvas::{Drag, interact};
use vizcraft_ui_egui::{CraftApp, Services};

fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

/// Whether wgpu can render here (see `snapshots.rs`; this is the only snapshot in this binary).
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

/// Two boxes joined by a connector whose straight line is clear, and a wall below it.
fn document() -> Document {
    let mut page = Page::new("p1", "Page-1");
    for (id, text, x, y, w, h) in
        [("a", "Request", 120.0, 300.0, 140.0, 70.0), ("b", "Response", 760.0, 300.0, 140.0, 70.0), ("wall", "Wall", 450.0, 520.0, 90.0, 220.0)]
    {
        let mut s = Shape::new(id, ShapeKind::Rectangle, x, y, w, h);
        s.text = text.into();
        page.shapes.push(s);
    }
    page.connectors.push(Connector::new("c", End::dynamic("a"), End::dynamic("b")));
    let mut doc = Document::new();
    doc.pages = vec![page];
    doc
}

/// The wall dragged up across the connector's line, still held.
fn drag() -> Drag {
    Drag::Move { ids: vec!["wall".into()], from: Point::new(495.0, 630.0), to: Point::new(495.0, 353.0) }
}

#[test]
fn the_drag_preview_reroutes_around_the_moving_shape() {
    let doc = document();
    let page = doc.page().unwrap();
    assert!(crossings(page).is_empty(), "before the drag the line is clear");
    let preview = interact::preview_page(page, &drag(), 1.0).unwrap();
    assert!(crossings(&preview).is_empty(), "mid-drag the connector goes around the wall: {:?}", crossings(&preview));
    assert!(!preview.connector("c").unwrap().waypoints.is_empty(), "it was rerouted");
    // Without the preview reroute the moved wall would sit on the line.
    let mut stale = page.clone();
    vizcraft_diagram::arrange::translate(&mut stale, &["wall".to_string()], 0.0, -277.0);
    assert!(!crossings(&stale).is_empty());
}

#[test]
fn drag_preview_snapshot() {
    if !wgpu_available() {
        eprintln!("routing snapshot: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut app = CraftApp::new(Session::new(document()), Services::default());
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
    for (id, params) in [
        ("ui.collapsePanes", json!({"side": "left", "value": true})),
        ("ui.collapsePanes", json!({"side": "right", "value": true})),
        ("view.zoomFit", json!({})),
        ("edit.select", json!({"ids": ["wall"]})),
    ] {
        h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
        h.run_steps(3);
    }
    h.state_mut().canvas.drag = Some(drag());
    h.run_steps(4);
    assert!(h.state().canvas.drag.is_some(), "the drag is still held");
    h.snapshot_options("drag-preview-routing", &SnapshotOptions::new().output_path(output_dir()));
}
