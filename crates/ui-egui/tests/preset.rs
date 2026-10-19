//! The workspace presets (`ui.preset`): the engineering Home arrangement, rulers and panes, the
//! office look restored, junk refused, the first Create from the gallery choosing one, and
//! `paint_master` drawing a path glyph's own outline.

use egui::vec2;
use egui_kittest::Harness;
use serde_json::{Value, json};
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services, icons, ribbon, widgets};

fn app() -> CraftApp {
    CraftApp::new(Session::new(vizcraft_engine::Document::new()), Services::default())
}

fn home_groups(a: &CraftApp) -> Vec<&'static str> {
    let tabs = ribbon::tabs(a);
    tabs.iter().find(|t| t.name == "Home").map(|t| t.groups.iter().map(|g| g.label).collect()).unwrap_or_default()
}

#[test]
fn engineering_arranges_home_turns_rulers_off_and_puts_the_model_tree_first() {
    let mut a = app();
    let office = home_groups(&a);
    let r = a.run("ui.preset", json!({"name": "engineering"})).unwrap();
    assert_eq!(r["preset"], "engineering");
    assert_eq!(home_groups(&a), ["Clipboard", "Text", "Insert", "Layout", "Check", "Arrange", "Styles", "Tools"]);
    assert!(!a.session.view.aids.rulers);
    assert_eq!(a.ui.panes.left.first().map(String::as_str), Some("navigation"));
    assert!(!a.ui.panes.collapsed.contains(&"navigation".to_string()));
    assert_eq!(a.ui.panes.side_of("shapeData"), Some("right"));
    assert_eq!(a.ui.tab, "Home");
    // Each setting stays the person's.
    a.run("view.rulers", json!({"value": true})).unwrap();
    assert!(a.session.view.aids.rulers);
    assert_eq!(a.run("ui.preset", json!({})).unwrap()["preset"], "engineering");

    let r = a.run("ui.preset", json!({"name": "office"})).unwrap();
    assert_eq!(r["preset"], "office");
    assert_eq!(home_groups(&a), office, "today's Home tab");
    assert!(a.session.view.aids.rulers);
    let fresh = vizcraft_ui_egui::panes::PaneLayout::default();
    assert_eq!(&a.ui.panes.left[..fresh.left.len()], &fresh.left[..]);
    assert!(a.ui.panes.collapsed.contains(&"navigation".to_string()), "navigation folded as on a fresh install");
}

#[test]
fn an_unknown_preset_is_refused_and_changes_nothing() {
    let mut a = app();
    let before = a.prefs();
    assert!(a.run("ui.preset", json!({"name": "cad"})).is_err());
    assert_eq!(a.prefs(), before);
    assert!(a.ui.preset.is_empty(), "no preset chosen draws the office look");
    assert_eq!(home_groups(&a)[1], "Font");
}

#[test]
fn every_engineering_home_control_runs_a_command_with_a_drawn_icon() {
    let mut a = app();
    a.run("ui.preset", json!({"name": "engineering"})).unwrap();
    let tabs = ribbon::tabs(&a);
    let home = tabs.iter().find(|t| t.name == "Home").unwrap();
    for id in home.ids() {
        assert!(widgets::exists(&a, id), "{id} is neither a registry command nor a UI command");
        assert!(!widgets::label_of(&a, id).is_empty(), "{id} has no label");
    }
    let mut used = Vec::new();
    for g in &home.groups {
        for c in &g.controls {
            c.icons(&mut used);
        }
    }
    for icon in used {
        assert!(icons::known(icon), "icon {icon} is not drawn");
    }
}

#[test]
fn the_first_create_from_the_software_chip_turns_engineering_on() {
    let a = app();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1200.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        a,
    );
    h.run_steps(3);
    let run = |h: &mut Harness<'static, CraftApp>, id: &str, p: Value| {
        let r = h.state_mut().run(id, p).unwrap_or_else(|e| panic!("{id}: {e}"));
        h.run_steps(3);
        r
    };
    run(&mut h, "ui.gallery", json!({"audience": "software"}));
    let g = vizcraft_ui_egui::control::inspect(h.state(), &h.ctx)["gallery"].clone();
    assert_eq!(g["workspace"], "engineering", "the Software chip suggests engineering");
    run(&mut h, "ui.gallery", json!({"select": "uml.classDiagram", "create": true}));
    assert_eq!(h.state().ui.preset, "engineering");
    let status = h.state().status_msg.as_ref().map(|(s, _)| s.clone()).unwrap_or_default();
    assert!(status.contains("View > Workspace"), "{status}");
    // Chosen once: the next New page has no Workspace row.
    run(&mut h, "ui.gallery", json!({}));
    let g = vizcraft_ui_egui::control::inspect(h.state(), &h.ctx)["gallery"].clone();
    assert!(g["workspace"].is_null());
}

#[test]
fn paint_master_draws_a_path_glyph_by_its_own_rings() {
    let r = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(18.0, 14.0));
    let masters: Vec<&vizcraft_stencils::Master> = vizcraft_stencils::sets().iter().flat_map(|s| s.masters.iter()).collect();
    let closed = |m: &vizcraft_stencils::Master| icons::master_rings(r, m).iter().filter(|(_, c)| *c).count();
    // A builder glyph (a parametric symbol) or another path glyph with several closed rings.
    let m = masters
        .iter()
        .copied()
        .filter(|m| m.path.is_some())
        .max_by_key(|m| (m.glyph.is_some() && closed(m) > 1, closed(m)))
        .expect("a path glyph ships");
    assert!(closed(m) > 1, "{} draws {} closed rings", m.id, closed(m));
    for (pts, _) in icons::master_rings(r, m) {
        assert!(pts.iter().all(|p| r.expand(1.0).contains(*p)), "{} stays in its cell", m.id);
    }
    // A master without a path draws as its kind did.
    let plain = vizcraft_stencils::master("basic.rectangle").unwrap();
    assert_eq!(closed(plain), 1);
}
