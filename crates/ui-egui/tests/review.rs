//! The Review tab, the Spelling dialog and the Info page: ribbon wiring, the language menu's
//! check marks, the dialog opened from the ribbon command, and pixel snapshots of the Review tab,
//! the Spelling dialog and the Info page. Snapshots follow `snapshots.rs` (need a wgpu adapter,
//! skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui::vec2;
use egui_kittest::kittest::Queryable as _;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::ribbon::{self, Overflow};
use vizcraft_ui_egui::{CraftApp, Services, info_page, widgets};

fn sample_app() -> CraftApp {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    app
}

/// The sample with a misspelling planted in the first shape.
fn planted_app() -> CraftApp {
    let mut app = sample_app();
    let id = app.session.doc.pages[0].shapes[0].id.clone();
    app.run("shape.setText", json!({"id": id, "text": "Recieve teh order"})).unwrap();
    app
}

fn window(app: CraftApp, width: f32, height: f32) -> Harness<'static, CraftApp> {
    let mut h = Harness::builder().with_size(vec2(width, height)).with_pixels_per_point(1.0).wgpu().build_ui_state(
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

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

#[test]
fn review_tab_has_proofing_language_and_reporting_groups_that_fit_1280() {
    let app = sample_app();
    let tabs = ribbon::tabs(&app);
    let review = tabs.iter().find(|t| t.name == "Review").expect("the Review tab shows now that it has groups");
    let labels: Vec<&str> = review.groups.iter().map(|g| g.label).collect();
    assert_eq!(labels, ["Comments", "Ink", "Proofing", "Language", "Reporting"]);
    for id in review.ids() {
        assert!(widgets::exists(&app, id), "{id}");
    }
    let mut app = sample_app();
    app.ui.tab = "Review".into();
    let mut h = egui_kittest::Harness::builder().with_size(vec2(1280.0, 700.0)).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    assert!(!Overflow::get(&h.ctx).overflows(), "Review overflows at 1280");
}

#[test]
fn the_language_menu_checks_the_language_in_force() {
    let mut app = sample_app();
    let tabs = ribbon::tabs(&app);
    let review = tabs.iter().find(|t| t.name == "Review").unwrap();
    let ribbon::Control::Menu { menu, .. } = &review.groups[3].controls[0] else { panic!("the Language group holds a menu") };
    let on = |app: &CraftApp| -> Vec<&'static str> { menu.iter().filter(|e| e.checked.is_some_and(|f| f(app))).map(|e| e.label).collect() };
    assert_eq!(on(&app), ["English (United States)"]);
    let id = app.session.doc.pages[0].shapes[0].id.clone();
    app.run("edit.select", json!({"ids": [id]})).unwrap();
    app.run("proof.setLanguage", json!({"language": "fr-FR"})).unwrap();
    assert_eq!(on(&app), ["French (France)"]);
    app.run("proof.setLanguage", json!({"language": "none"})).unwrap();
    assert_eq!(on(&app), ["Do Not Check Spelling"]);
    app.run("proof.setLanguage", json!({"language": "default"})).unwrap();
    assert_eq!(on(&app), ["English (United States)", "Use the Document Language"]);
}

#[test]
fn the_spelling_command_opens_a_dialog_on_the_planted_word() {
    let mut app = planted_app();
    app.run("ui.spelling", json!({})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(6);
    assert!(h.query_by_label("Not in Dictionary (1 of 2)").is_some());
    assert!(h.query_by_label("Recieve").is_some());
    h.get_by_label("Change").click();
    h.run_steps(4);
    assert_eq!(h.state().session.doc.pages[0].shapes[0].text, "Receive teh order");
}

#[test]
fn info_page_edits_properties_and_inspects_hidden_data() {
    let mut app = sample_app();
    app.run("doc.setProperties", json!({"author": "Pat"})).unwrap();
    let mut h = Harness::builder().with_size(vec2(900.0, 900.0)).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            // The fonts installed by the first pass are bound from the second on.
            if ui.ctx().cumulative_pass_nr() > 2 {
                egui::CentralPanel::default().show(ui, |ui| info_page::show(app, ui));
            }
        },
        app,
    );
    h.run_steps(5);
    assert!(h.query_by_label("Statistics").is_some());
    h.get_by_label("Inspect Document").click();
    h.run_steps(4);
    assert!(h.query_by_label("Document properties").is_some(), "the inspection lists the kinds");
    assert!(h.query_by_label("Remove All Selected").is_some());
}

#[test]
fn f7_opens_the_spelling_dialog() {
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        planted_app(),
    );
    h.run_steps(3);
    assert!(h.query_by_label("Not in Dictionary (1 of 2)").is_none());
    h.key_press(egui::Key::F7);
    h.run_steps(6);
    assert!(h.query_by_label("Not in Dictionary (1 of 2)").is_some());
}

#[test]
fn review_snapshots() {
    if !wgpu_available() {
        eprintln!("review snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    let mut h = window(sample_app(), 1280.0, 800.0);
    run(&mut h, "ui.tab", json!({"tab": "Review"}));
    snap(&mut results, &mut h, "ribbon-review");

    let mut h = window(planted_app(), 1280.0, 800.0);
    run(&mut h, "ui.spelling", json!({}));
    snap(&mut results, &mut h, "dialog-spelling");

    // The Info page on its own, as the File page shows it, with an inspection run.
    let mut app = sample_app();
    app.run("doc.setProperties", json!({"title": "Plant overview", "author": "Pat Example", "subject": "Process flow", "keywords": "plant; flow"}))
        .unwrap();
    let id = app.session.doc.pages[0].shapes[0].id.clone();
    app.run("shape.setText", json!({"id": id, "text": "Start"})).unwrap();
    // Some hidden data to report: a screen tip, a hidden shape and a comment.
    let page = app.session.doc.page_mut().unwrap();
    page.shapes[0].screen_tip = "Where the flow begins".into();
    page.shapes[1].behavior.hidden = true;
    app.session.doc.comments.push(vizcraft_diagram::Comment { id: "m1".into(), text: "Check this".into(), ..Default::default() });
    let mut h = Harness::builder().with_size(vec2(760.0, 960.0)).with_pixels_per_point(1.0).wgpu().build_ui_state(
        |ui, app: &mut CraftApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            if ctx.cumulative_pass_nr() > 2 {
                let t = vizcraft_ui_egui::theme::Tokens::get(&ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(36, 28)))
                    .show(ui, |ui| info_page::show(app, ui));
            }
        },
        app,
    );
    h.input_mut().max_texture_side = Some(8192);
    h.run_steps(6);
    h.get_by_label("Inspect Document").click();
    h.run_steps(4);
    snap(&mut results, &mut h, "info-page");
}
