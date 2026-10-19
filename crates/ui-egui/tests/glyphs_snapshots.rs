//! Pixel snapshots of the notation drawing foundation on the canvas: a freeform with a hole
//! (filled even-odd, the page showing through) and an open freeform line, connectors with end
//! labels and the slash end, and port marks on typed shapes. Same harness and rules as
//! `snapshots.rs` (needs a wgpu adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`;
//! refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_diagram::{Arrow, Connector, Document, End, Freeform, Path, Point, Route, Shape, ShapeKind};
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

/// A pack with one element type that declares an in, an out and an in-out port.
fn load_ported_pack() {
    let pack = json!({
        "id": "snapports", "version": "1.0.0", "name": "Ports",
        "glyphs": [{"id": "snapports.block", "label": "Block", "category": "Blocks", "outline": "rectangle", "w": 140, "h": 80, "fill": "#eef3f8", "line": "#345b84", "elementType": "snapports.Block"}],
        "elementTypes": [{"id": "snapports.Block", "glyph": "snapports.block", "ports": [
            {"id": "in", "label": "In", "direction": "in", "x": 0, "y": 0.5},
            {"id": "out", "label": "Out", "direction": "out", "x": 1, "y": 0.5},
            {"id": "bus", "label": "Bus", "direction": "inOut", "x": 0.5, "y": 1}
        ]}],
        "relationshipTypes": [{"id": "snapports.Wire"}],
        "connections": [{"source": ["*"], "relationship": "snapports.Wire", "target": ["*"]}],
        "connectorTypes": [{"id": "snapports.wire", "label": "Wire", "relationshipType": "snapports.Wire"}]
    });
    let p = vizcraft_notation::loader::load_json(&pack.to_string()).expect("the test pack loads");
    vizcraft_notation::catalog::add_pack(p).expect("the test pack installs");
}

/// A fixed page with a ring (a square with a square hole and a line across its top band), an
/// open zigzag, and two boxes joined by a connector with end labels and a slash at its start.
fn document() -> Document {
    let p = |x: f64, y: f64| Point::new(x, y);
    let mut doc = Document::new();
    let page = doc.page_mut().expect("a new document has a page");
    page.board = vizcraft_diagram::Board::Fixed;
    page.width = 900.0;
    page.height = 420.0;
    let mut path = Path::polygon(&[p(0.0, 0.0), p(160.0, 0.0), p(160.0, 160.0), p(0.0, 160.0)]);
    path.els.extend(Path::polygon(&[p(50.0, 50.0), p(110.0, 50.0), p(110.0, 110.0), p(50.0, 110.0)]).els);
    path.els.extend(Path::polyline(&[p(0.0, 25.0), p(160.0, 25.0)]).els);
    let mut ring = Shape::new("ring", ShapeKind::Freeform, 40.0, 40.0, 160.0, 160.0);
    ring.path = Some(Freeform::new(&path, true));
    ring.style.fill = vizcraft_diagram::Color::rgb(0x9f, 0xc3, 0xe6);
    let mut zig = Shape::new("zig", ShapeKind::Freeform, 40.0, 250.0, 160.0, 80.0);
    zig.path = Some(Freeform::new(&Path::polyline(&[p(0.0, 80.0), p(40.0, 0.0), p(80.0, 80.0), p(120.0, 0.0), p(160.0, 80.0)]), false));
    let a = Shape::new("a", ShapeKind::Rectangle, 300.0, 60.0, 120.0, 60.0);
    let b = Shape::new("b", ShapeKind::Rectangle, 640.0, 60.0, 120.0, 60.0);
    let mut c = Connector::new("c", End::dynamic("a"), End::dynamic("b"));
    c.route = Route::Straight;
    c.start_arrow = Arrow::Slash;
    c.start_label = "1".into();
    c.end_label = "0..*".into();
    c.text = "owns".into();
    for s in [&mut ring, &mut zig] {
        s.sanitize();
    }
    page.shapes.extend([ring, zig, a, b]);
    page.connectors.push(c);
    doc
}

fn window(doc: Document) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(doc), Services::default());
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
fn glyph_snapshots() {
    if !wgpu_available() {
        eprintln!("glyph snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    load_ported_pack();
    let mut results = SnapshotResults::new();

    // Holes, open freeforms, the slash end and end labels, at 100 percent.
    let mut h = window(document());
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "properties", "value": false}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "canvas-holes-end-labels");

    // Port marks on two typed shapes joined at their ports, labels shown from 100 percent.
    let mut h = window(document());
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "properties", "value": false}));
    let one = run(&mut h, "element.create", json!({"type": "snapports.Block", "name": "Source", "x": 300, "y": 240}));
    let two = run(&mut h, "element.create", json!({"type": "snapports.Block", "name": "Sink", "x": 640, "y": 240}));
    let (one, two) = (one["shape"].as_str().unwrap().to_string(), two["shape"].as_str().unwrap().to_string());
    run(&mut h, "connector.add", json!({"from": {"shape": one, "point": 1}, "to": {"shape": two, "point": 0}, "kind": "snapports.wire"}));
    run(&mut h, "edit.selectNone", json!({}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "canvas-ports");
}
