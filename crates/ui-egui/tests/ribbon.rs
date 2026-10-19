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
        h.get_by_label(entry).click();
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
    // The Inspector shows the selected shape's text, which is also "Decision"; close it too.
    h.state_mut().run("ui.pane", serde_json::json!({"name": "shapes", "value": false})).unwrap();
    h.state_mut().run("ui.pane", serde_json::json!({"name": "shapeData", "value": false})).unwrap();
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
    let split_ctl =
        Control::Split { id: "edit.redo", icon: "redo", big: true, menu: vec![ribbon::MenuEntry::new("Redo Once", "edit.redo", json!({}))] };
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
fn design_tab_line_jumps_toggle_acts_on_the_selection_or_the_page() {
    let mut h = harness_at(1280.0, "Design");
    assert!(!connectors(&h).is_empty());
    // Nothing selected: the toggle acts on every connector, and a second click turns it off.
    h.get_by_label("Line Jumps").click();
    h.run_steps(3);
    assert!(connectors(&h).iter().all(|c| c.jumps));
    h.get_by_label("Line Jumps").click();
    h.run_steps(3);
    assert!(connectors(&h).iter().all(|c| !c.jumps));
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

/// The rows of the open dropdown from `ui.inspect`, as `(label, checked)`.
fn open_menu_rows(h: &Harness) -> Vec<(String, serde_json::Value)> {
    let report = vizcraft_ui_egui::control::inspect(h.state(), &h.ctx);
    report["menu"]["rows"]
        .as_array()
        .map(|rows| rows.iter().map(|r| (r["label"].as_str().unwrap_or_default().to_string(), r["checked"].clone())).collect())
        .unwrap_or_default()
}

#[test]
fn inspect_lists_the_orientation_rows_with_their_checked_state() {
    let mut h = harness_at(1200.0, "Design");
    assert!(vizcraft_ui_egui::control::inspect(h.state(), &h.ctx)["menu"].is_null(), "no dropdown is open yet");
    h.get_by_label("Orientation").click();
    h.run_steps(4);
    let report = vizcraft_ui_egui::control::inspect(h.state(), &h.ctx);
    assert_eq!(report["menu"]["title"], "Orientation");
    // The sample page is landscape.
    assert_eq!(open_menu_rows(&h), vec![("Portrait".to_string(), json!(false)), ("Landscape".to_string(), json!(true))]);
    let row = &report["menu"]["rows"][0];
    assert_eq!(row["id"], "page.setup");
    assert_eq!(row["enabled"], true);
    // Choosing Portrait runs the command, closes the menu and moves the mark on reopening.
    h.get_by_label("Portrait").click();
    h.run_steps(4);
    assert!(h.state().session.doc.page().is_some_and(|p| p.height > p.width));
    h.get_by_label("Orientation").click();
    h.run_steps(4);
    assert_eq!(open_menu_rows(&h), vec![("Portrait".to_string(), json!(true)), ("Landscape".to_string(), json!(false))]);
    // The row's accessibility info says it is selected.
    assert_eq!(h.get_by_label("Portrait").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
    assert_eq!(h.get_by_label("Landscape").accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
}

#[test]
fn the_size_menu_marks_the_preset_that_matches_the_page_and_has_no_current_suffix() {
    let mut h = harness_at(1200.0, "Design");
    h.state_mut().run("page.setup", json!({"width": 826.8, "height": 1169.3})).unwrap();
    h.run_steps(3);
    h.get_by_label("Size").click();
    h.run_steps(4);
    let rows = open_menu_rows(&h);
    assert!(rows.iter().all(|(label, _)| !label.contains("(current)")), "{rows:?}");
    assert!(rows.contains(&("A4".to_string(), json!(true))), "{rows:?}");
    assert!(rows.contains(&("Letter".to_string(), json!(false))), "{rows:?}");
    assert!(rows.contains(&("Fit to Drawing".to_string(), serde_json::Value::Null)), "plain actions carry no mark: {rows:?}");
}

#[test]
fn the_connector_style_menu_marks_the_route_every_connector_has() {
    let mut h = harness_at(1200.0, "Design");
    h.state_mut().run("design.setConnectorStyle", json!({"style": "straight"})).unwrap();
    h.run_steps(3);
    h.get_by_label("Connector Style").click();
    h.run_steps(4);
    assert_eq!(
        open_menu_rows(&h),
        vec![("Right Angle".to_string(), json!(false)), ("Straight".to_string(), json!(true)), ("Curved".to_string(), json!(false))]
    );
}

#[test]
fn a_combo_list_marks_the_current_item() {
    let mut h = harness_at(1200.0, "View");
    h.get_by_label("Zoom Level options").click();
    h.run_steps(4);
    let rows = open_menu_rows(&h);
    let marked: Vec<&str> = rows.iter().filter(|(_, c)| *c == json!(true)).map(|(l, _)| l.as_str()).collect();
    assert_eq!(marked, vec!["100%"], "{rows:?}");
}

fn page(h: &Harness) -> vizcraft_diagram::Page {
    h.state().session.doc.page().cloned().unwrap()
}

/// Open the dropdown `name` and return its rows.
fn open_menu(h: &mut Harness, name: &str) -> Vec<(String, serde_json::Value)> {
    h.get_by_label(name).click();
    h.run_steps(4);
    open_menu_rows(h)
}

/// The labels of the rows marked checked.
fn marked(rows: &[(String, serde_json::Value)]) -> Vec<&str> {
    rows.iter().filter(|(_, c)| *c == json!(true)).map(|(l, _)| l.as_str()).collect()
}

#[test]
fn home_fits_1280_wide_without_a_scroll_arrow() {
    let h = harness_at(1280.0, "Home");
    let o = Overflow::get(&h.ctx);
    assert!(!o.overflows(), "{o:?}");
    assert!(h.query_by_label("Scroll ribbon right").is_none());
    // Delete moved beside Cut and Copy; Undo and Redo are a small stack.
    for label in ["Delete", "Undo", "Redo", "Format Painter"] {
        assert!(h.query_by_label(label).is_some(), "{label} is missing from Home");
    }
}

#[test]
fn every_tab_the_organising_pass_touched_fits_1280_wide() {
    for tab in ["Design", "Insert", "Process", "Generate"] {
        let h = harness_at(1280.0, tab);
        assert!(!Overflow::get(&h.ctx).overflows(), "{tab} overflows at 1280");
    }
    let mut a = app();
    let conn = a.session.doc.page().and_then(|p| p.connectors.first()).map(|c| c.id.clone()).unwrap();
    a.session.sel = vizcraft_engine::Selection::single(conn);
    a.ui.tab = "Connector".into();
    let mut h = egui_kittest::Harness::builder().with_size(vec2(1280.0, 700.0)).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        a,
    );
    h.run_steps(5);
    assert!(!Overflow::get(&h.ctx).overflows(), "Connector overflows at 1280");
}

#[test]
fn the_board_menu_marks_the_pages_board_and_sets_it() {
    let mut h = harness_at(1280.0, "Design");
    h.state_mut().run("page.setup", json!({"board": "infinite"})).unwrap();
    h.run_steps(3);
    let rows = open_menu(&mut h, "Board");
    assert_eq!(rows.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(), ["Infinite", "Fixed Size"]);
    assert_eq!(marked(&rows), ["Infinite"]);
    h.get_by_label("Fixed Size").click();
    h.run_steps(4);
    assert_eq!(page(&h).board, vizcraft_diagram::Board::Fixed);
    assert_eq!(marked(&open_menu(&mut h, "Board")), ["Fixed Size"]);
}

#[test]
fn the_avoid_shapes_menu_marks_the_pages_policy_and_sets_it() {
    let mut h = harness_at(1280.0, "Design");
    let rows = open_menu(&mut h, "Avoid Shapes");
    assert_eq!(rows.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(), ["Always", "Never"]);
    assert_eq!(marked(&rows), ["Always"], "the default policy is always");
    h.get_by_label("Never").click();
    h.run_steps(4);
    assert_eq!(page(&h).routing.avoid, vizcraft_diagram::Avoid::Never);
    assert_eq!(h.state().session.undo_label(), Some("Connector Routing"));
    assert_eq!(marked(&open_menu(&mut h, "Avoid Shapes")), ["Never"]);
    h.get_by_label("Always").click();
    h.run_steps(4);
    assert_eq!(page(&h).routing.avoid, vizcraft_diagram::Avoid::Always);
}

#[test]
fn the_straight_lines_menu_marks_the_fallback_and_sets_it() {
    let mut h = harness_at(1280.0, "Design");
    let rows = open_menu(&mut h, "Straight Lines");
    assert_eq!(rows.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(), ["Detour", "Keep Straight"]);
    assert_eq!(marked(&rows), ["Detour"]);
    h.get_by_label("Keep Straight").click();
    h.run_steps(4);
    assert_eq!(page(&h).routing.straight_fallback, vizcraft_diagram::StraightFallback::Keep);
    assert_eq!(marked(&open_menu(&mut h, "Straight Lines")), ["Keep Straight"]);
}

#[test]
fn default_kind_lists_the_catalog_and_acts_on_every_connector_when_nothing_is_selected() {
    let mut h = harness_at(1280.0, "Design");
    let rows = open_menu(&mut h, "Default Kind");
    let catalog = vizcraft_stencils::connector_kinds();
    assert!(catalog.len() > 3);
    for k in catalog {
        assert!(rows.iter().any(|(l, _)| l == k.label), "{} is missing from the menu", k.label);
    }
    assert!(marked(&rows).is_empty(), "no kind is shared yet: {:?}", marked(&rows));
    let kind = catalog.iter().find(|k| k.id == "basic.dependency").expect("the basic pack has a dependency kind");
    h.get_by_label(kind.label).click();
    h.run_steps(4);
    assert!(connectors(&h).iter().all(|c| c.kind == kind.id), "every connector took the kind");
    assert_eq!(marked(&open_menu(&mut h, "Default Kind")), [kind.label]);
    // With one connector selected, only that one is changed.
    h.get_by_label(kind.label).click();
    h.run_steps(3);
    let first = first_connector(&h);
    h.state_mut().run("edit.select", json!({"ids": [first.clone()]})).unwrap();
    h.run_steps(2);
    let other = catalog.iter().find(|k| k.id == "basic.bus").expect("the basic pack has a bus kind");
    open_menu(&mut h, "Default Kind");
    h.get_by_label(other.label).click();
    h.run_steps(4);
    for c in connectors(&h) {
        assert_eq!(c.kind == other.id, c.id == first, "{}", c.id);
    }
}

fn connector_tab(width: f32) -> (Harness, String) {
    let mut a = app();
    let conn = a.session.doc.page().and_then(|p| p.connectors.first()).map(|c| c.id.clone()).unwrap();
    a.session.sel = vizcraft_engine::Selection::single(conn.clone());
    a.ui.tab = "Connector".into();
    a.ui.panes.close("history");
    let mut h = egui_kittest::Harness::builder().with_size(vec2(width, 700.0)).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        a,
    );
    h.run_steps(5);
    (h, conn)
}

#[test]
fn the_connector_tab_kind_menu_marks_the_selected_connectors_kind() {
    let (mut h, conn) = connector_tab(1280.0);
    h.state_mut().run("connector.setKind", json!({"ids": [conn.clone()], "kind": "basic.bus"})).unwrap();
    h.run_steps(3);
    let rows = open_menu(&mut h, "Kind");
    let bus = vizcraft_stencils::connector_kind("basic.bus").expect("basic.bus").label;
    assert_eq!(marked(&rows), [bus]);
    let dep = vizcraft_stencils::connector_kind("basic.dependency").expect("basic.dependency").label;
    h.get_by_label(dep).click();
    h.run_steps(4);
    let kind = h.state().session.doc.page().and_then(|p| p.connector(&conn)).map(|c| c.kind.clone());
    assert_eq!(kind.as_deref(), Some("basic.dependency"));
    assert_eq!(marked(&open_menu(&mut h, "Kind")), [dep]);
}

#[test]
fn the_arrow_menus_offer_all_thirteen_ends_and_mark_the_current_one() {
    let (mut h, conn) = connector_tab(1280.0);
    let ends = |h: &Harness| h.state().session.doc.page().and_then(|p| p.connector(&conn)).map(|c| (c.start_arrow, c.end_arrow)).unwrap();
    // The sample's connectors have no start end and a triangle at the end.
    let rows = open_menu(&mut h, "End Arrow");
    assert_eq!(rows.len(), vizcraft_diagram::Arrow::ALL.len());
    for label in ["None", "Hollow triangle", "Crow's foot with bar", "Crow's foot with circle", "Bar with circle", "Hollow circle"] {
        assert!(rows.iter().any(|(l, _)| l == label), "{label} is missing: {rows:?}");
    }
    assert_eq!(marked(&rows), ["Triangle"]);
    h.get_by_label("Crow's foot with bar").click();
    h.run_steps(4);
    assert_eq!(ends(&h), (vizcraft_diagram::Arrow::None, vizcraft_diagram::Arrow::CrowsFootBar));
    assert_eq!(marked(&open_menu(&mut h, "End Arrow")), ["Crow's foot with bar"]);
    h.get_by_label("Crow's foot with bar").click();
    h.run_steps(3);
    let rows = open_menu(&mut h, "Start Arrow");
    assert_eq!(rows.len(), vizcraft_diagram::Arrow::ALL.len());
    assert_eq!(marked(&rows), ["None"]);
    h.get_by_label("Hollow diamond").click();
    h.run_steps(4);
    assert_eq!(ends(&h).0, vizcraft_diagram::Arrow::HollowDiamond);
    assert_eq!(marked(&open_menu(&mut h, "Start Arrow")), ["Hollow diamond"]);
}

#[test]
fn the_compartment_menu_needs_one_shape_and_adds_the_chosen_kind() {
    let mut h = harness_at(1280.0, "Insert");
    // Nothing selected: the menu does not open.
    h.get_by_label("Compartment").click();
    h.run_steps(4);
    assert!(open_menu_rows(&h).is_empty());
    let shape = first_shape(&h);
    h.state_mut().run("edit.select", json!({"ids": [shape.clone()]})).unwrap();
    h.run_steps(3);
    let rows = open_menu(&mut h, "Compartment");
    assert_eq!(rows.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(), ["Paragraph", "List", "Slot"]);
    let count = |h: &Harness| h.state().session.doc.page().and_then(|p| p.shape(&shape)).map_or(0, |s| s.compartments.len());
    assert_eq!(count(&h), 0);
    h.get_by_label("List").click();
    h.run_steps(4);
    assert_eq!(count(&h), 1);
    let kind = h.state().session.doc.page().and_then(|p| p.shape(&shape)).map(|s| s.compartments[0].kind);
    assert_eq!(kind, Some(vizcraft_diagram::CompartmentKind::List));
    // Two shapes selected: the menu is off again.
    let second = h.state().session.doc.page().map(|p| p.shapes[1].id.clone()).unwrap();
    h.state_mut().run("edit.select", json!({"ids": [shape, second]})).unwrap();
    h.run_steps(3);
    h.get_by_label("Compartment").click();
    h.run_steps(4);
    assert!(open_menu_rows(&h).is_empty());
}

#[test]
fn the_rules_menu_lists_off_and_every_ruleset_with_the_profile_checked() {
    let mut h = harness_at(1280.0, "Process");
    let sets = vizcraft_notation::catalog().rulesets();
    assert!(!sets.is_empty());
    h.state_mut().run("validate.setProfile", json!({"enabled": false})).unwrap();
    h.run_steps(3);
    let rows = open_menu(&mut h, "Rules");
    assert_eq!(rows.len(), sets.len() + 1);
    assert_eq!(rows[0].0, "Off");
    assert_eq!(marked(&rows), ["Off"], "the profile is off");
    let (_, set) = sets.iter().find(|(_, r)| r.id == "drawing.tidy").expect("the drawing pack has a tidy ruleset");
    h.get_by_label(set.label.as_str()).click();
    h.run_steps(4);
    let profile = vizcraft_notation::rules::profile(&h.state().session.doc, vizcraft_notation::catalog());
    assert!(profile.enabled);
    assert_eq!(profile.rulesets, ["drawing.tidy"]);
    assert_eq!(marked(&open_menu(&mut h, "Rules")), [set.label.as_str()]);
    h.get_by_label("Off").click();
    h.run_steps(4);
    assert!(!vizcraft_notation::rules::profile(&h.state().session.doc, vizcraft_notation::catalog()).enabled);
}

#[test]
fn live_checking_toggles_the_profile_and_next_and_previous_issue_run_their_commands() {
    let mut h = harness_at(1280.0, "Process");
    h.state_mut().run("validate.setProfile", json!({"enabled": false})).unwrap();
    h.run_steps(3);
    let toggled = |h: &Harness| h.get_by_label("Live Checking").accesskit_node().toggled();
    assert_eq!(toggled(&h), Some(egui::accesskit::Toggled::False));
    h.get_by_label("Live Checking").click();
    h.run_steps(4);
    assert!(h.state().session.doc.conformance.enabled);
    assert_eq!(toggled(&h), Some(egui::accesskit::Toggled::True));
    h.get_by_label("Live Checking").click();
    h.run_steps(4);
    assert!(!h.state().session.doc.conformance.enabled);
    // The issue buttons run validate.next and validate.previous (no issue: nothing selected).
    for (label, id) in [("Next Issue", "validate.next"), ("Previous Issue", "validate.previous")] {
        assert!(widgets::exists(h.state(), id));
        assert!(h.query_by_label(label).is_some(), "{label} is missing");
        h.get_by_label(label).click();
        h.run_steps(3);
    }
}

#[test]
fn the_new_controls_are_listed_with_their_commands() {
    let a = app();
    let tabs = ribbon::tabs(&a);
    let ids = |name: &str| tabs.iter().find(|t| t.name == name).map(|t| t.ids()).unwrap_or_default();
    let design = ids("Design");
    for id in ["page.setup", "page.setRouting", "connector.setJumps", "design.setConnectorStyle"] {
        assert!(design.contains(&id), "Design lacks {id}");
    }
    let connector = ids("Connector");
    for id in ["connector.setArrows", "connector.setRoute"] {
        assert!(connector.contains(&id), "Connector lacks {id}");
    }
    let process = ids("Process");
    for id in ["validate.run", "validate.setProfile", "validate.next", "validate.previous"] {
        assert!(process.contains(&id), "Process lacks {id}");
    }
}
