//! The ribbon kit: the data-driven tabs resolve to commands, buttons run them, collapsing hides
//! the body and shows it as a popup, contextual tabs come and go, icons stay inside their box.

use egui::{Rect, Shape, pos2, vec2};
use egui_kittest::kittest::Queryable as _;
use protocraft_engine::Session;
use protocraft_ui_egui::ribbon::{self, Control, Group, Overflow, Tab};
use protocraft_ui_egui::theme::Tokens;
use protocraft_ui_egui::{CraftApp, Services, UI_COMMANDS, icons, widgets};
use serde_json::json;

type Harness = egui_kittest::Harness<'static, CraftApp>;

fn app() -> CraftApp {
    CraftApp::new(Session::new(protocraft_engine::sample::sample_document()), Services::default())
}

fn harness_at(width: f32, tab: &str) -> Harness {
    let mut a = app();
    a.ui.tab = tab.into();
    // The History pane has its own Undo and Redo buttons; these tests look for the ribbon's.
    a.ui.panes.close("history");
    let mut h = egui_kittest::Harness::builder().with_size(vec2(width, 700.0)).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        a,
    );
    // The first two frames install the fonts and draw nothing.
    h.run_steps(5);
    h
}

fn harness() -> Harness {
    harness_at(1200.0, "Home")
}

#[test]
fn every_control_id_resolves_to_a_command() {
    let a = app();
    let tabs = ribbon::tabs(&a);
    assert!(tabs.iter().any(|t| t.name == "Home") && tabs.iter().any(|t| t.name == "View"));
    for tab in &tabs {
        let ids = tab.ids();
        assert!(!ids.is_empty(), "{} has no controls", tab.name);
        for id in ids {
            assert!(widgets::exists(&a, id), "{}: {id} is neither a registry command nor a UI command", tab.name);
            assert!(!widgets::label_of(&a, id).is_empty(), "{id} has no label");
        }
    }
    // Icons the default tabs use are drawn ones, not the letter-tile fallback.
    fn icons_of(c: &Control, out: &mut Vec<&'static str>) {
        match c {
            Control::Big { icon, .. }
            | Control::Small { icon, .. }
            | Control::Toggle { icon, .. }
            | Control::Split { icon, .. }
            | Control::Menu { icon, .. } => out.push(icon),
            Control::Stack(items) => items.iter().for_each(|c| icons_of(c, out)),
            Control::Combo { .. } | Control::Gap | Control::Custom(_) => {}
        }
    }
    let mut used = Vec::new();
    for tab in &tabs {
        for g in &tab.groups {
            for c in &g.controls {
                icons_of(c, &mut used);
            }
        }
    }
    for icon in used {
        assert!(icons::known(icon), "icon {icon} is not drawn");
    }
    // UI commands are also valid control ids.
    for (id, ..) in UI_COMMANDS {
        assert!(widgets::exists(&a, id));
    }
}

#[test]
fn clicking_undo_after_a_change_runs_edit_undo() {
    let mut h = harness();
    h.state_mut().run("edit.setText", json!({"text": "changed"})).unwrap();
    h.run_steps(2);
    assert!(h.state().session.can_undo());
    let rev = h.state().session.rev();
    h.get_by_label("Undo").click();
    h.run_steps(3);
    assert!(!h.state().session.can_undo(), "the big Undo button ran edit.undo");
    assert_ne!(h.state().session.rev(), rev);
    // Disabled now: clicking again changes nothing.
    h.get_by_label("Undo").click();
    h.run_steps(3);
    assert!(h.state().session.can_redo());
}

#[test]
fn toggles_show_their_state_and_run_their_command() {
    let mut h = harness_at(1200.0, "View");
    assert!(h.state().session.view.nav_pane);
    h.get_by_label("Navigation Pane").click();
    h.run_steps(3);
    assert!(!h.state().session.view.nav_pane);
    h.get_by_label("Dark Mode").click();
    h.run_steps(3);
    assert!(h.state().session.view.dark_mode);
}

#[test]
fn collapsing_hides_the_body_and_a_tab_click_shows_it_as_a_popup() {
    let mut h = harness();
    assert!(h.query_by_label("Undo").is_some());
    h.state_mut().run("ui.collapseRibbon", json!({})).unwrap();
    h.run_steps(3);
    assert!(h.state().ui.ribbon_collapsed);
    assert!(h.query_by_label("Undo").is_none(), "a collapsed ribbon has no body");
    assert!(!ribbon::popup_open(&h.ctx));
    // Clicking a tab shows that tab as a popup; the ribbon stays collapsed.
    h.get_by_label("View tab").click();
    h.run_steps(3);
    assert!(h.state().ui.ribbon_collapsed);
    assert_eq!(h.state().ui.tab, "View");
    assert!(ribbon::popup_open(&h.ctx));
    assert!(h.query_by_label("Navigation Pane").is_some(), "the popup shows the View tab");
    // Clicking in the document closes it.
    let outside = pos2(600.0, 500.0);
    h.input_mut().events.push(egui::Event::PointerMoved(outside));
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: outside,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    h.run_steps(1);
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: outside,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    h.run_steps(3);
    assert!(!ribbon::popup_open(&h.ctx), "a click outside closes the popup");
    assert!(h.query_by_label("Navigation Pane").is_none());
    // Running a command from the popup closes it too.
    h.get_by_label("Home tab").click();
    h.run_steps(3);
    assert!(ribbon::popup_open(&h.ctx));
    h.get_by_label("Note").click();
    h.run_steps(3);
    assert!(!ribbon::popup_open(&h.ctx));
    // Clicking the open tab again hides the popup.
    h.get_by_label("Home tab").click();
    h.run_steps(3);
    assert!(ribbon::popup_open(&h.ctx));
    h.get_by_label("Home tab").click();
    h.run_steps(3);
    assert!(!ribbon::popup_open(&h.ctx));
    assert!(h.state().ui.ribbon_collapsed);
}

#[test]
fn contextual_tabs_show_only_while_their_condition_holds() {
    let mut h = harness();
    assert!(h.query_by_label("Selection tab").is_none());
    h.state_mut().run("edit.select", json!({"note": 0, "start": 0, "end": 3})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Selection tab").is_some());
    h.get_by_label("Selection tab").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "Selection");
    assert!(h.query_by_label("Select All").is_some());
    // When the condition stops holding the strip falls back to the first tab's body.
    h.state_mut().run("edit.select", json!({"note": 0, "start": 0, "end": 0})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Selection tab").is_none());
    assert!(h.query_by_label("Undo").is_some(), "Home shows while the chosen tab is hidden");
}

#[test]
fn a_narrow_window_scrolls_the_groups_with_arrows() {
    let many: Vec<Tab> = vec![Tab::new(
        "Wide",
        (0..12)
            .map(|_| {
                Group::new("Zoom", vec![Control::Big { id: "view.zoomIn", icon: "zoomIn" }, Control::Big { id: "view.zoomOut", icon: "zoomOut" }])
            })
            .collect(),
    )];
    let mut a = app();
    a.ui.tab = "Wide".into();
    let tabs: &'static Vec<Tab> = Box::leak(Box::new(many));
    let mut h = egui_kittest::Harness::builder().with_size(vec2(500.0, 400.0)).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            if ui.ctx().cumulative_pass_nr() < 2 {
                return;
            }
            ribbon::show_tabs(app, ui, tabs);
        },
        a,
    );
    h.run_steps(5);
    let o = Overflow::get(&h.ctx);
    assert!(o.overflows(), "{o:?}");
    assert_eq!(o.offset, 0.0);
    assert!(h.query_by_label("Scroll ribbon left").is_none());
    h.get_by_label("Scroll ribbon right").click();
    h.run_steps(3);
    assert!(Overflow::get(&h.ctx).offset > 0.0);
    h.get_by_label("Scroll ribbon left").click();
    h.run_steps(3);
    assert!(Overflow::get(&h.ctx).offset < 0.5);
    // A wide window shows no arrows.
    let h = harness_at(1200.0, "Home");
    assert!(!Overflow::get(&h.ctx).overflows());
    assert!(h.query_by_label("Scroll ribbon right").is_none());
}

#[test]
fn split_buttons_run_the_main_part_and_open_a_menu_from_the_arrow() {
    let mut h = harness_at(1200.0, "Insert");
    let notes = h.state().session.doc.notes.len();
    h.get_by_label("Note").click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc.notes.len(), notes + 1, "the main part inserts a note");
    h.get_by_label("Note options").click();
    h.run_steps(3);
    // Menu items carry their shortcut in the label.
    assert!(h.query_by_label_contains("Checklist").is_some(), "the arrow opens the menu");
    h.get_by_label_contains("Checklist").click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc.notes.len(), notes + 2);
}

#[test]
fn file_tab_opens_the_file_page() {
    let mut h = harness();
    h.get_by_label("File tab").click();
    h.run_steps(3);
    assert!(h.state().backstage_open());
}

fn shapes_of(name: &str, size: f32, dark: bool) -> Vec<Shape> {
    let ctx = egui::Context::default();
    let t = if dark { Tokens::dark() } else { Tokens::light() };
    let mut output = ctx
        .run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0))), ..Default::default() }, |ui| {
            icons::paint(ui.painter(), Rect::from_min_size(pos2(20.0, 20.0), vec2(size, size)), name, t.icon, t.accent)
        });
    output.textures_delta.clear();
    output.shapes.into_iter().map(|s| s.shape).collect()
}

#[test]
fn icons_draw_inside_their_box_at_every_size() {
    for dark in [false, true] {
        for name in icons::NAMES {
            for size in [16.0, 20.0, 32.0] {
                let shapes = shapes_of(name, size, dark);
                assert!(!shapes.is_empty(), "{name} draws nothing");
                assert!(!shapes.iter().any(|s| matches!(s, Shape::Text(_))), "{name} fell back to the letter tile");
                let slack = icons::stroke_width(size);
                let box_ = Rect::from_min_size(pos2(20.0, 20.0), vec2(size, size)).expand(slack);
                for s in &shapes {
                    let r = s.visual_bounding_rect();
                    if r.is_positive() {
                        assert!(box_.contains_rect(r), "{name} at {size}: {r:?} spills out of {box_:?}");
                    }
                }
            }
        }
    }
    // The stroke grows with the size, but less than linearly.
    assert!(icons::stroke_width(16.0) < icons::stroke_width(20.0));
    assert!(icons::stroke_width(32.0) < 2.0 * icons::stroke_width(16.0));
    // Unknown names draw the letter tile instead of nothing.
    assert!(shapes_of("nothing", 20.0, false).iter().any(|s| matches!(s, Shape::Text(_))));
}

/// `PROTOCRAFT_ICON_SHEET=/path.png` saves every icon at 16, 20 and 32 px in both themes for review.
#[test]
fn icon_sheet() {
    let Ok(path) = std::env::var("PROTOCRAFT_ICON_SHEET") else { return };
    let n = icons::NAMES.len() as f32;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(n * 40.0 + 16.0, 260.0)).with_pixels_per_point(2.0).build_ui(|ui| {
        let full = ui.max_rect();
        for (row, dark) in [(0usize, false), (1, true)] {
            let t = if dark { Tokens::dark() } else { Tokens::light() };
            let band = Rect::from_min_size(pos2(full.left(), full.top() + row as f32 * 130.0), vec2(full.width(), 130.0));
            ui.painter().rect_filled(band, 0.0, t.ribbon);
            for (i, name) in icons::NAMES.iter().enumerate() {
                let x = band.left() + 8.0 + i as f32 * 40.0;
                icons::paint(ui.painter(), Rect::from_min_size(pos2(x, band.top() + 8.0), vec2(32.0, 32.0)), name, t.icon, t.accent);
                icons::paint(ui.painter(), Rect::from_min_size(pos2(x + 6.0, band.top() + 52.0), vec2(20.0, 20.0)), name, t.icon, t.accent);
                icons::paint(ui.painter(), Rect::from_min_size(pos2(x + 8.0, band.top() + 84.0), vec2(16.0, 16.0)), name, t.icon, t.accent);
                ui.painter().text(
                    pos2(x + 16.0, band.top() + 115.0),
                    egui::Align2::CENTER_CENTER,
                    &name[..name.len().min(6)],
                    egui::FontId::proportional(7.0),
                    t.text_dim,
                );
            }
        }
    });
    h.run_steps(2);
    h.render().unwrap().save(path).unwrap();
}
