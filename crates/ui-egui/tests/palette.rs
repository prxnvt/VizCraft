//! The scoped palette, the notation tab, New from notation and the connector kind, through the
//! app under egui_kittest. This binary loads a pack from disk, which changes the process-wide
//! catalog, so nothing here counts packs.

use egui::{Event, Modifiers, PointerButton, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness(doc: vizcraft_engine::Document) -> Harness<'static, CraftApp> {
    let app = CraftApp::new(Session::new(doc), Services::default());
    let mut h = Harness::builder().with_size(vec2(1400.0, 900.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn run(h: &mut Harness<'static, CraftApp>, id: &str, p: Value) -> Value {
    let r = h.state_mut().run(id, p).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
    r
}

fn group_labels(r: &Value) -> Vec<String> {
    r["groups"].as_array().unwrap().iter().map(|g| g["label"].as_str().unwrap().to_string()).collect()
}

fn items_of(r: &Value, group: &str) -> Vec<String> {
    let g = r["groups"].as_array().unwrap().iter().find(|g| g["id"] == group).unwrap();
    g["items"].as_array().unwrap().iter().map(|i| i.as_str().unwrap().to_string()).collect()
}

/// A pack on disk with two element types and a viewpoint that lists only one of them.
fn load_mini_pack() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("palette-mini");
        std::fs::create_dir_all(&dir).unwrap();
        let glyph = |n: &str| {
            json!({"id": format!("mini.{n}"), "label": n.to_uppercase(), "category": "Things", "outline": "rectangle", "w": 80.0, "h": 40.0,
            "fill": "#ffffff", "line": "#000000", "elementType": format!("mini.{}", n.to_uppercase())})
        };
        let pack = json!({
            "id": "mini", "version": "1.0.0", "name": "Mini pack", "description": "Two things.", "audience": ["research"],
            "glyphs": [glyph("a"), glyph("b")],
            "elementTypes": [{"id": "mini.A", "label": "Alpha", "glyph": "mini.a"}, {"id": "mini.B", "label": "Beta", "glyph": "mini.b"}],
            "palette": [{"id": "mini.things", "label": "Things", "types": ["mini.A", "mini.B"]}],
            "viewpoints": [{"id": "mini.OnlyA", "label": "Only Alpha", "types": ["mini.A"]}, {"id": "mini.Both", "label": "Both", "types": ["*"]}]
        });
        std::fs::write(dir.join("mini.json"), pack.to_string()).unwrap();
        let mut s = Session::new(vizcraft_engine::Document::new());
        let r = s.run("notation.load", &json!({"path": dir.join("mini.json")})).unwrap();
        assert_eq!(r["errors"], json!([]), "{r}");
    });
}

#[test]
fn a_flowchart_page_shows_steps_data_links_connectors_drawing() {
    let mut h = harness(vizcraft_engine::Document::new());
    let r = run(&mut h, "model.setViewpoint", json!({"viewpoint": "flowchart.Flowchart"}));
    assert_eq!(r["viewpoint"], "flowchart.Flowchart");
    let r = h.state_mut().run("ui.palette", json!({})).unwrap();
    assert_eq!(group_labels(&r), ["Steps", "Data", "Links", "Connectors", "More connectors", "Symbols"]);
    assert_eq!(items_of(&r, "flowchart.steps")[0], "flowchart.terminator");
    assert_eq!(items_of(&r, "connectors")[0], "flowchart.flow");
    // The same report is in ui.inspect, and the rows are on screen.
    let inspect = vizcraft_ui_egui::control::inspect(h.state(), &h.ctx);
    assert_eq!(group_labels(&inspect["palette"]), ["Steps", "Data", "Links", "Connectors", "More connectors", "Symbols"]);
    h.get_by_label("Decision");
    h.get_by_label("Flow");
    assert!(h.query_by_label("Cloud").is_none(), "the Basic primitives are not listed");
}

#[test]
fn the_viewpoint_filter_hides_a_type_outside_the_viewpoint() {
    load_mini_pack();
    let mut h = harness(vizcraft_engine::Document::new());
    run(&mut h, "model.setViewpoint", json!({"viewpoint": "mini.OnlyA"}));
    let r = h.state_mut().run("ui.palette", json!({})).unwrap();
    assert_eq!(items_of(&r, "mini.things"), ["mini.a"]);
    assert!(h.query_by_label("Beta").is_none() && h.query_by_label("Alpha").is_some());
    let r = h.state_mut().run("ui.palette", json!({"viewpointFilter": false})).unwrap();
    assert_eq!(items_of(&r, "mini.things"), ["mini.a", "mini.b"]);
    h.run_steps(3);
    assert!(h.query_by_label("Beta").is_some());
    // The wider viewpoint lists both again with the filter on.
    run(&mut h, "ui.palette", json!({"viewpointFilter": true}));
    run(&mut h, "model.setViewpoint", json!({"viewpoint": "mini.Both"}));
    let r = h.state_mut().run("ui.palette", json!({})).unwrap();
    assert_eq!(items_of(&r, "mini.things"), ["mini.a", "mini.b"]);
}

#[test]
fn a_pack_loaded_from_disk_appears_without_a_restart() {
    let mut h = harness(vizcraft_engine::Document::new());
    // No view: every pack's groups are closed under the pack name; the Basic primitives are not listed.
    assert!(h.query_by_label("Cloud Topology").is_some());
    assert!(h.query_by_label("Rectangle").is_none(), "the Basic primitives are not listed");
    assert!(h.query_by_label("Alpha").is_none());
    load_mini_pack();
    h.run_steps(3);
    h.get_by_label("Mini pack");
    run(&mut h, "ui.palette", json!({"group": "mini"}));
    h.get_by_label("Alpha");
    // The stencil catalog and search follow, too.
    let found = h.state_mut().run("shape.search", json!({"query": "things"})).unwrap();
    assert!(found.to_string().contains("mini.b"), "{found}");
}

#[test]
fn two_decision_rows_each_name_their_pack_on_a_page_without_a_view() {
    let mut h = harness(vizcraft_engine::Document::new());
    run(&mut h, "ui.palette", json!({"query": "decision"}));
    h.run_steps(3);
    // The Flowchart and UML Decision rows share a label; each names its pack.
    assert_eq!(h.query_all_by_label("Decision").count(), 2);
    let pal = vizcraft_ui_egui::palette::for_app(h.state());
    let ids: Vec<&str> = pal.search("decision").iter().map(|i| i.id).collect();
    let (f, u) = (ids.iter().position(|i| *i == "flowchart.decision"), ids.iter().position(|i| *i == "uml.decision"));
    assert!(f.is_some() && u.is_some() && f < u, "{ids:?}");
}

#[test]
fn search_covers_types_kinds_and_drawing_masters() {
    let mut h = harness(vizcraft_engine::Document::new());
    run(&mut h, "model.setViewpoint", json!({"viewpoint": "flowchart.Flowchart"}));
    run(&mut h, "ui.palette", json!({"query": "decision"}));
    h.get_by_label("Decision");
    run(&mut h, "ui.palette", json!({"query": "flow"}));
    h.get_by_label("Flow");
    run(&mut h, "ui.palette", json!({"query": "cloud"}));
    h.get_by_label("Cloud");
    run(&mut h, "ui.palette", json!({"query": "zzzzzzqq"}));
    h.get_by_label("No shapes match");
}

#[test]
fn new_from_notation_gives_a_page_with_the_viewpoint_set() {
    let mut h = harness(vizcraft_engine::Document::new());
    run(&mut h, "ui.backstage", json!({"value": true}));
    h.get_by_label("New").click();
    h.run_steps(3);
    h.get_by_label("Blank Drawing");
    assert!(h.get_all_by_label("General").count() >= 1, "the General discipline");
    // The gallery's card for the Flowchart document type, then Create.
    h.get_all_by_label("Flowchart").find(|n| n.rect().height() > 150.0).expect("the Flowchart card").click();
    h.run_steps(3);
    h.get_by_label("Create").click();
    h.run_steps(4);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.view.as_ref().map(|v| v.viewpoint.as_str()), Some("flowchart.Flowchart"));
    assert!(!h.state().backstage_open(), "the File page closes");
    // The palette followed, and the notation tab is in the strip.
    let r = h.state_mut().run("ui.palette", json!({})).unwrap();
    assert_eq!(group_labels(&r)[0], "Steps");
    assert!(vizcraft_ui_egui::ribbon::tabs(h.state()).iter().any(|t| t.name == "Flowchart" && t.visible(h.state())));
}

#[test]
fn a_connector_kind_row_arms_the_tool_and_the_drag_draws_that_kind() {
    let mut h = harness(vizcraft_engine::sample::sample_document());
    run(&mut h, "model.setViewpoint", json!({"viewpoint": "flowchart.Flowchart"}));
    h.get_by_label("Flow").click();
    h.run_steps(3);
    assert_eq!(h.state().connector_kind, "flowchart.flow");
    assert_eq!(h.state().tool, vizcraft_ui_egui::Tool::Connector);
    let n = h.state().session.doc.page().unwrap().connectors.len();
    let screen = |h: &Harness<'_, CraftApp>, id: &str| {
        let c = h.state().session.doc.page().unwrap().shape(id).unwrap().center();
        let vp = h.state().canvas.viewport(&h.state().session.view).unwrap();
        vp.world_to_screen(pos2(c.x as f32, c.y as f32))
    };
    let (a, b) = (screen(&h, "s7"), screen(&h, "s5"));
    h.event(Event::PointerMoved(a));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=8 {
        h.event(Event::PointerMoved(a + (b - a) * (i as f32 / 8.0)));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.connectors.len(), n + 1);
    assert_eq!(page.connectors.last().unwrap().kind, "flowchart.flow");
    // A plain connector tool goes back to the default kind, and an unknown kind is refused.
    run(&mut h, "ui.tool", json!({"tool": "connector"}));
    assert_eq!(h.state().connector_kind, "");
    assert!(h.state_mut().run("ui.tool", json!({"tool": "connector", "kind": "nope.nothing"})).is_err());
}

#[test]
fn the_notation_and_model_tabs_are_built_from_data() {
    let mut h = harness(vizcraft_engine::Document::new());
    let tabs = |h: &Harness<'static, CraftApp>| {
        vizcraft_ui_egui::ribbon::tabs(h.state()).iter().filter(|t| t.visible(h.state())).map(|t| t.name).collect::<Vec<_>>()
    };
    assert!(tabs(&h).contains(&"Model") && !tabs(&h).contains(&"Flowchart"));
    run(&mut h, "model.setViewpoint", json!({"viewpoint": "flowchart.Flowchart"}));
    assert!(tabs(&h).contains(&"Flowchart"));
    run(&mut h, "ui.pane", json!({"name": "shapes", "value": false}));
    run(&mut h, "ui.tab", json!({"tab": "Flowchart"}));
    h.get_by_label("Viewpoint");
    h.get_by_label("Steps").click();
    h.run_steps(3);
    h.get_by_label("Terminator").click();
    h.run_steps(3);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.shapes.last().unwrap().stencil, "flowchart.terminator");
    assert!(page.shapes.last().unwrap().element.is_some(), "the shape draws a new element of the type");
}

#[test]
fn breadcrumbs_show_the_title_and_page_and_a_drill_down_owner() {
    let mut h = harness(vizcraft_engine::sample::sample_document());
    let c = vizcraft_ui_egui::breadcrumbs::crumbs(h.state());
    assert_eq!(c.len(), 2);
    assert_eq!(c[1].label, h.state().session.doc.page().unwrap().name);
    let page = h.state().session.doc.page().unwrap().id.clone();
    run(&mut h, "page.add", json!({"name": "Second"}));
    let first = h.state().session.doc.pages[0].id.clone();
    assert_ne!(first, "");
    // A second page: the title crumb goes back to the first page.
    run(&mut h, "page.setActive", json!({"page": "Second"}));
    let c = vizcraft_ui_egui::breadcrumbs::crumbs(h.state());
    assert_eq!((c.len(), c[0].page.as_deref()), (2, Some(page.as_str())));
    h.get_by_label(&format!("Crumb {}", c[0].label)).click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc.page().unwrap().id, page);
    // A drill-down page `<element id>/<viewpoint>`: title, the element, the viewpoint.
    let r = run(&mut h, "shape.add", json!({"stencil": "flowchart.process", "x": 10, "y": 10, "text": "Step one"}));
    let element = r["element"].as_str().unwrap().to_string();
    run(&mut h, "page.setActive", json!({"page": "Second"}));
    run(&mut h, "page.rename", json!({"page": "Second", "name": format!("{element}/flowchart.Flowchart")}));
    let c = vizcraft_ui_egui::breadcrumbs::crumbs(h.state());
    let labels: Vec<&str> = c.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels[1..], ["Step one", "Flowchart"], "{labels:?}");
    assert_eq!(c[1].page.as_deref(), Some(page.as_str()), "the element crumb goes to a page that draws it");
}

#[test]
fn an_er_page_folds_the_other_notations_connectors_and_follows_the_variant() {
    let mut h = harness(vizcraft_engine::Document::new());
    run(&mut h, "template.new", json!({"template": "er.diagram"}));
    let r = h.state_mut().run("ui.palette", json!({})).unwrap();
    assert_eq!(items_of(&r, "connectors"), ["er.relationship", "er.identifying"]);
    let more = r["groups"].as_array().unwrap().iter().find(|g| g["id"] == "moreConnectors").unwrap().clone();
    assert_eq!((more["label"].as_str(), more["open"].as_bool()), (Some("More connectors"), Some(false)), "folded at the bottom");
    for k in ["er.chenRelationship", "er.barkerRelationship", "er.idef1xIdentifying", "basic.line", "basic.arrow", "basic.twoWay", "basic.bus"] {
        assert!(more["items"].as_array().unwrap().contains(&json!(k)), "{k} in {more}");
    }
    // The folded group is not drawn open (its rows are not on screen), but search still finds them.
    assert!(h.query_by_label("Two-Way").is_none() && h.query_by_label("Bus").is_none(), "the drawing connectors are folded away");
    let hits: Vec<&str> = vizcraft_ui_egui::palette::for_app(h.state()).search("chen").iter().map(|i| i.id).collect();
    assert!(hits.contains(&"er.chenRelationship"), "{hits:?}");
    // Switching the notation swaps what is out of the way.
    run(&mut h, "variant.set", json!({"variant": "er.chen"}));
    let r = h.state_mut().run("ui.palette", json!({})).unwrap();
    assert_eq!(items_of(&r, "connectors"), ["er.chenRelationship", "er.chenIdentifying"]);
    assert!(items_of(&r, "moreConnectors").contains(&"er.relationship".to_string()));
}

#[test]
fn the_picker_and_the_insert_menus_offer_the_viewpoints_types() {
    let mut h = harness(vizcraft_engine::Document::new());
    run(&mut h, "template.new", json!({"template": "uml.classDiagram"}));
    let shapes = |h: &Harness<'static, CraftApp>| -> Vec<String> {
        vizcraft_ui_egui::insert_picker::items(h.state())
            .into_iter()
            .filter(|i| matches!(i.kind, vizcraft_ui_egui::insert_picker::ItemKind::Shape { .. }) && i.id.starts_with("uml."))
            .map(|i| i.id)
            .collect()
    };
    let class = shapes(&h);
    assert!(class.contains(&"uml.Class".to_string()), "{class:?}");
    assert!(!class.iter().any(|t| t == "uml.Lifeline" || t == "uml.State"), "{class:?}");
    // With "Only this viewpoint" off the picker offers the whole pack, as the pane does.
    run(&mut h, "ui.palette", json!({"viewpointFilter": false}));
    assert!(shapes(&h).len() > class.len());
}
