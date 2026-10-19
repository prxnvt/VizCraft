//! The Model Tree pane under egui_kittest: the Model branch grouped by type, the Diagrams
//! branch, click to select (switching page), Context mode following the selection, inline
//! rename, `ui.tree` and the `tree` field of `ui.inspect`. The pane is drawn alone in a narrow
//! panel so queries only see its rows.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services, panes};

fn flow_app() -> CraftApp {
    let mut a = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
    let run = |a: &mut CraftApp, id: &str, p: Value| a.run(id, p).unwrap_or_else(|e| panic!("{id}: {e}"));
    run(&mut a, "element.create", json!({"type": "flowchart.Terminator", "name": "Start", "id": "start", "x": 0, "y": 0}));
    run(&mut a, "element.create", json!({"type": "flowchart.Process", "name": "Do", "id": "do", "x": 0, "y": 150}));
    run(&mut a, "element.create", json!({"type": "flowchart.Decision", "name": "Ok?", "id": "ok", "x": 0, "y": 300}));
    run(&mut a, "relationship.create", json!({"from": "start", "to": "do", "id": "f1"}));
    run(&mut a, "page.add", json!({"name": "Second"}));
    run(&mut a, "page.setActive", json!({"page": 0}));
    run(&mut a, "element.show", json!({"id": "ok", "page": "Second", "x": 50, "y": 50}));
    run(&mut a, "edit.selectNone", json!({}));
    a
}

fn pane(app: CraftApp) -> Harness<'static, CraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(300.0, 520.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            let spec = panes::spec("navigation").expect("the pane exists");
            (spec.show)(app, ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn tree(h: &mut Harness<'static, CraftApp>, p: Value) -> Value {
    let r = h.state_mut().run("ui.tree", p).unwrap();
    h.run_steps(3);
    r
}

#[test]
fn the_pane_is_the_model_tree_under_the_old_key() {
    let spec = panes::spec("navigation").unwrap();
    assert_eq!(spec.title, "Model Tree");
    assert!(panes::spec("modelTree").is_none());
}

#[test]
fn model_branch_lists_elements_grouped_by_type() {
    let mut h = pane(flow_app());
    tree(&mut h, json!({"collapse": ["branch:diagrams"]}));
    for label in ["Model (3)", "Terminator (1)", "Process (1)", "Decision (1)", "Start", "Do", "Ok?"] {
        assert!(h.query_by_label(label).is_some(), "{label} is listed");
    }
    // The pages an element is drawn on show beside it.
    assert!(h.query_by_label_contains("on Page-1, Second").is_some(), "Ok? is on both pages");
}

#[test]
fn diagrams_branch_lists_pages_and_their_shapes() {
    let mut h = pane(flow_app());
    tree(&mut h, json!({"branch": "diagrams"}));
    assert!(h.query_by_label("Diagrams (2)").is_some());
    assert!(h.query_by_label("Page-1").is_some() && h.query_by_label("Second").is_some());
    assert!(h.query_by_label("Start").is_some(), "shapes are listed by text under their page");
    assert!(h.query_by_label("Model (3)").is_some(), "the Model header stays, closed");
    assert!(h.query_by_label("Process (1)").is_none(), "branch model closed");
}

#[test]
fn free_drawing_shows_only_diagrams() {
    let h = pane(CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default()));
    assert!(h.query_by_label_contains("Model (").is_none());
    assert!(h.query_by_label("Diagrams (1)").is_some());
}

#[test]
fn clicking_a_row_selects_its_items_and_switches_page() {
    let mut h = pane(flow_app());
    tree(&mut h, json!({"collapse": ["branch:diagrams"], "expand": ["model"]}));
    h.get_by_label("Do").click();
    h.run_steps(3);
    let a = h.state();
    assert_eq!(a.session.doc.active_page, a.session.doc.pages[0].id);
    assert_eq!(a.session.sel.items.len(), 1);
    // Ok? is drawn on both pages; make the second one active, then click again: it stays.
    h.state_mut().run("page.setActive", json!({"page": "Second"})).unwrap();
    h.run_steps(2);
    h.get_by_label("Ok?").click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc.page().unwrap().name, "Second");
    // Start is only on page one: clicking it goes back.
    h.get_by_label("Start").click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc.page().unwrap().name, "Page-1");
}

#[test]
fn context_mode_follows_the_selection() {
    let mut h = pane(flow_app());
    tree(&mut h, json!({"collapse": ["branch:diagrams"], "mode": "context"}));
    assert!(h.query_by_label("Do").is_some(), "nothing selected: everything shows");
    let ok = shape_of(&h, "ok");
    h.state_mut().run("edit.select", json!({"ids": [ok]})).unwrap();
    h.run_steps(3);
    // Ok? sits on page one with no relationships: only it is left, with no neighbours.
    assert!(h.query_by_label("Ok?").is_some());
    assert!(h.query_by_label("Do").is_none() && h.query_by_label("Start").is_none());
    let sel = shape_of(&h, "do");
    h.state_mut().run("edit.select", json!({"ids": [sel]})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Do").is_some() && h.query_by_label("Start").is_some(), "the neighbour over f1 shows");
    assert!(h.query_by_label("Ok?").is_none());
    let r = tree(&mut h, json!({}));
    assert_eq!(r["mode"], "context");
    assert_eq!(r["selected"], json!(["do"]));
}

fn shape_of(h: &Harness<'static, CraftApp>, element: &str) -> String {
    let doc = &h.state().session.doc;
    doc.pages[0].shapes.iter().find(|s| s.element.as_deref() == Some(element)).map(|s| s.id.clone()).unwrap_or_default()
}

#[test]
fn double_click_renames_through_element_rename() {
    let mut h = pane(flow_app());
    tree(&mut h, json!({"collapse": ["branch:diagrams"]}));
    let pos = h.get_by_label("Do").rect().center();
    h.input_mut().events.push(egui::Event::PointerMoved(pos));
    for pressed in [true, false, true, false] {
        h.input_mut().events.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
    }
    h.run_steps(3);
    // A double click opens the inline editor.
    let edit = h.get_by_role(egui::accesskit::Role::TextInput);
    edit.focus();
    edit.type_text("Blend");
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().session.doc.model.element("do").map(|e| e.name.as_str()), Some("Blend"));
}

#[test]
fn ui_tree_expands_reveals_and_reports() {
    let mut a = flow_app();
    a.run("model.move", json!({"id": "do", "owner": null})).unwrap();
    let r = a.run("ui.tree", json!({"mode": "context", "expand": ["start"], "collapse": ["do"]})).unwrap();
    assert_eq!((r["mode"].as_str(), r["branch"].as_str()), (Some("context"), Some("both")));
    assert!(r["expanded"].as_array().unwrap().contains(&json!("start")));
    assert!(r["collapsed"].as_array().unwrap().contains(&json!("do")));
    let r = a.run("ui.tree", json!({"branch": "model", "reveal": "ok"})).unwrap();
    assert_eq!(r["branch"], "model");
    let ins = vizcraft_ui_egui::control::inspect(&a, &egui::Context::default());
    assert_eq!(ins["tree"]["mode"], "context");
    for bad in [json!({"mode": "x"}), json!({"branch": "x"}), json!({"reveal": "nope"}), json!({"reveal": "a".repeat(999)})] {
        assert!(a.run("ui.tree", bad.clone()).is_err(), "{bad}");
    }
    for junk in [json!(null), json!([]), json!({"expand": "x"}), json!({"expand": [1, null, {"a": 1}]}), json!({"collapse": 5})] {
        assert!(a.run("ui.tree", junk).is_ok());
    }
}

#[test]
fn ten_thousand_elements_stay_smooth() {
    let mut a = CraftApp::new(Session::new(vizcraft_diagram::Document::new()), Services::default());
    a.session.doc.model.packs.clear();
    a.run("element.create", json!({"type": "flowchart.Process", "name": "first", "id": "e0", "x": 0, "y": 0})).unwrap();
    let proto = a.session.doc.model.elements[0].clone();
    for i in 1..10_000 {
        let mut e = proto.clone();
        e.id = format!("e{i}");
        e.name = format!("Element {i}");
        a.session.doc.model.elements.push(e);
    }
    let mut h = pane(a);
    let t = std::time::Instant::now();
    h.run_steps(5);
    assert!(t.elapsed().as_secs_f32() < 10.0, "five frames of ten thousand elements took {:?}", t.elapsed());
    assert!(h.query_by_label("Element 9999").is_none(), "rows far below the fold are not built");
}

#[test]
fn the_model_tabs_show_in_all_views_goes_to_the_page_and_selects_the_shapes() {
    let mut app = flow_app();
    app.ui.tab = "Model".into();
    let on_first = app.session.doc.pages[0].shapes.iter().find(|s| s.element.as_deref() == Some("ok")).map(|s| s.id.clone()).unwrap();
    let on_second = app.session.doc.pages[1].shapes.iter().find(|s| s.element.as_deref() == Some("ok")).map(|s| s.id.clone()).unwrap();
    app.run("edit.select", json!({"ids": [on_first]})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 900.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h.get_by_label("Show in All Views").click();
    h.run_steps(3);
    // The menu entry is the last "Second" on screen (the page tab is another).
    h.get_all_by_label("Second").last().expect("the menu lists the page").click();
    h.run_steps(4);
    assert_eq!(h.state().session.doc.page().map(|p| p.name.as_str()), Some("Second"));
    assert_eq!(h.state().session.sel.items, vec![on_second], "the element's shapes on that page are selected");
}
