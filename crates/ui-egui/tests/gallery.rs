//! The New page through the app under egui_kittest: a card for every document type under its
//! discipline, search and audience, Create (one undo step, the bar's standards, an empty page set
//! up for the type), the details choices (viewpoint, notation, sheet), Add Page, Recent, the
//! unsaved-changes notice, Help > Examples with a failing example, the preview budget, the
//! keyboard, and the New page the app starts on. This binary loads a pack from disk, which
//! changes the process-wide catalog.

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

/// Click at `p`, once or twice.
fn click_at(h: &mut Harness<'static, CraftApp>, p: egui::Pos2, times: usize) {
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    for _ in 0..times {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    }
    h.run_steps(2);
}

/// The centre of card `id`, from `ui.inspect`.
fn card_at(h: &Harness<'static, CraftApp>, id: &str) -> egui::Pos2 {
    let g = gallery(h);
    let r = g["cards"].as_array().unwrap().iter().find(|c| c["id"] == id).unwrap_or_else(|| panic!("no card {id}: {g}"))["rect"].clone();
    let n = |i: usize| r[i].as_f64().unwrap() as f32;
    egui::pos2((n(0) + n(2)) / 2.0, (n(1) + n(3)) / 2.0)
}

fn profile(h: &Harness<'static, CraftApp>) -> Vec<String> {
    let p = vizcraft_notation::rules::profile(&h.state().session.doc, vizcraft_notation::catalog());
    if p.enabled { p.rulesets } else { Vec::new() }
}

fn ids(rows: &[Value]) -> Vec<String> {
    rows.iter().map(|r| r["id"].as_str().unwrap().to_string()).collect()
}

/// A pack on disk whose example seeds a command nobody registered (not optional), and no
/// document type.
fn load_broken_pack() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gallery-broken");
        std::fs::create_dir_all(&dir).unwrap();
        let pack = json!({
            "id": "broke", "version": "1.0.0", "name": "Broken pack", "description": "An example that fails.", "audience": ["research"],
            "glyphs": [{"id": "broke.box", "label": "Box", "category": "Things", "outline": "rectangle", "w": 80.0, "h": 40.0, "fill": "#ffffff", "line": "#000000"}],
            "viewpoints": [{"id": "broke.All", "label": "All", "types": ["*"]}],
            "examples": [{"id": "broke.fails", "label": "Fails Always", "description": "Seeds a missing command.",
                "seed": [{"command": "shape.add", "params": {"stencil": "broke.box", "x": 0, "y": 0}}, {"command": "nobody.hasThis"}]}]
        });
        std::fs::write(dir.join("broke.json"), pack.to_string()).unwrap();
        let mut s = Session::new(vizcraft_engine::Document::new());
        let r = s.run("notation.load", &json!({"path": dir.join("broke.json")})).unwrap();
        assert_eq!(r["errors"], json!([]), "{r}");
    });
}

#[test]
fn every_document_type_has_a_card_under_its_discipline_and_search_and_chips_filter() {
    let mut h = blank();
    run(&mut h, "ui.backstage", json!({"value": true}));
    h.get_by_label("New").click();
    h.run_steps(3);
    assert!(h.state().gallery.open);
    let rows = template::listing(None, None, None);
    assert!(rows.len() >= 16);
    for r in &rows {
        let label = r["label"].as_str().unwrap();
        let heading = vizcraft_ui_egui::gallery::discipline_label(r["discipline"].as_str().unwrap());
        let top = h.get_all_by_label(&heading).map(|n| n.rect()).fold(f32::MAX, |m, rr| m.min(rr.min.y));
        let card = h.get_all_by_label(label).map(|n| n.rect()).find(|rr| rr.height() > 150.0).unwrap_or_else(|| panic!("no card for {label}"));
        assert!(top < card.min.y, "{label} sits under {heading}");
    }
    let order = shown(&h);
    assert_eq!(order.first().map(String::as_str), Some("basic.blank"), "Blank Drawing first: {order:?}");
    // Grouped: General, Software, Data, Infrastructure, Engineering.
    let at = |id: &str| order.iter().position(|x| x == id).unwrap_or_else(|| panic!("{id} in {order:?}"));
    assert!(at("flowchart.flowchart") < at("uml.classDiagram") && at("uml.classDiagram") < at("er.diagram"));
    assert!(at("er.diagram") < at("network.physicalDiagram") && at("network.physicalDiagram") < at("sheet.engineeringSheet"));
    assert!(!order.iter().any(|id| id == "flowchart.decision"), "no examples on the New page");
    // A pack with viewpoints and no document types keeps a blank page card.
    assert!(order.contains(&"blank:broke".to_string()));

    // Typing filters: the search has the keyboard when the gallery opens.
    h.event(egui::Event::Text("entity".into()));
    h.run_steps(3);
    assert_eq!(shown(&h), ["er.diagram"]);
    run(&mut h, "ui.gallery", json!({"query": ""}));
    let chip = gallery(&h)["chips"].as_array().unwrap().iter().find(|c| c["label"] == "Software").unwrap()["rect"].clone();
    let n = |i: usize| chip[i].as_f64().unwrap() as f32;
    click_at(&mut h, egui::pos2((n(0) + n(2)) / 2.0, (n(1) + n(3)) / 2.0), 1);
    assert_eq!(gallery(&h)["audience"], "software");
    let software = ids(&template::listing(Some("software"), None, None));
    assert!(!software.is_empty());
    let mut got = shown(&h);
    got.sort();
    let mut want = software.clone();
    want.sort();
    assert_eq!(got, want);
    run(&mut h, "ui.gallery", json!({"audience": ""}));
    assert!(shown(&h).len() > software.len());
}

#[test]
fn create_makes_an_empty_page_set_up_for_the_type_in_one_undo_step() {
    let mut h = blank();
    run(&mut h, "ui.pane.shapeData", json!({"value": false}));
    run(&mut h, "ui.gallery", json!({"mode": "new"}));
    let at = card_at(&h, "flowchart.flowchart");
    click_at(&mut h, at, 1);
    assert_eq!(gallery(&h)["selected"], "flowchart.flowchart");
    h.get_by_label("Checks: ISO 5807 flowchart");
    h.get_by_label("Create").click();
    h.run_steps(4);
    let app = h.state();
    assert!(!app.backstage_open() && !app.gallery.open, "the File page closes");
    assert!(app.ui.panes.is_open("shapeData"), "the Inspector opens");
    assert_eq!(app.session.undo_label(), Some("New from Flowchart"));
    let page = app.session.doc.page().unwrap();
    assert!(page.shapes.is_empty(), "a document type draws nothing");
    assert_eq!(page.view.as_ref().map(|v| v.viewpoint.as_str()), Some("flowchart.Flowchart"));
    assert_eq!((app.session.doc.standards.symbols.as_str(), app.session.doc.standards.sheet.as_str()), ("iec", "iso.a3.landscape"), "the bar");
    assert_eq!(profile(&h), ["flowchart.iso5807"]);
    let palette = &vizcraft_ui_egui::control::inspect(h.state(), &h.ctx)["palette"];
    assert_eq!(palette["viewpoint"], "flowchart.Flowchart", "the palette shows the type's shapes");
    run(&mut h, "edit.undo", json!({}));
    let doc = &h.state().session.doc;
    assert!(doc.pages.len() == 1 && doc.pages[0].view.is_none(), "one undo gives a blank page");
}

#[test]
fn asme_b_in_the_bar_gives_an_asme_sheet() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({"sheet": "asme.b", "select": "network.physicalDiagram"}));
    assert_eq!(gallery(&h)["sheet"], "asme.b.landscape");
    h.get_by_label("Create").click();
    h.run_steps(4);
    let sheet = h.state().session.doc.pages.iter().find_map(|p| p.sheet).expect("a sheet");
    assert_eq!((sheet.standard, sheet.size), (SheetStandard::AsmeY14_1, SheetSize::AnsiB));
    assert_eq!(h.state().session.doc.standards.sheet, "asme.b.landscape");
    // The bar's choice is the next New's default (the preferences).
    assert_eq!(h.state().ui.new_standards.sheet, "asme.b.landscape");
    // Junk is refused and changes nothing.
    for bad in [
        json!({"mode": "sideways"}),
        json!({"select": "nope.nope"}),
        json!({"select": "flowchart.decision"}),
        json!({"sheet": "iso.z9"}),
        json!({"symbols": "klingon"}),
        json!({"select": "er.diagram", "viewpoint": "uml.Sequence"}),
        json!({"mode": "examples", "select": "er.diagram"}),
    ] {
        assert!(h.state_mut().run("ui.gallery", bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(h.state().ui.new_standards.sheet, "asme.b.landscape");
}

#[test]
fn the_details_viewpoint_and_notation_reach_the_document() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({"select": "er.diagram"}));
    h.get_by_label("Checks: Physical data model");
    h.get_by_label("Viewpoint").click();
    h.run_steps(2);
    h.get_by_label("Conceptual").click();
    h.run_steps(3);
    assert_eq!(h.state().gallery.viewpoint.as_deref(), Some("er.Conceptual"));
    h.get_by_label("Checks: Conceptual data model");
    h.get_by_label("Notation").click();
    h.run_steps(2);
    let ie = h.get_by_label("Information Engineering");
    assert_eq!(ie.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True), "crow's foot is the type's notation");
    h.get_by_label("Chen").click();
    h.run_steps(3);
    assert_eq!(h.state().gallery.variant.as_deref(), Some("er.chen"));
    h.get_by_label("Create").click();
    h.run_steps(4);
    let view = h.state().session.doc.page().unwrap().view.clone().unwrap();
    assert_eq!((view.viewpoint.as_str(), view.variant.as_str()), ("er.Conceptual", "er.chen"));
    assert_eq!(profile(&h), ["er.conceptual"]);
    // The same through ui.gallery.
    h.state_mut().session.mark_saved();
    run(&mut h, "ui.gallery", json!({"select": "er.diagram", "viewpoint": "er.Logical", "create": true}));
    assert_eq!(h.state().session.doc.page().unwrap().view.as_ref().map(|v| v.viewpoint.as_str()), Some("er.Logical"));
}

#[test]
fn page_mode_adds_a_page_in_one_step_and_leaves_the_standards() {
    let mut h = blank();
    run(&mut h, "template.new", json!({"template": "flowchart.flowchart", "standards": {"symbols": "jic", "sheet": "asme.c"}}));
    let standards = h.state().session.doc.standards.clone();
    let pages = h.state().session.doc.pages.len();
    run(&mut h, "ui.gallery", json!({"mode": "page"}));
    h.get_by_label("Add Page");
    assert_eq!(gallery(&h)["symbols"], "jic", "the bar shows the document's standards");
    run(&mut h, "ui.gallery", json!({"select": "er.diagram"}));
    h.get_by_label("Create").click();
    h.run_steps(4);
    let doc = &h.state().session.doc;
    assert_eq!(doc.pages.len(), pages + 1);
    assert_eq!(doc.standards, standards);
    assert_eq!(h.state().session.undo_label(), Some("New Page from Entity Relationship Diagram"));
    assert_eq!(profile(&h), ["flowchart.iso5807", "er.physical"], "the page's rules join the document's");
    run(&mut h, "edit.undo", json!({}));
    assert_eq!(h.state().session.doc.pages.len(), pages);
    // In page mode the bar edits the document's standards, one undo step.
    run(&mut h, "ui.gallery", json!({"mode": "page", "symbols": "ansi"}));
    assert_eq!(h.state().session.doc.standards.symbols, "ansi");
    assert_eq!(h.state().session.undo_label(), Some("Project Standards"));
}

#[test]
fn recent_lists_the_last_document_type_and_round_trips() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({"select": "flowchart.flowchart", "create": true}));
    run(&mut h, "ui.gallery", json!({"mode": "new"}));
    h.get_by_label("Flowchart (recent)");
    assert_eq!(gallery(&h)["recent"], json!(["flowchart.flowchart"]));
    let saved = serde_json::to_value(h.state().prefs()).unwrap();
    assert_eq!(saved["recentTemplates"], json!(["flowchart.flowchart"]));
    run(&mut h, "ui.gallery", json!({"clearRecent": true}));
    assert!(h.state().ui.recent_templates.is_empty());
}

#[test]
fn a_dirty_drawing_shows_the_notice_and_is_not_replaced() {
    let mut h = blank();
    run(&mut h, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
    let before = h.state().session.doc.clone();
    run(&mut h, "ui.gallery", json!({"mode": "new"}));
    let at = card_at(&h, "uml.classDiagram");
    click_at(&mut h, at, 2);
    assert_eq!(gallery(&h)["notice"], "uml.classDiagram", "the double-click asked first");
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
    assert_eq!(h.state().session.undo_label(), Some("New from Class Diagram"));
}

#[test]
fn examples_open_filled_in_from_help_and_a_failing_one_changes_nothing() {
    load_broken_pack();
    let mut h = blank();
    run(&mut h, "ui.tab", json!({"tab": "Help"}));
    h.get_by_label("Examples").click();
    h.run_steps(4);
    let g = gallery(&h);
    assert_eq!((g["open"].as_bool(), g["mode"].as_str()), (Some(true), Some("examples")));
    let mut want = ids(&template::example_listing(None, None, None));
    let mut got = shown(&h);
    assert!(got.contains(&"flowchart.decision".to_string()) && !got.contains(&"flowchart.flowchart".to_string()));
    want.sort();
    got.sort();
    assert_eq!(got, want);
    // A failing example shows "No preview" and Create leaves the drawing alone.
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
    // A good one opens filled in, in one undo step, with the bar's standards.
    run(&mut h, "ui.gallery", json!({"query": "", "symbols": "jic"}));
    run(&mut h, "ui.gallery", json!({"select": "flowchart.decision", "create": true}));
    let app = h.state();
    assert!(!app.gallery.open && app.session.doc.page().unwrap().shapes.len() == 6);
    assert_eq!((app.session.undo_depth(), app.session.undo_label()), (1, Some("New from Decision Flow")));
    assert_eq!(app.session.doc.standards.symbols, "jic");
    assert!(app.ui.recent_templates.is_empty(), "examples do not fill the New page's Recent row");
    // The unsaved notice applies here too.
    run(&mut h, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
    let e = h.state_mut().run("ui.gallery", json!({"mode": "examples", "select": "flowchart.ciPipeline", "create": true})).unwrap_err();
    assert!(e.contains("unsaved changes"), "{e}");
}

#[test]
fn the_help_menu_offers_the_examples() {
    let h = blank();
    let tree = vizcraft_ui_egui::menus::menu_tree(h.state());
    let help = tree.iter().find(|m| m.title == "Help").unwrap();
    assert!(help.items.iter().any(|i| matches!(i, vizcraft_ui_egui::menus::Item::Cmd("Examples...", "ui.gallery", p) if p["mode"] == "examples")));
}

#[test]
fn previews_settle_with_a_texture_per_sheet_or_example() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({}));
    settle(&mut h);
    let g = gallery(&h);
    assert_eq!(g["pending"], 0);
    let st = vizcraft_ui_egui::gallery::bar(h.state());
    let cards = h.state().gallery.cards();
    let sheets = cards
        .iter()
        .filter(|c| c.blank_viewpoint.is_none())
        .filter(|c| template::resolved(&c.id, &template::Choices::default(), Some(&st)).is_ok_and(|(_, sheet)| !sheet.is_empty()))
        .count();
    assert!(sheets >= 2, "the physical network and the engineering sheet draw their sheets");
    assert_eq!(g["textures"], json!(sheets), "only a sheet is rendered; the other types are a plain page and shapes");
    run(&mut h, "ui.gallery", json!({"mode": "examples"}));
    settle(&mut h);
    let g = gallery(&h);
    let examples = h.state().gallery.cards().iter().filter(|c| !g["failed"].as_array().unwrap().contains(&json!(c.id))).count();
    assert!(g["textures"].as_u64().unwrap() as usize >= examples, "every example has its drawing");
}

#[test]
fn enter_creates_the_selected_card_and_escape_clears_then_closes() {
    let mut h = blank();
    run(&mut h, "ui.gallery", json!({}));
    h.event(egui::Event::Text("sequence".into()));
    h.run_steps(3);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().gallery.open && h.state().gallery.query.is_empty(), "the first Escape clears the search");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!h.state().gallery.open && !h.state().backstage_open(), "the second closes");

    run(&mut h, "ui.gallery", json!({"select": "uml.sequenceDiagram"}));
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    let next = h.state().gallery.selected.clone().unwrap();
    assert_ne!(next, "uml.sequenceDiagram", "Right moves by one card");
    h.key_press(Key::ArrowLeft);
    h.run_steps(2);
    assert_eq!(h.state().gallery.selected.as_deref(), Some("uml.sequenceDiagram"));
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().session.undo_label(), Some("New from Sequence Diagram"));
}

#[test]
fn the_app_starts_on_the_new_page_and_escape_or_blank_drawing_gives_the_blank_page() {
    let mut h = blank();
    vizcraft_ui_egui::gallery::start(h.state_mut());
    h.run_steps(4);
    let g = gallery(&h);
    assert!(h.state().backstage_open() && g["open"] == true && g["mode"] == "new", "{g}");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!h.state().backstage_open() && !h.state().gallery.open, "Escape leaves for the blank page");
    let doc = &h.state().session.doc;
    assert!(doc.pages.len() == 1 && doc.pages[0].shapes.is_empty() && !h.state().session.can_undo());

    let mut h = blank();
    vizcraft_ui_egui::gallery::start(h.state_mut());
    h.run_steps(4);
    let at = card_at(&h, "basic.blank");
    click_at(&mut h, at, 2);
    assert!(!h.state().backstage_open(), "Blank Drawing closes the New page");
    assert!(h.state().session.doc.page().unwrap().view.is_none());
}

#[test]
fn a_file_opened_after_the_start_goes_straight_to_it() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gallery-start");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.vizcraft");
    let mut s = Session::new(vizcraft_engine::Document::new());
    s.run("example.open", &json!({"example": "flowchart.decision", "path": path.to_string_lossy()})).unwrap();
    let mut h = blank();
    vizcraft_ui_egui::gallery::start(h.state_mut());
    h.run_steps(3);
    // As the host opens a file from Finder or the command line.
    run(&mut h, "file.open", json!({"path": path.to_string_lossy()}));
    run(&mut h, "ui.backstage", json!({"value": false}));
    assert!(!h.state().backstage_open() && !h.state().gallery.open);
    assert_eq!(h.state().session.doc.page().unwrap().shapes.len(), 6);
}
