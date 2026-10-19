//! The Analyze and Compare groups of the Model tab, the canvas highlights and the dock's Analysis
//! tab under egui_kittest, on the flow fixture of `model_tree.rs` (Start, Do, Ok? joined by f1
//! and f2) and a looped variant (f3 from Ok? back to Do, and an island Lone).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_engine::Session;
use vizcraft_ui_egui::canvas::draw::highlights;
use vizcraft_ui_egui::dock::{DockLayout, DockTab, analysis};
use vizcraft_ui_egui::{CraftApp, Services};

fn run(a: &mut CraftApp, id: &str, p: Value) -> Value {
    a.run(id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn flow_app(looped: bool) -> CraftApp {
    let mut a = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
    run(&mut a, "element.create", json!({"type": "flowchart.Terminator", "name": "Start", "id": "start", "x": 0, "y": 0}));
    run(&mut a, "element.create", json!({"type": "flowchart.Process", "name": "Do", "id": "do", "x": 0, "y": 150}));
    run(&mut a, "element.create", json!({"type": "flowchart.Decision", "name": "Ok?", "id": "ok", "x": 0, "y": 300}));
    run(&mut a, "relationship.create", json!({"from": "start", "to": "do", "id": "f1"}));
    run(&mut a, "relationship.create", json!({"from": "do", "to": "ok", "id": "f2", "name": "next"}));
    if looped {
        run(&mut a, "relationship.create", json!({"from": "ok", "to": "do", "id": "f3"}));
        run(&mut a, "element.create", json!({"type": "flowchart.Process", "name": "Lone", "id": "lone", "x": 300, "y": 0}));
    }
    run(&mut a, "edit.selectNone", json!({}));
    a.ui.in_window_menus = true;
    a.integrated_titlebar = false;
    a.ui.tab = "Model".into();
    a
}

fn window(app: CraftApp) -> Harness<'static, CraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

fn cmd(h: &mut Harness<'static, CraftApp>, id: &str, p: Value) -> Value {
    let r = run(h.state_mut(), id, p);
    h.run_steps(3);
    r
}

/// The shape drawing `element` on the active page.
fn shape_of(h: &Harness<'static, CraftApp>, element: &str) -> String {
    let s = &h.state().session;
    s.doc.page().and_then(|p| p.shapes.iter().find(|x| x.element.as_deref() == Some(element))).map(|x| x.id.clone()).unwrap()
}

fn select(h: &mut Harness<'static, CraftApp>, elements: &[&str]) {
    let ids: Vec<String> = elements.iter().map(|e| shape_of(h, e)).collect();
    cmd(h, "edit.select", json!({"ids": ids}));
}

fn state(h: &Harness<'static, CraftApp>) -> Value {
    analysis::state(h.state())
}

fn rows_of(v: &Value, title: &str) -> u64 {
    v["sections"].as_array().unwrap().iter().find(|s| s["title"] == title).map_or(0, |s| s["rows"].as_u64().unwrap())
}

/// Click the first widget labelled `label` (the ribbon is drawn before the dock, so a ribbon
/// button wins over a legend entry of the same name).
fn click(h: &mut Harness<'static, CraftApp>, label: &str) {
    h.get_all_by_label(label).next().unwrap_or_else(|| panic!("no {label}")).click();
    h.run_steps(4);
}

/// Downstream of Start, from the ribbon.
fn downstream_of_start(h: &mut Harness<'static, CraftApp>) {
    select(h, &["start"]);
    click(h, "Downstream");
}

#[test]
fn downstream_opens_the_dock_on_analysis_and_highlights_the_result() {
    let mut h = window(flow_app(false));
    downstream_of_start(&mut h);
    let d = &h.state().ui.dock;
    assert!(d.open && d.tab == DockTab::Analysis);
    let v = state(&h);
    assert_eq!(v["kind"], "dependencies");
    assert_eq!(rows_of(&v, "Elements"), 3, "{v}");
    assert_eq!(v["highlighted"], 5, "three shapes and two connectors");
    for name in ["Start", "Do", "Ok?"] {
        assert!(h.query_all_by_label(name).next().is_some(), "{name} is a row");
    }
    // Re-run would ask about Start even after the selection moves on.
    assert_eq!(v["last"]["params"]["id"], "start");
    assert_eq!(v["last"]["params"]["direction"], "downstream");
    assert!(vizcraft_ui_egui::canvas::status_items(h.state()).iter().any(|s| s.starts_with("Analysis: 2 elements downstream")));
}

#[test]
fn depth_one_from_the_combo_cuts_the_result() {
    let mut h = window(flow_app(false));
    downstream_of_start(&mut h);
    click(&mut h, "Depth options");
    click(&mut h, "1");
    let v = state(&h);
    assert_eq!(v["options"]["depth"], 1);
    assert_eq!(rows_of(&v, "Elements"), 2, "{v}");
    assert_eq!(v["last"]["params"]["depth"], 1);
    // All goes back to the whole walk, and the option is left out of the params.
    cmd(&mut h, "ui.analysis", json!({"depth": "All"}));
    let v = state(&h);
    assert_eq!(rows_of(&v, "Elements"), 3);
    assert!(v["last"]["params"].get("depth").is_none());
}

#[test]
fn both_marks_upstream_and_downstream_with_level_chips() {
    let mut h = window(flow_app(false));
    select(&mut h, &["do"]);
    click(&mut h, "Both");
    let s = &h.state().session;
    let r = s.analysis.as_ref().unwrap();
    assert_eq!(r.marks.get("start"), Some(&vizcraft_analysis::Mark::Upstream));
    assert_eq!(r.marks.get("ok"), Some(&vizcraft_analysis::Mark::Downstream));
    let t = highlights::targets(s, s.doc.page().unwrap());
    let chip = |e: &str| t.iter().find(|x| x.id == e).and_then(|x| x.chip.clone());
    assert_eq!((chip("do"), chip("start"), chip("ok")), (Some("0".into()), Some("1".into()), Some("1".into())));
    assert!(h.query_by_label("Level 1 upstream").is_some());
    assert!(h.query_by_label("Level 1 downstream").is_some());
}

#[test]
fn cycles_list_a_group_and_go_to_selects_it() {
    let mut h = window(flow_app(true));
    click(&mut h, "Cycles");
    assert!(h.query_by_label_contains("Cycle 1").is_some());
    h.get_all_by_label("Go to").next().unwrap().click();
    h.run_steps(4);
    let s = &h.state().session;
    let page = s.doc.page().unwrap();
    let shapes = s.sel.items.iter().filter(|i| page.shape(i).is_some()).count();
    let connectors = s.sel.items.iter().filter(|i| page.connector(i).is_some()).count();
    assert_eq!((shapes, connectors), (2, 2), "{:?}", s.sel.items);
}

#[test]
fn path_runs_between_the_two_selected_shapes() {
    let mut h = window(flow_app(false));
    select(&mut h, &["start", "ok"]);
    click(&mut h, "Path");
    let v = state(&h);
    assert_eq!(v["kind"], "path");
    assert_eq!(rows_of(&v, "Steps"), 5, "three elements and the two links between them: {v}");
    let steps = h.query_all_by_label_contains("Step ").count();
    assert_eq!(steps, 3);
}

#[test]
fn a_metrics_header_click_sorts_without_running_again() {
    let mut h = window(flow_app(true));
    click(&mut h, "Metrics");
    let (rev, last) = (h.state().session.analysis.as_ref().unwrap().rev, state(&h)["last"].clone());
    let order = |h: &Harness<'static, CraftApp>| {
        let st = &h.state().ui.dock.state.analysis;
        analysis::sections(&h.state().session, st)[0].rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>()
    };
    let before = order(&h);
    click(&mut h, "Fan-in");
    let after = order(&h);
    assert_eq!(after.first().map(String::as_str), Some("do"), "Do has the most links in: {after:?}");
    assert_ne!(before, after);
    assert_eq!(state(&h)["options"]["sort"], json!({"column": 0, "descending": true}));
    assert_eq!((h.state().session.analysis.as_ref().unwrap().rev, state(&h)["last"].clone()), (rev, last));
    assert!(h.query_by_label_contains("Totals:").is_some());
}

#[test]
fn an_edit_makes_the_result_stale_and_re_run_refreshes_it() {
    let mut h = window(flow_app(false));
    downstream_of_start(&mut h);
    cmd(&mut h, "element.rename", json!({"id": "do", "name": "Done"}));
    assert_eq!(state(&h)["stale"], true);
    assert!(h.query_by_label("Out of date").is_some());
    assert!(vizcraft_ui_egui::canvas::status_items(h.state()).iter().any(|s| s.ends_with("(out of date)")));
    click(&mut h, "Re-run");
    assert_eq!(state(&h)["stale"], false);
    assert!(h.query_by_label("Out of date").is_none());
}

#[test]
fn live_refreshes_a_stale_result_within_two_frames() {
    let mut h = window(flow_app(false));
    cmd(&mut h, "ui.analysis", json!({"live": true}));
    downstream_of_start(&mut h);
    run(h.state_mut(), "element.rename", json!({"id": "ok", "name": "Okay"}));
    assert_eq!(state(&h)["stale"], true);
    h.run_steps(2);
    assert_eq!(state(&h)["stale"], false);
    // A compare is never re-run live: it reads a file.
    let dir = tempdir("live");
    let file = dir.join("flow.vizcraft");
    run(h.state_mut(), "file.saveAs", json!({"path": file.to_string_lossy()}));
    cmd(&mut h, "model.diff", json!({}));
    run(h.state_mut(), "element.rename", json!({"id": "ok", "name": "Fine"}));
    h.run_steps(2);
    assert_eq!(state(&h)["stale"], true);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn clear_empties_the_result_the_highlights_and_re_run() {
    let mut h = window(flow_app(false));
    downstream_of_start(&mut h);
    click(&mut h, "Clear Highlights");
    assert!(h.state().session.analysis.is_none());
    assert_eq!(analysis::inspect(h.state()), Value::Null);
    let v = state(&h);
    assert_eq!((v["highlighted"].clone(), v["last"].clone()), (json!(0), Value::Null));
    assert!(h.query_by_label("Run an analysis from the Model tab.").is_some());
}

#[test]
fn go_to_a_row_drawn_on_another_page_switches_page() {
    let mut app = flow_app(false);
    run(&mut app, "page.add", json!({"name": "Second"}));
    run(&mut app, "element.create", json!({"type": "flowchart.Process", "name": "Far", "id": "far", "x": 100, "y": 100}));
    run(&mut app, "page.setActive", json!({"page": 0}));
    let mut h = window(app);
    cmd(&mut h, "analyze.unreachable", json!({"from": ["start"]}));
    assert!(h.state().ui.dock.open, "a UI-origin run opens the dock");
    click(&mut h, "Far");
    let s = &h.state().session;
    let page = s.doc.page().unwrap();
    assert_eq!(page.name, "Second");
    assert_eq!(s.sel.items.len(), 1);
    assert_eq!(page.shape(&s.sel.items[0]).unwrap().element.as_deref(), Some("far"));
    assert_eq!(state(&h)["selected"], "far");
}

fn tempdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("vizcraft-analysis-ui-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn compare_with_the_saved_file_shows_added_removed_and_changed_with_a_ghost() {
    let mut h = window(flow_app(true));
    let dir = tempdir("compare");
    let file = dir.join("flow.vizcraft");
    cmd(&mut h, "file.saveAs", json!({"path": file.to_string_lossy()}));
    cmd(&mut h, "element.rename", json!({"id": "do", "name": "Done"}));
    cmd(&mut h, "model.delete", json!({"ids": ["lone"]}));
    cmd(&mut h, "element.create", json!({"type": "flowchart.Process", "name": "Fresh", "id": "fresh", "x": 300, "y": 150}));
    h.get_by_label("Compare Model options").click();
    h.run_steps(4);
    click(&mut h, "With Saved File");
    let v = state(&h);
    assert_eq!(v["kind"], "diff");
    assert_eq!((rows_of(&v, "Added"), rows_of(&v, "Removed"), rows_of(&v, "Changed")), (1, 1, 1), "{v}");
    assert_eq!(v["ghosts"], 1);
    assert!(h.query_by_label("name: Do \u{2192} Done").is_some());
    let s = &h.state().session;
    let t = highlights::targets(s, s.doc.page().unwrap());
    let mark = |e: &str| t.iter().find(|x| x.id == e).map(|x| x.mark);
    assert_eq!((mark("fresh"), mark("do")), (Some(vizcraft_analysis::Mark::Added), Some(vizcraft_analysis::Mark::Changed)));
    // The filter chips narrow the rows; Go to on the removed row stays on its page.
    cmd(&mut h, "ui.analysis", json!({"filter": "removed"}));
    let v = state(&h);
    assert_eq!((v["count"].clone(), rows_of(&v, "Added")), (json!(1), 0));
    click(&mut h, "Lone");
    assert!(h.state().session.sel.items.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn compare_with_a_file_needs_a_picker() {
    let mut h = window(flow_app(false));
    let r = cmd(&mut h, "ui.compareFile", json!({}));
    assert_eq!(r, json!({"picked": false}));
    assert!(h.state().session.analysis.is_none());
    // With a picker the chosen file is compared.
    let dir = tempdir("pick");
    let file = dir.join("flow.vizcraft");
    cmd(&mut h, "file.saveAs", json!({"path": file.to_string_lossy()}));
    let path = file.to_string_lossy().into_owned();
    h.state_mut().services.pick_open = Some(Box::new(move |_| Some(path.clone())));
    let r = cmd(&mut h, "ui.compareFile", json!({}));
    assert_eq!(r["picked"], true);
    assert_eq!(h.state().session.analysis.as_ref().unwrap().kind, "diff");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn nothing_here_makes_an_undo_step_or_dirties_the_document() {
    let mut h = window(flow_app(true));
    let dir = tempdir("clean");
    cmd(&mut h, "file.saveAs", json!({"path": dir.join("flow.vizcraft").to_string_lossy()}));
    let steps = h.state().session.transactions().len();
    assert!(!h.state().session.dirty);
    downstream_of_start(&mut h);
    click(&mut h, "Depth options");
    click(&mut h, "2");
    click(&mut h, "Upstream");
    click(&mut h, "Cycles");
    h.get_all_by_label("Go to").next().unwrap().click();
    h.run_steps(4);
    click(&mut h, "Single Points of Failure");
    click(&mut h, "Unreachable");
    click(&mut h, "Metrics");
    click(&mut h, "Fan-out");
    select(&mut h, &["start", "ok"]);
    click(&mut h, "Path");
    click(&mut h, "Do");
    cmd(&mut h, "analyze.select", json!({}));
    click(&mut h, "Compare Model");
    cmd(&mut h, "ui.analysis", json!({"live": true, "dim": false, "types": ["flowchart.flow"], "containment": true}));
    click(&mut h, "Clear Highlights");
    cmd(&mut h, "ui.compareFile", json!({}));
    let s = &h.state().session;
    assert_eq!(s.transactions().len(), steps, "{:?}", s.transactions().back().map(|t| t.label.clone()));
    assert!(!s.dirty);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn junk_options_and_a_result_whose_ids_were_deleted_never_panic() {
    let mut h = window(flow_app(true));
    let junk = [
        json!(null),
        json!([]),
        json!({"depth": -5, "types": "x", "containment": 3, "dim": "no", "live": [], "filter": 7, "row": {}, "goto": "yes"}),
        json!({"depth": 1e300, "types": [1, null, "", "x".repeat(500)], "filter": "nope", "row": "x".repeat(5000), "goto": true}),
        json!({"depth": "lots", "row": "nothing", "goto": true}),
        json!({"depth": f64::MAX, "types": vec!["a.b"; 500]}),
    ];
    for j in &junk {
        h.state_mut().run("ui.analysis", j.clone()).unwrap();
        h.run_steps(2);
    }
    assert_eq!(state(&h)["options"]["depth"], 1024, "clamped");
    assert!(state(&h)["options"]["types"].as_array().unwrap().len() <= 64);
    select(&mut h, &["start"]);
    click(&mut h, "Downstream");
    cmd(&mut h, "model.delete", json!({"ids": ["do", "ok"]}));
    h.run_steps(3);
    let s = &h.state().session;
    let t = highlights::targets(s, s.doc.page().unwrap());
    assert!(t.iter().all(|x| s.doc.model.contains_id(&x.id)));
    cmd(&mut h, "ui.analysis", json!({"row": "do", "goto": true}));
    click(&mut h, "Re-run");
}

#[test]
fn a_saved_layout_on_the_analysis_tab_restores_it() {
    let l: DockLayout = serde_json::from_value(json!({"open": true, "height": 300, "tab": "analysis"})).unwrap();
    assert_eq!(l.tab, DockTab::Analysis);
    assert_eq!(serde_json::to_value(&l).unwrap()["tab"], "analysis");
    assert_eq!(DockTab::parse("Analysis"), Some(DockTab::Analysis));
    let mut h = window(flow_app(false));
    let r = cmd(&mut h, "ui.dock", json!({"tab": "analysis"}));
    assert_eq!(r["tab"], "analysis");
    assert!(h.query_by_label("Run an analysis from the Model tab.").is_some());
}

#[test]
fn targets_skip_ids_the_model_lost() {
    let mut a = flow_app(false);
    run(&mut a, "analyze.dependencies", json!({"id": "start"}));
    let mut r = a.session.analysis.clone().unwrap();
    r.marks.insert("gone".into(), vizcraft_analysis::Mark::Cycle);
    a.session.analysis = Some(r);
    // A shape still says it draws the lost element.
    if let Some(p) = a.session.doc.page_mut()
        && let Some(s) = p.shapes.first_mut()
    {
        s.element = Some("gone".into());
    }
    let t = highlights::targets(&a.session, a.session.doc.page().unwrap());
    assert!(t.iter().all(|x| x.id != "gone"));
    assert_eq!(t.len(), 4, "the other two shapes and both connectors");
}
