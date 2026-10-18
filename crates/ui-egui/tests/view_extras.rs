//! View tab extras: presentation mode, the Pan and Zoom pane's commands, window commands, the
//! Macros group and the View tab fitting 1280 points. The pane's drag tests are in
//! `panes/pan_zoom.rs`, the window arrangement tests in `apps/vizcraft/src/windows.rs`.

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::ribbon::Overflow;
use vizcraft_ui_egui::{CraftApp, Services, WindowRequest};

fn app() -> CraftApp {
    CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default())
}

fn harness_with(app: CraftApp, width: f32) -> Harness<'static, CraftApp> {
    let mut h = Harness::builder().with_size(vec2(width, 800.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(6);
    h
}

#[test]
fn the_view_tab_fits_1280_wide() {
    let mut a = app();
    a.ui.tab = "View".into();
    a.ui.panes.close("history");
    let h = harness_with(a, 1280.0);
    let o = Overflow::get(&h.ctx);
    assert!(!o.overflows(), "the View tab overflows at 1280: {o:?}");
    for label in ["Page Width", "Presentation", "New Window", "Arrange All", "Cascade", "Record Macro", "Save Macro"] {
        assert!(h.query_by_label(label).is_some(), "{label} is missing from the View tab");
    }
}

#[test]
fn task_panes_lists_every_pane_with_its_check_mark() {
    let mut a = app();
    a.ui.tab = "View".into();
    let mut h = harness_with(a, 1280.0);
    h.get_by_label("Task Panes").click();
    h.run_steps(4);
    let menu = vizcraft_ui_egui::widgets::open_menu(&h.ctx).expect("the menu is open");
    let rows: Vec<(String, bool)> =
        menu["rows"].as_array().unwrap().iter().map(|r| (r["label"].as_str().unwrap().to_string(), r["checked"] == json!(true))).collect();
    assert!(rows.contains(&("Pan and Zoom".to_string(), false)), "{rows:?}");
    assert!(rows.contains(&("Palette".to_string(), true)), "{rows:?}");
    assert_eq!(rows.len(), vizcraft_ui_egui::panes::panes().len());
}

#[test]
fn presentation_hides_the_chrome_pages_with_keys_and_escape_leaves() {
    let mut a = app();
    a.run("page.add", json!({"name": "Second"})).unwrap();
    a.run("page.setActive", json!({"page": 0})).unwrap();
    let mut h = harness_with(a, 1280.0);
    let drawn = |h: &Harness<'static, CraftApp>, label: &str| h.query_all_by_label_contains(label).count() > 0;
    assert!(drawn(&h, "Home") && drawn(&h, "Insert"), "the ribbon shows normally");
    assert!(drawn(&h, "Drawing"), "and so do the panes (the Palette's Drawing group)");
    assert!(drawn(&h, "Ready"), "and the status bar");
    let zoom = h.state().session.view.zoom;
    let pan = h.state().session.view.pan;
    h.state_mut().run("ui.present", json!({})).unwrap();
    h.run_steps(4);
    assert!(h.state().presenting);
    for label in ["Home", "Insert", "Drawing", "Ready"] {
        assert!(!drawn(&h, label), "{label} is still drawn in presentation mode");
    }
    assert!(h.state().session.view.canvas.is_some(), "the canvas is still drawn");
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(h.state().session.doc.active_index(), 1, "Right shows the next page");
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(h.state().session.doc.active_index(), 1, "and stops at the last");
    h.key_press(egui::Key::PageUp);
    h.run_steps(3);
    assert_eq!(h.state().session.doc.active_index(), 0, "PageUp goes back");
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert!(!h.state().presenting, "Escape leaves");
    assert!(drawn(&h, "Home"), "the ribbon is back");
    assert_eq!(h.state().session.view.zoom, zoom, "the view is put back as it was");
    assert_eq!(h.state().session.view.pan, pan);
}

#[test]
fn presentation_asks_for_full_screen_and_toggles_through_the_command() {
    let mut a = app();
    assert_eq!(a.run("ui.present", json!({"on": true})).unwrap()["presenting"], true);
    assert_eq!(a.run("ui.present", json!({})).unwrap()["presenting"], false, "no value toggles");
    assert_eq!(a.run("ui.present", json!({"on": "junk"})).unwrap()["presenting"], true, "junk toggles too");
    let mut h = harness_with(app(), 1280.0);
    h.state_mut().run("ui.present", json!({})).unwrap();
    let asked = |h: &Harness<'static, CraftApp>, on: bool| {
        h.output().viewport_output.values().flat_map(|v| v.commands.iter()).any(|c| matches!(c, egui::ViewportCommand::Fullscreen(x) if *x == on))
    };
    let mut entered = false;
    for _ in 0..4 {
        h.step();
        entered |= asked(&h, true);
    }
    assert!(entered, "entering asks for full screen");
    h.state_mut().run("ui.present", json!({"on": false})).unwrap();
    let mut left = false;
    for _ in 0..4 {
        h.step();
        left |= asked(&h, false);
    }
    assert!(left, "leaving asks for the window back");
    assert!(!h.state().presenting);
}

#[test]
fn window_commands_become_requests_only_with_a_multi_window_host() {
    let mut a = app();
    assert_eq!(a.run("ui.window.cascade", json!({})).unwrap()["requested"], false);
    assert!(a.window_request.is_none());
    a.multi_window = true;
    for (id, params, want) in [
        ("ui.window.new", json!({"same": true}), WindowRequest::NewSame),
        ("ui.window.arrangeAll", json!({}), WindowRequest::ArrangeAll),
        ("ui.window.cascade", json!({}), WindowRequest::Cascade),
        ("ui.window.switch", json!({"index": 2}), WindowRequest::Switch(2)),
    ] {
        assert_eq!(a.run(id, params).unwrap()["requested"], true, "{id}");
        assert_eq!(a.window_request.take(), Some(want), "{id}");
    }
    // Without `same` a new window is the ordinary blank one.
    a.run("ui.window.new", json!({})).unwrap();
    assert_eq!(a.new_window_request.take().as_deref(), Some("blank"));
    // Junk indexes are refused without panicking.
    for junk in [json!({"index": -1}), json!({"index": "x"}), json!({"index": 1e30}), json!({})] {
        assert_eq!(a.run("ui.window.switch", junk).unwrap()["requested"], false);
    }
    assert!(a.window_request.is_none());
}

#[test]
fn recording_saving_running_and_deleting_a_macro_through_the_app() {
    let dir = std::env::temp_dir().join(format!("vizcraft-view-extras-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    vizcraft_engine::cmd::r#macro::set_dir(Some(dir.clone()));
    if std::env::var_os(vizcraft_engine::cmd::r#macro::DIR_ENV).is_some() {
        return;
    }
    let mut a = app();
    assert_eq!(a.run("ui.macroRecord", json!({})).unwrap()["recording"], true);
    a.run("shape.add", json!({"stencil": "basic.rectangle", "id": "rec1", "x": 5, "y": 5, "w": 30, "h": 20})).unwrap();
    let stopped = a.run("ui.macroRecord", json!({})).unwrap();
    assert_eq!(stopped["recording"], false);
    assert_eq!(stopped["kept"], 1);
    // Save Macro: the dialog names the script just stopped.
    a.run("ui.dialog", json!({"name": "saveMacro"})).unwrap();
    let state = vizcraft_ui_egui::addon_dialog::State::SaveMacro { name: "Add Rec".into() };
    assert!(vizcraft_ui_egui::addon_dialog::finish(&mut a, &state, true, false));
    assert!(a.last_script.is_none(), "saved, so no longer held");
    assert_eq!(vizcraft_engine::cmd::r#macro::names(), ["Add Rec"]);
    a.run("edit.undo", json!({})).unwrap();
    a.run("macro.run", json!({"name": "Add Rec"})).unwrap();
    assert!(a.session.doc.page().unwrap().shape("rec1").is_some());
    a.run("macro.delete", json!({"name": "Add Rec"})).unwrap();
    assert!(vizcraft_engine::cmd::r#macro::names().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    vizcraft_engine::cmd::r#macro::set_dir(None);
}
