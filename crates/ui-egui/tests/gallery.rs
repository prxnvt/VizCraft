//! The template gallery through the app under egui_kittest: cards for every template, search and
//! audience, Create (one undo step, the bar's standards), the details choices, Add Page, Recent,
//! the unsaved-changes notice, a failing template, the preview budget and the keyboard. This
//! binary loads a pack from disk, which changes the process-wide catalog.

use egui::{Key, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use serde_json::{Value, json};
use vizcraft_diagram::sheet::{SheetSize, SheetStandard};
use vizcraft_engine::Session;
use vizcraft_engine::cmd::template;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness(doc: vizcraft_engine::Document) -> Harness<'static, CraftApp> {
    // Every test sees the same catalog (loading a pack replaces it).
    load_broken_pack();
    let app = CraftApp::new(Session::new(doc), Services::default());
    let mut h = Harness::builder().with_size(vec2(1600.0, 1400.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn blank() -> Harness<'static, CraftApp> {
    harness(vizcraft_engine::Document::new())
}

fn run(h: &mut Harness<'static, CraftApp>, id: &str, p: Value) -> Value {
    let r = h.state_mut().run(id, p).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
    r
}

fn gallery(h: &Harness<'static, CraftApp>) -> Value {
    vizcraft_ui_egui::control::inspect(h.state(), &h.ctx)["gallery"].clone()
}

/// Run frames until every preview is built (one per frame), with a cap.
fn settle(h: &mut Harness<'static, CraftApp>) {
    for _ in 0..200 {
        h.run_steps(1);
        if h.state().gallery.pending == 0 {
            return;
        }
    }
    panic!("previews never settled: {} pending", h.state().gallery.pending);
}

fn shown(h: &Harness<'static, CraftApp>) -> Vec<String> {
    gallery(h)["shown"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
}

fn ids(rows: &[Value]) -> Vec<String> {
    rows.iter().map(|r| r["id"].as_str().unwrap().to_string()).collect()
}

/// A pack on disk whose template seeds a command nobody registered (not optional).
fn load_broken_pack() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gallery-broken");
        std::fs::create_dir_all(&dir).unwrap();
        let pack = json!({
            "id": "broke", "version": "1.0.0", "name": "Broken pack", "description": "A template that fails.", "audience": ["research"],
            "glyphs": [{"id": "broke.box", "label": "Box", "category": "Things", "outline": "rectangle", "w": 80.0, "h": 40.0, "fill": "#ffffff", "line": "#000000"}],
            "viewpoints": [{"id": "broke.All", "label": "All", "types": ["*"]}],
            "templates": [{"id": "broke.fails", "label": "Fails Always", "description": "Seeds a missing command.",
                "seed": [{"command": "shape.add", "params": {"stencil": "broke.box", "x": 0, "y": 0}}, {"command": "nobody.hasThis"}]}]
        });
        std::fs::write(dir.join("broke.json"), pack.to_string()).unwrap();
        let mut s = Session::new(vizcraft_engine::Document::new());
        let r = s.run("notation.load", &json!({"path": dir.join("broke.json")})).unwrap();
        assert_eq!(r["errors"], json!([]), "{r}");
    });
}

#[test]
fn every_template_has_a_card_under_its_pack_heading_and_search_and_chips_filter() {
    let mut h = blank();
    run(&mut h, "ui.backstage", json!({"value": true}));
    h.get_by_label("New").click();
    h.run_steps(3);
    assert!(h.state().gallery.open);
    let rows = template::listing(None, None, None);
    for r in &rows {
        let label = r["label"].as_str().unwrap();
        let heading = h.get_all_by_label(r["packName"].as_str().unwrap()).map(|n| n.rect()).fold(f32::MAX, |m, rr| m.min(rr.min.y));
        let card = h.get_all_by_label(label).map(|n| n.rect()).find(|rr| rr.height() > 150.0).unwrap_or_else(|| panic!("no card for {label}"));
        assert!(heading < card.min.y, "{label} sits under its pack heading");
    }
    assert_eq!(&shown(&h)[..rows.len().min(1)], ["basic.blank"], "Blank Drawing first");
    // A pack with viewpoints and no templates keeps a blank page card.
    assert!(shown(&h).contains(&"blank:sheet".to_string()));
    h.get_by_label("Blank Drawing Sheet page");

    // Typing filters: the search has the keyboard when the gallery opens.
    h.event(egui::Event::Text("pipeline".into()));
    h.run_steps(3);
    assert_eq!(shown(&h), ["flowchart.ciPipeline"]);
    run(&mut h, "ui.gallery", json!({"query": ""}));
    h.get_by_label("Software").click();
    h.run_steps(3);
    assert_eq!(gallery(&h)["audience"], "software");
    let software = ids(&template::listing(Some("software"), None, None));
    assert!(!software.is_empty());
    assert_eq!(shown(&h), software);
    h.get_by_label("All").click();
    h.run_steps(2);
    assert!(shown(&h).len() > software.len());
}

#[test]
fn create_runs_template_new_with_the_bar_in_one_undo_step() {
    let mut h = blank();
    run(&mut h, "ui.pane.shapeData", json!({"value": false}));
    run(&mut h, "ui.gallery", json!({"mode": "new"}));
    h.get_by_label("Decision Flow").click();
    h.run_steps(3);
    assert_eq!(gallery(&h)["selected"], "flowchart.decision");
    h.get_by_label("Create").click();
    h.run_steps(4);
    let app = h.state();
    assert!(!app.backstage_open() && !app.gallery.open, "the File page closes");
    assert!(app.ui.panes.is_open("shapeData"), "the Inspector opens");
    assert_eq!(app.session.undo_label(), Some("New from Decision Flow"));
    assert_eq!((app.session.doc.standards.symbols.as_str(), app.session.doc.standards.sheet.as_str()), ("iec", "iso.a3.landscape"), "the bar");
    run(&mut h, "edit.undo", json!({}));
    let doc = &h.state().session.doc;
    assert!(doc.pages.len() == 1 && doc.pages[0].shapes.is_empty(), "one undo gives a blank page");
}

#[test]
fn asme_b_in_the_bar_gives_an_asme_sheet() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({"sheet": "asme.b", "select": "network.physical"}));
    assert_eq!(gallery(&h)["sheet"], "asme.b.landscape");
    h.get_by_label("Create").click();
    h.run_steps(4);
    let sheet = h.state().session.doc.pages.iter().find_map(|p| p.sheet).expect("a sheet");
    assert_eq!((sheet.standard, sheet.size), (SheetStandard::AsmeY14_1, SheetSize::AnsiB));
    assert_eq!(h.state().session.doc.standards.sheet, "asme.b.landscape");
    // The bar's choice is the next New's default (the preferences).
    assert_eq!(h.state().ui.new_standards.sheet, "asme.b.landscape");
    // Junk is refused and changes nothing.
    for bad in [json!({"mode": "sideways"}), json!({"select": "nope.nope"}), json!({"sheet": "iso.z9"}), json!({"symbols": "klingon"})] {
        assert!(h.state_mut().run("ui.gallery", bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(h.state().ui.new_standards.sheet, "asme.b.landscape");
}

#[test]
fn the_details_notation_starts_on_chen_and_barker_draws_barker() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({"select": "er.conceptual"}));
    h.get_by_label("Notation").click();
    h.run_steps(2);
    let chen = h.get_by_label("Chen");
    assert_eq!(chen.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True), "Chen is the engine's choice");
    h.get_by_label("Barker").click();
    h.run_steps(3);
    assert_eq!(h.state().gallery.variant.as_deref(), Some("er.barker"));
    h.get_by_label("Create").click();
    h.run_steps(4);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.view.as_ref().map(|v| v.variant.as_str()), Some("er.barker"));
}

#[test]
fn page_mode_adds_a_page_in_one_step_and_leaves_the_standards() {
    let mut h = blank();
    run(&mut h, "template.new", json!({"template": "flowchart.decision", "standards": {"symbols": "jic", "sheet": "asme.c"}}));
    let standards = h.state().session.doc.standards.clone();
    let pages = h.state().session.doc.pages.len();
    run(&mut h, "ui.gallery", json!({"mode": "page"}));
    h.get_by_label("Add Page");
    assert_eq!(gallery(&h)["symbols"], "jic", "the bar shows the document's standards");
    run(&mut h, "ui.gallery", json!({"select": "er.schema"}));
    h.get_by_label("Create").click();
    h.run_steps(4);
    let doc = &h.state().session.doc;
    assert_eq!(doc.pages.len(), pages + 1);
    assert_eq!(doc.standards, standards);
    assert_eq!(h.state().session.undo_label(), Some("New Page from Database schema"));
    run(&mut h, "edit.undo", json!({}));
    assert_eq!(h.state().session.doc.pages.len(), pages);
    // In page mode the bar edits the document's standards, one undo step.
    run(&mut h, "ui.gallery", json!({"mode": "page", "symbols": "ansi"}));
    assert_eq!(h.state().session.doc.standards.symbols, "ansi");
    assert_eq!(h.state().session.undo_label(), Some("Project Standards"));
}

#[test]
fn recent_lists_the_last_template_and_round_trips() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({"select": "flowchart.decision", "create": true}));
    run(&mut h, "ui.gallery", json!({"mode": "new"}));
    h.get_by_label("Decision Flow (recent)");
    assert_eq!(gallery(&h)["recent"], json!(["flowchart.decision"]));
    let saved = serde_json::to_value(h.state().prefs()).unwrap();
    assert_eq!(saved["recentTemplates"], json!(["flowchart.decision"]));
    run(&mut h, "ui.gallery", json!({"clearRecent": true}));
    assert!(h.state().ui.recent_templates.is_empty());
}

#[test]
fn a_dirty_drawing_shows_the_notice_and_is_not_replaced() {
    let mut h = blank();
    run(&mut h, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
    let before = h.state().session.doc.clone();
    run(&mut h, "ui.gallery", json!({"mode": "new"}));
    let p = h.get_by_label("Decision Flow").rect().center();
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    }
    h.run_steps(2);
    assert_eq!(gallery(&h)["notice"], "flowchart.decision", "the double-click asked first");
    h.get_by_label("Save First");
    h.get_by_label("Replace");
    let e = h.state_mut().run("ui.gallery", json!({"create": true})).unwrap_err();
    assert!(e.contains("unsaved changes"), "{e}");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(gallery(&h)["notice"].is_null());
    assert_eq!(h.state().session.doc, before);
    // Replace goes ahead.
    assert!(h.state_mut().run("ui.gallery", json!({"create": true})).is_err());
    h.run_steps(3);
    h.get_by_label("Replace").click();
    h.run_steps(4);
    assert_eq!(h.state().session.undo_label(), Some("New from Decision Flow"));
}

#[test]
fn a_failing_template_shows_no_preview_and_create_changes_nothing() {
    load_broken_pack();
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({"query": "fails always"}));
    settle(&mut h);
    assert_eq!(shown(&h), ["broke.fails"]);
    assert_eq!(gallery(&h)["failed"], json!(["broke.fails"]));
    let before = h.state().session.doc.clone();
    assert!(h.state_mut().run("ui.gallery", json!({"create": true})).is_err());
    h.run_steps(2);
    assert_eq!(h.state().session.doc, before);
    assert!(h.state().gallery.open, "the gallery stays open");
    let status = h.state().status_msg.as_ref().map(|(s, _)| s.clone()).unwrap_or_default();
    assert!(status.contains("nobody.hasThis"), "{status}");
}

#[test]
fn previews_settle_with_one_texture_per_card() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({}));
    settle(&mut h);
    let g = gallery(&h);
    let cards = h.state().gallery.cards();
    let built = cards.iter().filter(|c| c.blank_viewpoint.is_none() && !g["failed"].as_array().unwrap().contains(&json!(c.id))).count();
    assert_eq!(g["pending"], 0);
    assert_eq!(g["textures"], json!(built));
}

#[test]
fn enter_creates_the_selected_card_and_escape_clears_then_closes() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({}));
    h.event(egui::Event::Text("decision".into()));
    h.run_steps(3);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().gallery.open && h.state().gallery.query.is_empty(), "the first Escape clears the search");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!h.state().gallery.open && !h.state().backstage_open(), "the second closes");

    run(&mut h, "ui.gallery", json!({"select": "flowchart.ciPipeline"}));
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    let next = h.state().gallery.selected.clone().unwrap();
    assert_ne!(next, "flowchart.ciPipeline", "Right moves by one card");
    h.key_press(Key::ArrowLeft);
    h.run_steps(2);
    assert_eq!(h.state().gallery.selected.as_deref(), Some("flowchart.ciPipeline"));
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().session.undo_label(), Some("New from Build and Release Pipeline"));
}
