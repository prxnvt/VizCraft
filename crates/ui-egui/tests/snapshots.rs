//! Pixel snapshots of the whole window, rendered offscreen with `egui_kittest` and wgpu and
//! compared with the PNGs in `tests/snapshots/`. They catch layout and theme regressions the
//! state-based tests cannot see.
//!
//! The test needs a GPU (or a software rasterizer wgpu can use). When no wgpu adapter exists, or
//! `VIZCRAFT_SKIP_SNAPSHOTS` is set, it prints a note and passes, so `cargo xtask ci` stays
//! green on machines without one. `UPDATE_SNAPSHOTS=1 cargo test -p vizcraft-ui-egui --test snapshots`
//! rewrites the PNGs that changed (`docs/testing.md`, Snapshot tests).
//!
//! Shortcut text differs by platform (Cmd glyphs on Apple macOS, `Ctrl+` elsewhere), so the
//! references live in `tests/snapshots/<os>/`. An OS without a reference folder is skipped until
//! someone records one with `UPDATE_SNAPSHOTS=1` on that OS.
//!
//! Every snapshot starts the window the same way on every platform: the in-window menu bar is
//! drawn, the title bar has no traffic-light gap, the sample document is open.

use egui_kittest::kittest::Queryable;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 800.0;

/// Where this platform's references live, relative to the crate root (the test's working directory).
fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

/// Whether wgpu can render here. Probing creates one renderer; `WgpuTestRenderer::new` panics
/// without an adapter, so the panic is caught and silenced (this is the only test in the binary,
/// so swapping the hook races with nothing).
fn wgpu_available() -> bool {
    if std::env::var_os("VIZCRAFT_SKIP_SNAPSHOTS").is_some() {
        eprintln!("snapshots: skipped (VIZCRAFT_SKIP_SNAPSHOTS is set)");
        return false;
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let probe = std::panic::catch_unwind(WgpuTestRenderer::new);
    std::panic::set_hook(hook);
    if probe.is_err() {
        eprintln!("snapshots: skipped (no wgpu adapter on this machine)");
        return false;
    }
    if !std::path::Path::new(&output_dir()).is_dir() && std::env::var_os("UPDATE_SNAPSHOTS").is_none() {
        eprintln!("snapshots: skipped (no references in {} yet; record them with UPDATE_SNAPSHOTS=1)", output_dir());
        return false;
    }
    true
}

/// The window at 1280x800 with the sample document, laid out the way the host does it.
fn window(dark: bool) -> Harness<'static, CraftApp> {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app.session.view.dark_mode = dark;
    let mut h = Harness::builder().with_size(egui::vec2(WIDTH, HEIGHT)).with_pixels_per_point(1.0).wgpu().build_ui_state(
        |ui, app: &mut CraftApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.input_mut().max_texture_side = Some(8192);
    // Fonts install on the first frame and the app skips drawing for two more; settle first.
    h.run_steps(6);
    h
}

/// Run a UI command and let the result be drawn.
fn run(h: &mut Harness<'static, CraftApp>, id: &str, params: serde_json::Value) {
    h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
}

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

#[test]
fn window_snapshots() {
    if !wgpu_available() {
        return;
    }
    let mut results = SnapshotResults::new();

    // The whole window, Home tab, in both themes.
    let mut h = window(false);
    snap(&mut results, &mut h, "home-light");
    let mut h = window(true);
    snap(&mut results, &mut h, "home-dark");

    // The View tab's ribbon.
    let mut h = window(false);
    run(&mut h, "ui.tab", json!({"tab": "View"}));
    snap(&mut results, &mut h, "ribbon-view");

    // The Process tab's ribbon.
    let mut h = window(false);
    run(&mut h, "ui.tab", json!({"tab": "Process"}));
    snap(&mut results, &mut h, "ribbon-process");

    // The in-window Edit menu open.
    let mut h = window(false);
    h.get_by_label("Edit").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "menu-edit");

    // The collapsed ribbon showing the Home tab as a popup over the document.
    let mut h = window(false);
    run(&mut h, "ui.collapseRibbon", json!({"value": true}));
    h.get_by_label("Home tab").click();
    h.run_steps(3);
    snap(&mut results, &mut h, "ribbon-collapsed-popup");

    // Both pane sides folded to their strips of titles.
    let mut h = window(false);
    run(&mut h, "ui.collapsePanes", json!({"side": "left", "value": true}));
    run(&mut h, "ui.collapsePanes", json!({"side": "right", "value": true}));
    snap(&mut results, &mut h, "panes-folded");

    // The About dialog.
    let mut h = window(false);
    run(&mut h, "ui.dialog", json!({"name": "about"}));
    snap(&mut results, &mut h, "dialog-about");

    // The page tab strip with two pages, the second active, on the Design tab's ribbon.
    let mut h = window(false);
    run(&mut h, "page.add", json!({"name": "Details"}));
    run(&mut h, "ui.tab", json!({"tab": "Design"}));
    snap(&mut results, &mut h, "page-tabs");

    // The Page Setup dialog, opened the way the ribbon's launcher opens it.
    let mut h = window(false);
    run(&mut h, "page.setup", json!({}));
    snap(&mut results, &mut h, "dialog-page-setup");

    // The command palette.
    let mut h = window(true);
    run(&mut h, "ui.commands", json!({}));
    snap(&mut results, &mut h, "palette");

    // The Layers pane: one layer hidden, one locked and tinted, shapes on both.
    let mut h = window(false);
    let ids: Vec<String> = h.state().session.doc.page().map(|p| p.shapes.iter().map(|s| s.id.clone()).collect()).unwrap_or_default();
    run(&mut h, "layer.add", json!({"name": "Notes"}));
    run(&mut h, "layer.add", json!({"name": "Process"}));
    run(&mut h, "layer.assign", json!({"ids": ids.get(..1), "layer": "Notes"}));
    run(&mut h, "layer.assign", json!({"ids": ids.get(1..3), "layer": "Process"}));
    run(&mut h, "layer.set", json!({"layer": "Notes", "visible": false}));
    run(&mut h, "layer.set", json!({"layer": "Process", "locked": true, "color": "#c4452f"}));
    run(&mut h, "ui.pane", json!({"name": "history", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "properties", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "layers", "value": true}));
    snap(&mut results, &mut h, "pane-layers");
}
