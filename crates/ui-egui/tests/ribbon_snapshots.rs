//! Pixel snapshots of the organised ribbon at 1280 points wide: the Design, Connector (with a
//! connector selected) and Insert (with a shape selected) tabs (Process is `ribbon-process` in
//! `snapshots.rs`), and the new dropdowns
//! open on their current choice. Home at this width is `home-light` in `snapshots.rs`. Same
//! harness and rules as `menu_snapshots.rs` (needs a wgpu adapter, skipped without one or with
//! `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui_kittest::kittest::Queryable;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::ribbon::Overflow;
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

/// The window on `tab`, 1280 wide, `height` tall (a ribbon alone, or room for an open dropdown).
fn window(tab: &str, height: f32) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app.ui.tab = tab.into();
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, height)).with_pixels_per_point(1.0).wgpu().build_ui_state(
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

fn first_connector(h: &Harness<'static, CraftApp>) -> String {
    h.state().session.doc.page().and_then(|p| p.connectors.first()).map(|c| c.id.clone()).unwrap_or_default()
}

fn first_shape(h: &Harness<'static, CraftApp>) -> String {
    h.state().session.doc.page().and_then(|p| p.shapes.first()).map(|s| s.id.clone()).unwrap_or_default()
}

#[test]
fn ribbon_tab_snapshots() {
    if !wgpu_available() {
        eprintln!("ribbon snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // Design: Page Setup as small menus, the Connectors group with the routing policy.
    let mut h = window("Design", 230.0);
    assert!(!Overflow::get(&h.ctx).overflows());
    snap(&mut results, &mut h, "tab-design");

    // Connector: Kind, Route, Arrows and Style with a connector selected.
    let mut h = window("Connector", 230.0);
    let c = first_connector(&h);
    run(&mut h, "edit.select", json!({"ids": [c]}));
    run(&mut h, "ui.tab", json!({"tab": "Connector"}));
    assert!(!Overflow::get(&h.ctx).overflows());
    snap(&mut results, &mut h, "tab-connector");

    // Insert with one shape selected: the Compartment menu is on.
    let mut h = window("Insert", 230.0);
    let s = first_shape(&h);
    run(&mut h, "edit.select", json!({"ids": [s]}));
    snap(&mut results, &mut h, "tab-insert");

    // The Kind menu on the Connector tab, the selected connector drawn as a bus.
    let mut h = window("Connector", 560.0);
    let c = first_connector(&h);
    run(&mut h, "edit.select", json!({"ids": [c.clone()]}));
    run(&mut h, "connector.setKind", json!({"ids": [c], "kind": "basic.bus"}));
    run(&mut h, "ui.tab", json!({"tab": "Connector"}));
    h.get_by_label("Kind").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "menu-connector-kind");

    // The End Arrow menu with the crow's foot with bar chosen.
    let mut h = window("Connector", 440.0);
    let c = first_connector(&h);
    run(&mut h, "edit.select", json!({"ids": [c.clone()]}));
    run(&mut h, "connector.setArrows", json!({"ids": [c], "end": "crowsFootBar"}));
    run(&mut h, "ui.tab", json!({"tab": "Connector"}));
    h.get_by_label("End Arrow").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "menu-end-arrow");

    // The Rules menu with the profile holding one ruleset.
    let mut h = window("Process", 300.0);
    run(&mut h, "validate.setProfile", json!({"rulesets": ["drawing.tidy"]}));
    h.get_by_label("Rules").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "menu-rules");

    // The Board menu on a fixed board.
    let mut h = window("Design", 230.0);
    run(&mut h, "page.setup", json!({"board": "fixed"}));
    h.get_by_label("Board").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "menu-board");
}
