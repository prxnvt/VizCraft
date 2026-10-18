//! Pixel snapshots of text: a page of shapes showing the font, paragraph and text block settings
//! on the canvas (family, size, bold, italic, underline, strikethrough, highlight, alignment,
//! justify, bullets, indent, vertical and right-to-left text, a moved and turned text block, a
//! rotated shape, a text box and fields), the Home tab with the Paragraph group showing a
//! selection's settings, and the in-place editor on a rotated shape. Same harness and rules as
//! `snapshots.rs` (needs a wgpu adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`;
//! refresh with `UPDATE_SNAPSHOTS=1`).

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

/// A window on an empty document.
fn window() -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::Document::new()), Services::default());
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
    let v = h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
    v
}

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

/// A rectangle with `text` at `(x, y)`, its text styled by `format` and `paragraph`.
fn shape(h: &mut Harness<'static, CraftApp>, id: &str, x: f64, y: f64, text: &str, format: serde_json::Value, paragraph: serde_json::Value) {
    run(h, "shape.add", json!({"stencil": "basic.rectangle", "id": id, "x": x, "y": y, "w": 200, "h": 110, "text": text}));
    if format.as_object().is_some_and(|o| !o.is_empty()) {
        let mut p = format;
        p["ids"] = json!([id]);
        run(h, "text.setFormat", p);
    }
    if paragraph.as_object().is_some_and(|o| !o.is_empty()) {
        let mut p = paragraph;
        p["ids"] = json!([id]);
        run(h, "text.setParagraph", p);
    }
}

/// The text page: one shape per setting.
fn text_page(h: &mut Harness<'static, CraftApp>) {
    run(h, "page.rename", json!({"name": "Floor plan"}));
    shape(
        h,
        "f1",
        40.0,
        40.0,
        "Georgia 16 bold\nitalic underlined",
        json!({"fontFamily": "Georgia", "size": 16, "bold": true, "italic": true, "underline": true}),
        json!({}),
    );
    shape(
        h,
        "f2",
        280.0,
        40.0,
        "Struck through\nand highlighted",
        json!({"strikethrough": true, "highlight": "#fff3a0", "color": "#b03a2e"}),
        json!({}),
    );
    shape(h, "f3", 520.0, 40.0, "Mono at the\ntop left", json!({"fontFamily": "JetBrains Mono"}), json!({"hAlign": "left", "vAlign": "top"}));
    shape(
        h,
        "j1",
        40.0,
        190.0,
        "Justified text fills each wrapped row to both margins of the block.",
        json!({}),
        json!({"hAlign": "justify", "vAlign": "bottom"}),
    );
    shape(h, "n1", 280.0, 190.0, "First\nSecond\nThird", json!({}), json!({"bullets": "number", "hAlign": "left", "indent": 20}));
    shape(h, "r2", 520.0, 190.0, "Bullets\non the right", json!({}), json!({"bullets": "disc", "hAlign": "left", "direction": "rtl"}));
    shape(h, "d1", 40.0, 340.0, "Vertical text", json!({"size": 14}), json!({"direction": "vertical"}));
    shape(h, "b1", 280.0, 340.0, "Moved block", json!({}), json!({}));
    run(h, "text.setBlock", json!({"id": "b1", "dx": 40, "dy": 30, "w": 110, "h": 30, "angle": 20}));
    shape(h, "r1", 520.0, 340.0, "Rotated shape", json!({"size": 14}), json!({}));
    run(h, "shape.rotate", json!({"ids": ["r1"], "angle": 30}));
    run(
        h,
        "shape.addTextBox",
        json!({"x": 40, "y": 490, "w": 420, "h": 30, "text": "Text box on {{pageName}}, page {{pageNumber}} of {{pageCount}}"}),
    );
    shape(h, "t1", 520.0, 470.0, "w {{width}}, h {{height}}\n[{{data:missing}}]", json!({}), json!({}));
    run(h, "text.setCase", json!({"ids": ["f2"], "case": "upper"}));
}

#[test]
fn text_snapshots() {
    if !wgpu_available() {
        eprintln!("text snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    let mut h = window();
    text_page(&mut h);
    run(&mut h, "ui.collapsePanes", json!({"side": "left", "value": true}));
    run(&mut h, "ui.collapsePanes", json!({"side": "right", "value": true}));
    run(&mut h, "edit.select", json!({"ids": ["n1"]}));
    run(&mut h, "page.fitToDrawing", json!({}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "text-canvas");

    // The Paragraph group showing a justified, bottom-aligned selection.
    run(&mut h, "edit.select", json!({"ids": ["j1"]}));
    run(&mut h, "ui.tab", json!({"tab": "Home"}));
    snap(&mut results, &mut h, "ribbon-paragraph");

    // The Text Block tool on the moved block, then the editor on the rotated shape.
    run(&mut h, "edit.select", json!({"ids": ["b1"]}));
    run(&mut h, "ui.tool", json!({"tool": "textBlock"}));
    snap(&mut results, &mut h, "text-block-tool");
    run(&mut h, "ui.tool", json!({"tool": "pointer"}));
    vizcraft_ui_egui::canvas::interact::start_editing(h.state_mut(), vizcraft_diagram::Hit::Shape("r1".into()));
    h.run_steps(4);
    snap(&mut results, &mut h, "text-edit-rotated");
}
