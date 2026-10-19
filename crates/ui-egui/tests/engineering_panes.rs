//! Engineering drawing panes through the app under egui_kittest: the Inspector's Symbol, Net,
//! Sheet and document property groups, the Nets dock tab, the contextual Sheet tab and the
//! Insert > Field document property submenu.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_diagram::glyph_state::GlyphState;
use vizcraft_diagram::{Connector, Document, End, Shape, ShapeKind};
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

type H = Harness<'static, CraftApp>;

fn harness_with(doc: Document) -> H {
    let mut app = CraftApp::new(Session::new(doc), Services::default());
    app.run("ui.pane", json!({"name": "shapeData", "value": true})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn run(h: &mut H, id: &str, params: Value) -> Value {
    let r = h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
    r
}

/// Type `text` into the focused field, replacing what is there, and press Enter.
fn type_over(h: &mut H, text: &str) {
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.event(egui::Event::Text(text.into()));
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
}

/// A free contact symbol drawn by the `contact` builder (no pack glyph needed), selected.
fn contact() -> H {
    let mut doc = Document::new();
    let mut s = Shape::new("k1", ShapeKind::Freeform, 200.0, 200.0, 40.0, 80.0);
    s.glyph = Some(GlyphState { builder: "contact".into(), ..GlyphState::default() });
    doc.page_mut().unwrap().shapes.push(s);
    let mut h = harness_with(doc);
    run(&mut h, "glyph.rebuild", json!({}));
    run(&mut h, "edit.select", json!({"ids": ["k1"]}));
    h
}

fn ports(h: &H) -> Vec<String> {
    h.state().session.doc.page().unwrap().shape("k1").unwrap().glyph.as_ref().unwrap().ports.clone()
}

#[test]
fn the_symbol_group_edits_a_contacts_poles_and_it_redraws() {
    let mut h = contact();
    assert_eq!(ports(&h).len(), 2, "one pole, two terminals");
    h.get_by_label("Symbol");
    h.get_by_role_and_label(Role::SpinButton, "Poles").click();
    h.run_steps(2);
    type_over(&mut h, "3");
    assert_eq!(ports(&h).len(), 6, "three poles redraw with six terminals");
    let fields = h.state_mut().session.run("glyph.params", &json!({"id": "k1"})).unwrap();
    let poles = fields["params"].as_array().unwrap().iter().find(|p| p["key"] == "poles").unwrap().clone();
    assert_eq!((poles["value"].as_f64(), poles["from"].as_str()), (Some(3.0), Some("shape")), "a free shape keeps it as its own override");
    // The reset button clears the override and the contact redraws with one pole.
    h.get_by_label("x").click();
    h.run_steps(4);
    assert_eq!(ports(&h).len(), 2);
}

/// Two boxes joined by a connector that is a wire net's segment, the connector selected.
fn wired() -> (H, String) {
    let mut doc = Document::new();
    let page = doc.page_mut().unwrap();
    page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 60.0, 40.0));
    page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 300.0, 0.0, 60.0, 40.0));
    page.connectors.push(Connector::new("c1", End::dynamic("a"), End::dynamic("b")));
    let mut h = harness_with(doc);
    let net = run(&mut h, "net.assign", json!({"ids": ["c1"], "kind": "wire"}))["net"].as_str().unwrap().to_string();
    run(&mut h, "net.set", json!({"net": net, "number": "101"}));
    run(&mut h, "edit.select", json!({"ids": ["c1"]}));
    (h, net)
}

#[test]
fn a_selected_connector_shows_its_net_and_opens_it_in_the_nets_tab() {
    let (mut h, net) = wired();
    assert!(h.query_all_by_value("101").count() >= 1, "the net number is shown");
    h.get_by_label("Show in Nets").click();
    h.run_steps(4);
    let r = h.state_mut().run("ui.dock", json!({"open": true})).unwrap();
    assert_eq!(r["tab"], "nets");
    let nets = h.state_mut().run("ui.nets", json!({})).unwrap();
    assert_eq!(nets["selected"], net);
}

#[test]
fn the_nets_tab_lists_selects_edits_the_number_and_goes_to_a_net() {
    let (mut h, net) = wired();
    run(&mut h, "edit.select", json!({"ids": []}));
    run(&mut h, "ui.dock", json!({"tab": "nets"}));
    let r = h.state_mut().run("ui.nets", json!({})).unwrap();
    assert_eq!(r["nets"][0]["number"], "101");
    assert_eq!(r["nets"][0]["kind"], "wire");
    // Click the number, type a new one, Enter: net.set.
    h.get_by_label(&format!("Number of net {net}")).click();
    h.run_steps(3);
    type_over(&mut h, "205");
    assert_eq!(h.state().session.doc.model.net(&net).unwrap().number, "205");
    // The members show for the selected net; Go to selects its wire.
    h.get_by_label(&format!("Members of {net}"));
    h.get_by_label("Go to").click();
    h.run_steps(4);
    assert_eq!(h.state().session.sel.items, vec!["c1".to_string()]);
}

#[test]
fn the_document_property_form_shows_the_title_block_fields_and_the_sheet() {
    let mut h = harness_with(Document::new());
    run(&mut h, "sheet.set", json!({}));
    run(&mut h, "doc.setProperty", json!({"key": "legalOwner", "value": "Works Ltd"}));
    run(&mut h, "edit.setTitle", json!({"title": "Pump skid"}));
    h.get_by_label("Legal owner *");
    assert!(h.query_all_by_value("Works Ltd").count() >= 1);
    // One title: the document title field shows what the title block shows.
    assert!(h.query_all_by_value("Pump skid").count() >= 1);
    // The Sheet section names the preset and the zones.
    h.get_by_label("ISO 5457");
    h.get_by_label("A3 landscape");
    h.get_by_label("8 columns by 6 rows");
    let f = vizcraft_ui_egui::panes::inspector_fields(h.state());
    assert_eq!((f["sheet"]["preset"].as_str(), f["sheet"]["zones"]["cols"].as_u64()), (Some("iso.a3.landscape"), Some(8)));
}

#[test]
fn the_sheet_tab_switches_iso_a3_to_asme_b_with_its_frame_and_title_block() {
    let mut h = harness_with(Document::new());
    assert!(h.query_by_label("Sheet").is_none(), "no Sheet tab on a plain page");
    run(&mut h, "sheet.set", json!({"sheet": "iso.a3.landscape"}));
    run(&mut h, "ui.tab", json!({"tab": "Sheet"}));
    h.get_by_label("Sheet Standard").click();
    h.run_steps(3);
    h.get_by_label("ASME Y14.1").click();
    h.run_steps(4);
    let doc = &h.state().session.doc;
    let sh = doc.pages[0].sheet.unwrap();
    assert_eq!(sh.preset_id(), "asme.b.landscape", "ASME takes its usual B size and keeps the orientation");
    assert_eq!(sh.title_block, vizcraft_diagram::sheet::TitleBlockKind::Asme);
    let frame = &doc.pages[vizcraft_diagram::decor::generated_page(doc, &doc.pages[0].id).unwrap()];
    assert!(frame.shapes.iter().any(|s| s.text == "Dwg. no."), "the ASME title block is drawn");
    assert!((doc.pages[0].width - 1700.0).abs() < 1e-6, "17 inches wide");
    // The Size menu offers every standard's sizes; choosing D resizes the sheet.
    h.get_by_label("Sheet Size").click();
    h.run_steps(3);
    h.get_by_label("ANSI D  (34 x 22 in)").click();
    h.run_steps(4);
    assert_eq!(h.state().session.doc.pages[0].sheet.unwrap().preset_id(), "asme.d.landscape");
    // Revise without a description opens the dialog.
    h.get_by_label("Revise").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("revise"));
}

#[test]
fn insert_field_offers_document_properties_by_key() {
    let mut doc = Document::new();
    doc.page_mut().unwrap().shapes.push(Shape::new("t", ShapeKind::Rectangle, 0.0, 0.0, 120.0, 40.0));
    let mut h = harness_with(doc);
    run(&mut h, "edit.select", json!({"ids": ["t"]}));
    run(&mut h, "ui.tab", json!({"tab": "Insert"}));
    h.get_by_label("Insert Field").click();
    h.run_steps(3);
    assert!(h.query_by_label("Page Name").is_some(), "the Field menu is open");
    h.get_by_label_contains("Document Property").click();
    h.run_steps(3);
    h.get_by_label("Legal owner").click();
    h.run_steps(4);
    assert!(h.state().session.doc.page().unwrap().shape("t").unwrap().text.contains("{{doc:legalOwner}}"));
}
