//! Hyperlinks, comments and subprocesses driven through the app under egui_kittest: a Mod+click
//! follows a url link into a recording `open_external` service, a page link switches page, the
//! Comments pane lists and posts threads, and the Review and Process ribbons run their commands.
//! Assertions read the session and the service, never pixels.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness(opened: Rc<RefCell<Vec<String>>>) -> Harness<'static, CraftApp> {
    let services = Services {
        open_external: Some(Box::new(move |t| {
            opened.borrow_mut().push(t.to_string());
            Ok(())
        })),
        ..Services::default()
    };
    let app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), services);
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn id_of(h: &Harness<'_, CraftApp>, text: &str) -> String {
    h.state().session.doc.page().and_then(|p| p.shapes.iter().find(|s| s.text == text)).map(|s| s.id.clone()).unwrap()
}

fn centre(h: &Harness<'_, CraftApp>, id: &str) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    let c = app.session.doc.page().unwrap().shape(id).unwrap().center();
    vp.world_to_screen(pos2(c.x as f32, c.y as f32))
}

fn click(h: &mut Harness<'_, CraftApp>, p: Pos2, m: Modifiers) {
    h.event(Event::ModifiersChanged(m));
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: m });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: m });
    h.run_steps(1);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
}

#[test]
fn mod_click_on_a_url_link_reaches_the_open_external_service() {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let mut h = harness(opened.clone());
    let start = id_of(&h, "Start");
    h.state_mut().run("link.add", json!({"id": start, "kind": "url", "target": "https://example.com/docs"})).unwrap();
    h.run_steps(2);
    // A plain click only selects.
    let at = centre(&h, &start);
    click(&mut h, at, Modifiers::NONE);
    assert!(opened.borrow().is_empty());
    assert_eq!(h.state().session.sel.items, vec![start.clone()]);
    // Mod+click follows the first link.
    let at = centre(&h, &start);
    click(&mut h, at, Modifiers::COMMAND);
    assert_eq!(*opened.borrow(), vec!["https://example.com/docs".to_string()]);
    // Running the command by id does the same through the same service.
    h.state_mut().run("link.follow", json!({"id": start})).unwrap();
    assert_eq!(opened.borrow().len(), 2);
}

#[test]
fn a_page_link_switches_page_and_the_comments_pane_posts_a_thread() {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let mut h = harness(opened);
    let end = id_of(&h, "End");
    h.state_mut().run("process.createSubprocess", json!({"ids": [end], "name": "Wrap up"})).unwrap();
    h.run_steps(2);
    let stand = h.state().session.sel.primary.clone().unwrap();
    let first = h.state().session.doc.page().unwrap().id.clone();
    let at = centre(&h, &stand);
    click(&mut h, at, Modifiers::COMMAND);
    assert_ne!(h.state().session.doc.page().unwrap().id, first, "the link moved to the subprocess page");
    h.state_mut().run("page.setActive", json!({"page": first.clone()})).unwrap();
    // The Comments pane lists the threads of the active page.
    h.state_mut().run("edit.select", json!({"ids": [stand.clone()]})).unwrap();
    let cid = h.state_mut().run("comment.add", json!({"text": "Check this step"})).unwrap()["id"].as_str().unwrap().to_string();
    h.state_mut().run("ui.pane.comments", json!({"value": true})).unwrap();
    h.run_steps(4);
    assert!(h.state().ui.panes.is_open("comments"));
    h.state_mut().run("comment.reply", json!({"id": cid, "text": "Done"})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().session.doc.comments[0].replies.len(), 1);
    assert_eq!(h.state().session.doc.comments[0].anchor.shape.as_deref(), Some(stand.as_str()));
}

#[test]
fn the_ribbons_run_the_review_and_process_commands() {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let mut h = harness(opened);
    h.state_mut().run("ui.tab", json!({"tab": "Review"})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "Review");
    h.state_mut().run("ui.tab", json!({"tab": "Process"})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "Process");
}
