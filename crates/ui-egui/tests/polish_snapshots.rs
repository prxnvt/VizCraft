//! The canvas drawing follow-ups of Block 6: ER tables (one header row, aligned columns), Chen
//! diamonds and Barker halves, step badges clear of labels, names under actor figures. Same
//! harness and rules as `glyphs_snapshots.rs` (needs a wgpu adapter, skipped without one;
//! refresh with `UPDATE_SNAPSHOTS=1`). The state checks run without one.

use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::{Value, json};
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::{CraftApp, Services};

fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

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

fn window(template: &str) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(Document::new()), Services::default());
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
    run(&mut h, "template.new", json!({"template": template}));
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "properties", "value": false}));
    run(&mut h, "edit.selectNone", json!({}));
    run(&mut h, "view.zoomFit", json!({}));
    h
}

fn run(h: &mut Harness<'static, CraftApp>, id: &str, params: Value) -> Value {
    let r = h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
    r
}

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

#[test]
fn er_tables_chen_and_barker_snapshots() {
    if !wgpu_available() {
        eprintln!("polish snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();
    let mut h = window("er.schema");
    snap(&mut results, &mut h, "canvas-er-schema");
    let mut h = window("er.conceptual");
    run(&mut h, "variant.set", json!({"variant": "er.chen"}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "canvas-er-chen");
    let mut h = window("er.schema");
    run(&mut h, "variant.set", json!({"variant": "er.barker"}));
    run(&mut h, "view.zoomFit", json!({}));
    snap(&mut results, &mut h, "canvas-er-barker");
}

#[test]
fn canvas_draws_step_badges() {
    let mut s = Session::new(Document::new());
    s.run("template.new", &json!({"template": "cloud.topology"})).unwrap();
    let page = s.doc.page().unwrap();
    let stepped: Vec<_> = page.connectors.iter().filter(|c| c.step > 0).collect();
    assert!(!stepped.is_empty(), "the template numbers some connectors");
    for c in stepped {
        let g = vizcraft_diagram::connector_geometry(page, c);
        let b = vizcraft_diagram::step_badge::badge(c, &g.polyline).expect("a stepped connector has a badge");
        if !c.text.trim().is_empty() {
            let (w, h) = vizcraft_diagram::connector_look::label_size(&c.text, &c.style);
            let (dx, dy) = ((b.center.x - g.label_pos.x).abs() - w / 2.0, (b.center.y - g.label_pos.y).abs() - h / 2.0);
            assert!(dx.max(0.0).hypot(dy.max(0.0)) >= b.radius - 0.5, "{}: badge touches its label", c.id);
        }
    }
    if !wgpu_available() {
        return;
    }
    let mut results = SnapshotResults::new();
    let mut h = window("cloud.topology");
    snap(&mut results, &mut h, "canvas-step-badge");
}

#[test]
fn actor_name_sits_below_the_figure() {
    let mut s = Session::new(Document::new());
    s.run("template.new", &json!({"template": "uml.useCase"})).unwrap();
    let actor = s.doc.page().unwrap().shapes.iter().find(|x| x.stencil == "uml.actor").expect("the template has an actor").clone();
    assert_eq!(actor.text_place, vizcraft_diagram::shape_fields::TextPlace::Below);
    assert!(actor.text_rect().0.y0 >= actor.h, "the name starts under the figure");
    if !wgpu_available() {
        return;
    }
    let mut results = SnapshotResults::new();
    let mut h = window("uml.useCase");
    snap(&mut results, &mut h, "canvas-actor-label");
}
