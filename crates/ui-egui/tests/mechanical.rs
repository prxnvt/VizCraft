//! The mechanical, reliability and IDEF0 groups of the notation tab and the Sheet tab's Annotate
//! group, driven under egui_kittest on the packs' examples: each tab fits 1280 points with its
//! icons drawn, the groups show only on their pack's pages, Mobility fills the Analysis dock,
//! Decompose adds a page and the Annotate group inserts and switches symbols. Assertions read the
//! session and the UI state, never pixels.

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::dock::DockTab;
use vizcraft_ui_egui::ribbon::{self, Overflow};
use vizcraft_ui_egui::{CraftApp, Services, icons, widgets};

/// The app at `width` points with example `example` open on tab `tab`.
fn harness(width: f32, example: &str, tab: &str) -> Harness<'static, CraftApp> {
    let mut s = Session::new(Document::new());
    s.run("example.open", &json!({"example": example})).unwrap_or_else(|e| panic!("{example}: {e}"));
    let mut app = CraftApp::new(s, Services::default());
    app.ui.tab = tab.into();
    let mut h = Harness::builder().with_size(vec2(width, 900.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

/// The labels of the groups of tab `name` as the app has it now.
fn groups(app: &CraftApp, name: &str) -> Vec<&'static str> {
    ribbon::tabs(app).into_iter().find(|t| t.name == name && t.visible(app)).map(|t| t.groups.iter().map(|g| g.label).collect()).unwrap_or_default()
}

#[test]
fn each_pack_tab_fits_1280_wide_with_drawn_icons_and_known_commands() {
    for (example, tab, group) in [
        ("mechanical.fourBar", "Mechanical", "Mechanism"),
        ("reliability.pumpFmea", "Reliability", "FMEA"),
        ("reliability.pumpFaultTree", "Reliability", "Reliability Analysis"),
        ("idef0.makeWidget", "IDEF0", "Function Model"),
        ("annotation.bracket", "Sheet", "Annotate"),
    ] {
        let h = harness(1280.0, example, tab);
        let app = h.state();
        assert!(groups(app, tab).contains(&group), "{example}: {tab} has {:?}", groups(app, tab));
        assert!(!Overflow::get(&h.ctx).overflows(), "{example} {tab}: {:?}", Overflow::get(&h.ctx));
        let t = ribbon::tabs(app).into_iter().find(|t| t.name == tab).unwrap();
        for id in t.ids() {
            assert!(widgets::exists(app, id), "{tab}: {id} is not a command");
        }
        let mut used = Vec::new();
        for g in &t.groups {
            for c in &g.controls {
                c.icons(&mut used);
            }
        }
        for icon in used {
            assert!(icons::known(icon), "{tab}: icon {icon} is not drawn");
        }
    }
}

#[test]
fn the_pack_groups_show_only_on_their_pack_pages() {
    let h = harness(1280.0, "mechanical.fourBar", "Mechanical");
    assert!(groups(h.state(), "Mechanical").contains(&"Mechanism"));
    let r = harness(1280.0, "reliability.coolingRbd", "Reliability");
    assert!(!groups(r.state(), "Reliability").contains(&"Mechanism"));
    let f = harness(1280.0, "flowchart.ciPipeline", "Flowchart");
    for g in ["Mechanism", "Reliability Analysis", "FMEA", "Function Model"] {
        assert!(!groups(f.state(), "Flowchart").contains(&g), "{g} on a flowchart page");
    }
}

#[test]
fn mobility_fills_the_analysis_dock() {
    let mut h = harness(1280.0, "mechanical.fourBar", "Mechanical");
    h.get_by_label("Mobility").click();
    h.run_steps(4);
    let app = h.state();
    assert_eq!(app.session.analysis.as_ref().map(|r| r.kind.as_str()), Some("mobility"));
    assert!(app.ui.dock.open && app.ui.dock.tab == DockTab::Analysis);
}

#[test]
fn decompose_adds_a_page() {
    let mut h = harness(1280.0, "idef0.makeWidget", "IDEF0");
    let a0 = h.state().session.doc.pages.iter().position(|p| p.ext.get("idef0.node").map(String::as_str) == Some("A0")).unwrap();
    let pages = h.state().session.doc.pages.iter().filter(|p| !p.is_background).count();
    let b1 = {
        let s = &h.state().session;
        let e = s.doc.model.elements.iter().find(|e| e.name == "Plan job").unwrap().id.clone();
        s.doc.pages[a0].shapes.iter().find(|sh| sh.element.as_deref() == Some(e.as_str())).unwrap().id.clone()
    };
    h.state_mut().run("page.setActive", json!({"page": a0})).unwrap();
    h.state_mut().run("edit.select", json!({"ids": [b1]})).unwrap();
    h.run_steps(2);
    h.get_by_label("Decompose").click();
    h.run_steps(3);
    let doc = &h.state().session.doc;
    assert_eq!(doc.pages.iter().filter(|p| !p.is_background).count(), pages + 1);
    assert_eq!(doc.page().unwrap().name, "A1");
}

#[test]
fn the_sheet_tab_shows_annotate_and_inserts_and_switches_symbols() {
    let mut h = harness(1280.0, "annotation.bracket", "Sheet");
    assert!(groups(h.state(), "Sheet").contains(&"Annotate"));
    let before = h.state().session.doc.page().unwrap().shapes.len();
    h.get_by_label("Control Frame").click();
    h.run_steps(3);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.shapes.len(), before + 1);
    assert_eq!(page.shapes.last().unwrap().stencil, "annotation.fcf");
    h.get_by_label("Annotation Standard").click();
    h.run_steps(2);
    h.get_by_label("ASME (Y14.5, AWS A2.4, Y14.36)").click();
    h.run_steps(3);
    let s = &h.state().session;
    let fcf = s.doc.model.elements.iter().find(|e| e.name == "Hole position").unwrap();
    assert_eq!(fcf.prop("standard").and_then(|v| v.as_str()), Some("asmeY14_5"));
}
