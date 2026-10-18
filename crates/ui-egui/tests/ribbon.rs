//! The ribbon kit: the data-driven tabs resolve to commands, buttons run them, collapsing hides
//! the body and shows it as a popup, contextual tabs come and go, icons stay inside their box.

use egui::{Rect, Shape, pos2, vec2};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::ribbon::{self, Control, Group, Overflow, Tab};
use vizcraft_ui_egui::theme::Tokens;
use vizcraft_ui_egui::{CraftApp, Services, Tool, UI_COMMANDS, icons, widgets};

type Harness = egui_kittest::Harness<'static, CraftApp>;

fn app() -> CraftApp {
    CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default())
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

/// The app's default window width, at which the Home tab fits without scrolling.
const DEFAULT_W: f32 = 1440.0;

fn harness() -> Harness {
    harness_at(DEFAULT_W, "Home")
}

#[test]
fn every_control_id_resolves_to_a_command() {
    let a = app();
    let tabs = ribbon::tabs(&a);
    for name in ["Home", "Insert", "Design", "View", "Help", "Connector"] {
        assert!(tabs.iter().any(|t| t.name == name), "{name} tab missing");
    }
    for tab in &tabs {
        let ids = tab.ids();
        assert!(!ids.is_empty(), "{} has no controls", tab.name);
        for id in ids {
            assert!(widgets::exists(&a, id), "{}: {id} is neither a registry command nor a UI command", tab.name);
            assert!(!widgets::label_of(&a, id).is_empty(), "{id} has no label");
        }
    }
    // Icons the default tabs use are drawn ones, not the letter-tile fallback.
    let mut used = Vec::new();
    for tab in &tabs {
        for g in &tab.groups {
            for c in &g.controls {
                c.icons(&mut used);
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
    h.state_mut().run("edit.setTitle", json!({"title": "changed"})).unwrap();
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
    h.get_by_label("View tab").click();
    h.run_steps(3);
    assert!(ribbon::popup_open(&h.ctx));
    h.get_by_label("Dark Mode").click();
    h.run_steps(3);
    assert!(!ribbon::popup_open(&h.ctx));
    // Clicking the open tab again hides the popup.
    h.get_by_label("View tab").click();
    h.run_steps(3);
    assert!(ribbon::popup_open(&h.ctx));
    h.get_by_label("View tab").click();
    h.run_steps(3);
    assert!(!ribbon::popup_open(&h.ctx));
    assert!(h.state().ui.ribbon_collapsed);
}

#[test]
fn contextual_tabs_show_only_while_their_condition_holds() {
    let mut h = harness();
    assert!(h.query_by_label("Connector tab").is_none());
    // A shape selected is not enough: the Connector tab is for connectors.
    let shape = first_shape(&h);
    h.state_mut().session.sel = vizcraft_engine::Selection::single(shape);
    h.run_steps(3);
    assert!(h.query_by_label("Connector tab").is_none());
    let conn = first_connector(&h);
    h.state_mut().session.sel = vizcraft_engine::Selection::single(conn.clone());
    h.run_steps(3);
    assert!(h.query_by_label("Connector tab").is_some());
    h.get_by_label("Connector tab").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "Connector");
    // The route buttons run connector.setRoute on the selected connector.
    h.get_by_label("Curved").click();
    h.run_steps(3);
    let route = |h: &Harness| h.state().session.doc.page().and_then(|p| p.connector(&conn)).map(|c| c.route);
    assert_eq!(route(&h), Some(vizcraft_diagram::Route::Curved));
    h.get_by_label("Straight").click();
    h.run_steps(3);
    assert_eq!(route(&h), Some(vizcraft_diagram::Route::Straight));
    // When the condition stops holding the strip falls back to the first tab's body.
    h.state_mut().session.sel = vizcraft_engine::Selection::empty();
    h.run_steps(3);
    assert!(h.query_by_label("Connector tab").is_none());
    assert!(h.query_by_label("Undo").is_some(), "Home shows while the chosen tab is hidden");
}

fn first_shape(h: &Harness) -> String {
    h.state().session.doc.page().and_then(|p| p.shapes.first()).map(|s| s.id.clone()).unwrap()
}

fn first_connector(h: &Harness) -> String {
    h.state().session.doc.page().and_then(|p| p.connectors.first()).map(|c| c.id.clone()).unwrap()
}

#[test]
fn bold_and_italic_toggle_the_selected_shapes_style() {
    let mut h = harness();
    let id = first_shape(&h);
    let style = |h: &Harness, id: &str| h.state().session.doc.page().and_then(|p| p.shape(id)).map(|s| s.style.clone()).unwrap();
    assert!(!style(&h, &id).bold);
    // Nothing selected: the click is refused with a status message and nothing changes.
    let rev = h.state().session.rev();
    h.get_by_label("Bold").click();
    h.run_steps(3);
    assert_eq!(h.state().session.rev(), rev);
    assert!(h.state().status_msg.is_some(), "the status bar says why");
    h.state_mut().session.sel = vizcraft_engine::Selection::single(id.clone());
    h.run_steps(3);
    h.get_by_label("Bold").click();
    h.run_steps(3);
    assert!(style(&h, &id).bold, "Bold set the style");
    assert!(h.get_by_label("Bold").accesskit_node().toggled() == Some(egui::accesskit::Toggled::True), "Bold shows checked");
    h.get_by_label("Bold").click();
    h.run_steps(3);
    assert!(!style(&h, &id).bold, "a second click clears it");
    h.get_by_label("Italic").click();
    h.run_steps(3);
    assert!(style(&h, &id).italic);
    // Rotate right turns the shape a quarter turn from where it is.
    let rotate = |h: &mut Harness, entry: &str| {
        h.get_by_label("Rotate").click();
        h.run_steps(3);
        // Menu items carry an empty shortcut column after the label.
        h.get_by_label(&format!("{entry} ")).click();
        h.run_steps(3);
    };
    rotate(&mut h, "Rotate Right 90");
    assert_eq!(style_angle(&h, &id), 90.0);
    for _ in 0..2 {
        rotate(&mut h, "Rotate Left 90");
    }
    assert_eq!(style_angle(&h, &id), 270.0);
}

fn style_angle(h: &Harness, id: &str) -> f64 {
    h.state().session.doc.page().and_then(|p| p.shape(id)).map(|s| s.angle_deg).unwrap()
}

#[test]
fn tool_toggles_set_the_canvas_tool() {
    let mut h = harness();
    assert_eq!(h.state().tool, Tool::Pointer);
    assert!(h.get_by_label("Pointer").accesskit_node().toggled() == Some(egui::accesskit::Toggled::True));
    h.get_by_label("Connector").click();
    h.run_steps(3);
    assert_eq!(h.state().tool, Tool::Connector);
    assert!(h.get_by_label("Connector").accesskit_node().toggled() == Some(egui::accesskit::Toggled::True));
    h.get_by_label("Text").click();
    h.run_steps(3);
    assert_eq!(h.state().tool, Tool::Text);
    h.get_by_label("Pointer").click();
    h.run_steps(3);
    assert_eq!(h.state().tool, Tool::Pointer);
}

#[test]
fn an_insert_gallery_button_adds_that_master_on_the_page() {
    let mut h = harness_at(1200.0, "Insert");
    // The Shapes pane has buttons with the same names; close it so the labels find the ribbon's.
    h.state_mut().run("ui.pane", serde_json::json!({"name": "shapes", "value": false})).unwrap();
    h.run_steps(2);
    let count = |h: &Harness| h.state().session.doc.page().map(|p| p.shapes.len()).unwrap();
    let before = count(&h);
    h.get_by_label("Decision").click();
    h.run_steps(3);
    assert_eq!(count(&h), before + 1);
    let page = h.state().session.doc.page().unwrap();
    let added = page.shapes.last().unwrap();
    assert_eq!(added.stencil, "flowchart.decision");
    assert!(page.rect().contains_rect(&added.frame()), "the new shape is on the page: {:?}", added.frame());
    assert!(h.state().session.sel.contains(&added.id), "the new shape is selected");
    // A second insert of the same master steps aside instead of hiding under the first.
    let first = (added.x, added.y);
    h.get_by_label("Decision").click();
    h.run_steps(3);
    let page = h.state().session.doc.page().unwrap();
    let second = page.shapes.last().unwrap();
    assert_ne!((second.x, second.y), first);
    // The Insert tab's Connector and Text buttons pick the tool.
    h.get_by_label("Connector").click();
    h.run_steps(3);
    assert_eq!(h.state().tool, Tool::Connector);
}

#[test]
fn colour_buttons_set_the_fill_from_the_palette() {
    let mut h = harness();
    let id = first_shape(&h);
    h.state_mut().session.sel = vizcraft_engine::Selection::single(id.clone());
    h.run_steps(3);
    h.get_by_label("Fill").click();
    h.run_steps(3);
    h.get_by_label("Fill #2f7d6d").click();
    h.run_steps(3);
    let fill = h.state().session.doc.page().and_then(|p| p.shape(&id)).map(|s| s.style.fill.to_hex()).unwrap();
    assert!(fill.starts_with("#2f7d6d"), "{fill}");
}

#[test]
fn every_shape_kind_draws_inside_its_box() {
    for kind in vizcraft_diagram::ShapeKind::ALL {
        for (w, h) in [(120.0, 70.0), (50.0, 50.0), (90.0, 100.0), (140.0, 50.0)] {
            let ctx = egui::Context::default();
            let t = Tokens::light();
            let r = Rect::from_min_size(pos2(20.0, 20.0), vec2(16.0, 16.0));
            let mut out = ctx.run_ui(Default::default(), |ui| icons::paint_kind(ui.painter(), r, kind, w, h, t.icon, t.accent));
            out.textures_delta.clear();
            assert!(!out.shapes.is_empty(), "{kind:?} draws nothing");
            for s in out.shapes {
                let b = s.shape.visual_bounding_rect();
                assert!(r.expand(icons::stroke_width(16.0)).contains_rect(b), "{kind:?} {w}x{h}: {b:?} spills out of {r:?}");
            }
        }
    }
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
    let h = harness_at(DEFAULT_W, "Home");
    assert!(!Overflow::get(&h.ctx).overflows());
    assert!(h.query_by_label("Scroll ribbon right").is_none());
}

#[test]
fn split_buttons_run_the_main_part_and_open_a_menu_from_the_arrow() {
    let a = app();
    let split_ctl = Control::Split { id: "edit.redo", icon: "redo", big: true, menu: vec![("Redo Once", "edit.redo", json!({}))] };
    let mut h = egui_kittest::Harness::builder().with_size(vec2(300.0, 300.0)).build_ui_state(
        move |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            split_ctl.show(app, ui);
        },
        a,
    );
    h.run_steps(5);
    assert!(h.query_by_label("Redo").is_some());
    assert!(h.query_by_label("Redo options").is_some());
    h.get_by_label("Redo options").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("Redo Once").is_some(), "the arrow opens the menu");
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

/// `VIZCRAFT_ICON_SHEET=/path.png` saves every icon at 16, 20 and 32 px in both themes for review.
#[test]
fn icon_sheet() {
    let Ok(path) = std::env::var("VIZCRAFT_ICON_SHEET") else { return };
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

fn connectors(h: &Harness) -> Vec<vizcraft_diagram::Connector> {
    h.state().session.doc.page().map(|p| p.connectors.clone()).unwrap_or_default()
}

#[test]
fn design_tab_toggles_avoid_shapes_and_line_jumps() {
    let mut h = harness_at(1200.0, "Design");
    assert!(!connectors(&h).is_empty());
    // Nothing selected: the toggles act on every connector.
    h.get_by_label("Avoid Shapes").click();
    h.run_steps(3);
    assert!(connectors(&h).iter().all(|c| c.avoid && c.route == vizcraft_diagram::Route::Orthogonal));
    assert_eq!(h.state().session.undo_label(), Some("Avoid Shapes"));
    h.get_by_label("Avoid Shapes").click();
    h.run_steps(3);
    assert!(connectors(&h).iter().all(|c| !c.avoid && c.waypoints.is_empty()), "a second click turns it off");
    // With one connector selected, only that one changes.
    let id = first_connector(&h);
    h.state_mut().run("edit.select", json!({"ids": [id.clone()]})).unwrap();
    h.run_steps(2);
    h.get_by_label("Line Jumps").click();
    h.run_steps(3);
    for c in connectors(&h) {
        assert_eq!(c.jumps, c.id == id, "{}", c.id);
    }
}

#[test]
fn design_tab_relayout_menu_and_route_button_run_their_commands() {
    let mut h = harness_at(1200.0, "Design");
    let shapes = |h: &Harness| h.state().session.doc.page().map(|p| p.shapes.clone()).unwrap_or_default();
    let before = shapes(&h);
    h.get_by_label("Re-Layout").click();
    h.run_steps(3);
    h.get_by_label_contains("Tree, Left to Right").click();
    h.run_steps(3);
    assert_ne!(shapes(&h), before, "the menu entry ran layout.full");
    assert_eq!(h.state().session.undo_label(), Some("Re-Layout"));
    h.get_by_label("Route Connectors").click();
    h.run_steps(3);
    assert!(connectors(&h).iter().all(|c| c.avoid));
}
