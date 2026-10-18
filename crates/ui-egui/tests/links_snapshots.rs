//! Pixel snapshots of links and review: the Comments pane with a thread, the Review and Process
//! tabs at 1280 px, and the Link dialog. Same harness and rules as `snapshots.rs` (needs a wgpu
//! adapter, skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with
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
fn link_and_review_snapshots() {
    if !wgpu_available() {
        eprintln!("link snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    let mut h = window();
    run(&mut h, "ui.tab", json!({"tab": "Review"}));
    snap(&mut results, &mut h, "tab-review");

    let mut h = window();
    run(&mut h, "ui.tab", json!({"tab": "Process"}));
    snap(&mut results, &mut h, "tab-process");

    // The Comments pane: a comment on a shape with a reply, and a second one on the page. The
    // time stamps are fixed so the picture does not change from run to run.
    let mut h = window();
    let id = h.state().session.doc.page().unwrap().shapes[1].id.clone();
    run(&mut h, "edit.select", json!({"ids": [id]}));
    let c = h.state_mut().run("comment.add", json!({"text": "Should this step ask for a name as well?"})).unwrap()["id"].as_str().unwrap().to_string();
    run(&mut h, "comment.reply", json!({"id": c, "text": "Yes, the form needs both."}));
    run(&mut h, "comment.add", json!({"text": "Move the No branch below the decision.", "shape": null}));
    for c in &mut h.state_mut().session.doc.comments {
        c.time = "2026-10-18T09:30:00Z".into();
        c.author = "Reviewer".into();
        for r in &mut c.replies {
            r.time = "2026-10-18T10:05:00Z".into();
            r.author = "Author".into();
        }
    }
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "properties", "value": false}));
    run(&mut h, "ui.tab", json!({"tab": "Review"}));
    run(&mut h, "ui.pane.comments", json!({"value": true}));
    snap(&mut results, &mut h, "pane-comments");

    // The Link dialog on a shape with a web link and a screen tip.
    let mut h = window();
    let id = h.state().session.doc.page().unwrap().shapes[0].id.clone();
    run(&mut h, "link.add", json!({"id": id, "kind": "url", "target": "https://example.com/spec", "label": "Specification"}));
    run(&mut h, "shape.setScreenTip", json!({"ids": [id], "text": "Where the flow begins"}));
    run(&mut h, "edit.select", json!({"ids": [id]}));
    run(&mut h, "link.add", json!({}));
    snap(&mut results, &mut h, "dialog-links");
}
