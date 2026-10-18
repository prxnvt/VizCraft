//! The pane manager through the app: commands, moving sides, persistence, sanitising, hostile
//! `ui.set` values, and a few frames of real layout and pointer input under egui_kittest.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use vizcraft_engine::Session;
use vizcraft_ui_egui::panes::{self, DEFAULT_HEIGHT, MAX_WIDTH, MIN_HEIGHT, MIN_WIDTH, PaneLayout, Side};
use vizcraft_ui_egui::{CraftApp, Services, UI_COMMANDS, control};

fn app() -> CraftApp {
    CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default())
}

fn layout(v: &Value) -> PaneLayout {
    serde_json::from_value(v.clone()).unwrap()
}

/// The app laid out for `steps` frames (the first two frames install the fonts).
fn harness(app: CraftApp) -> Harness<'static, CraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

#[test]
fn registry_has_the_four_panes_and_commands() {
    let keys: Vec<&str> = panes::panes().iter().map(|p| p.key).collect();
    assert_eq!(keys, ["shapes", "navigation", "history", "layers", "shapeData"]);
    for p in panes::panes() {
        assert!(p.default_width >= MIN_WIDTH && p.default_width <= MAX_WIDTH);
        assert!(!p.title.is_empty());
        let id = format!("ui.pane.{}", p.key);
        assert!(UI_COMMANDS.iter().any(|(i, ..)| *i == id), "{id} is listed in UI_COMMANDS");
    }
    for id in ["ui.pane", "ui.paneSide", "ui.panesReset", "ui.collapsePanes"] {
        assert!(UI_COMMANDS.iter().any(|(i, ..)| *i == id), "{id} is listed");
    }
    let l = PaneLayout::default();
    assert_eq!(l.left, ["shapes", "navigation"], "Shapes opens on the left above Navigation");
    assert!(l.is_collapsed("navigation") && !l.is_collapsed("shapes"), "Navigation starts folded so Shapes gets the height");
    assert_eq!(l.right, ["history", "shapeData"], "the right side stacks two panes by default");
}

#[test]
fn ui_pane_toggles_opens_and_closes() {
    let mut a = app();
    let r = a.run("ui.pane", json!({"name": "navigation"})).unwrap();
    assert!(!layout(&r).is_open("navigation"), "toggle closes an open pane");
    assert!(!a.session.view.nav_pane, "the view flag mirrors the layout");
    let r = a.run("ui.pane", json!({"name": "navigation"})).unwrap();
    assert_eq!(layout(&r).side("navigation"), Some(Side::Left), "reopened where it was");
    assert!(a.session.view.nav_pane);
    a.run("ui.pane", json!({"name": "navigation", "value": false})).unwrap();
    a.run("ui.pane", json!({"name": "navigation", "value": false})).unwrap();
    assert!(!a.ui.panes.is_open("navigation"), "closing twice stays closed");
    let r = a.run("ui.pane", json!({"name": "navigation", "value": true, "side": "right"})).unwrap();
    assert_eq!(layout(&r).right, ["history", "shapeData", "navigation"]);
    // The per-pane ids menus use.
    a.run("ui.pane.history", json!({})).unwrap();
    assert!(!a.ui.panes.is_open("history"));
    a.run("ui.pane.history", json!({"value": true})).unwrap();
    assert!(a.ui.panes.is_open("history"));
    // Unknown panes and sides are errors; a missing name just reports.
    assert!(a.run("ui.pane", json!({"name": "ghost"})).is_err());
    assert!(a.run("ui.pane", json!({"name": "history", "side": "top"})).is_err());
    assert!(a.run("ui.pane", json!({})).is_ok());
    assert!(a.run("ui.pane.ghost", json!({})).is_err());
}

#[test]
fn ui_pane_side_moves_and_opens() {
    let mut a = app();
    let r = a.run("ui.paneSide", json!({"name": "shapeData", "side": "left"})).unwrap();
    let l = layout(&r);
    assert_eq!(l.left, ["shapes", "navigation", "shapeData"]);
    assert_eq!(l.right, ["history"]);
    a.run("ui.pane", json!({"name": "history", "value": false})).unwrap();
    assert!(a.ui.panes.right.is_empty());
    let r = a.run("ui.paneSide", json!({"name": "history", "side": "right"})).unwrap();
    assert_eq!(layout(&r).right, ["history"], "moving a closed pane opens it on that side");
    assert_eq!(a.ui.panes.right_width, 260.0, "the first pane on an empty side sets its default width");
    assert!(a.run("ui.paneSide", json!({"name": "history", "side": "middle"})).is_err());
    assert!(a.run("ui.paneSide", json!({"name": "nothing", "side": "left"})).is_err());
    // A closed pane remembers its side.
    a.run("ui.pane", json!({"name": "shapeData", "value": false})).unwrap();
    a.run("ui.pane", json!({"name": "shapeData", "value": true})).unwrap();
    assert_eq!(a.ui.panes.side("shapeData"), Some(Side::Left));
    // Reset puts everything back.
    let r = a.run("ui.panesReset", json!({})).unwrap();
    assert_eq!(layout(&r), PaneLayout::default());
    assert!(a.session.view.nav_pane && a.session.view.props_pane);
}

#[test]
fn collapse_commands_fold_a_side() {
    let mut a = app();
    let r = a.run("ui.collapsePanes", json!({"side": "left"})).unwrap();
    assert!(layout(&r).left_collapsed);
    a.run("ui.collapsePanes", json!({"side": "left", "value": false})).unwrap();
    assert!(!a.ui.panes.left_collapsed);
    assert!(a.run("ui.collapsePanes", json!({"side": "sideways"})).is_err());
    // Opening a pane on a folded side unfolds it.
    a.run("ui.collapsePanes", json!({"side": "right", "value": true})).unwrap();
    a.run("ui.pane", json!({"name": "history", "value": true})).unwrap();
    assert!(!a.ui.panes.right_collapsed);
}

#[test]
fn window_menu_items_track_the_layout() {
    let mut a = app();
    let items = panes::window_menu_items(&a);
    assert_eq!(items.len(), 5);
    assert!(items.iter().all(|(title, _, on)| *on == !matches!(*title, "Layers")), "every pane but Layers starts open");
    assert_eq!(items[0], ("Shapes", "ui.pane.shapes", true));
    assert_eq!(items[1], ("Navigation", "ui.pane.navigation", true));
    for (_, id, _) in &items {
        a.run(id, json!({})).unwrap();
    }
    assert!(panes::window_menu_items(&a).iter().all(|(title, _, on)| *on == matches!(*title, "Layers")), "each toggle flipped");
}

#[test]
fn persistence_round_trips_through_prefs() {
    let mut first = app();
    first.run("ui.paneSide", json!({"name": "history", "side": "left"})).unwrap();
    first.run("ui.pane", json!({"name": "shapeData", "value": false})).unwrap();
    first.run("ui.collapsePanes", json!({"side": "left", "value": true})).unwrap();
    first.ui.panes.set_collapsed("history", true);
    first.ui.panes.heights.insert("navigation".into(), 300.0);
    first.ui.panes.left_width = 333.0;
    let saved = serde_json::to_string(&first.prefs()).unwrap();
    let mut second = app();
    second.apply_prefs(serde_json::from_str(&saved).unwrap());
    assert_eq!(second.ui.panes, first.ui.panes);
    assert_eq!(second.ui.panes.left, ["shapes", "navigation", "history"]);
    assert!(second.ui.panes.left_collapsed);
    assert!(second.ui.panes.is_collapsed("history"));
    assert_eq!(second.ui.panes.height("navigation"), 300.0);
    // After a frame the loaded layout, not the engine's default flags, decides what shows.
    assert!(second.session.view.props_pane, "the engine default before the first frame");
    let h = harness(second);
    let app = h.state();
    assert!(!app.session.view.props_pane, "the layout is the source of truth");
    assert!(!app.ui.panes.is_open("shapeData"));
}

#[test]
fn sanitize_drops_junk_layouts() {
    let junk = json!({
        "left": ["ghost", "navigation", "navigation", 7],
        "right": ["navigation", "shapeData", ""],
        "leftWidth": -5, "rightWidth": 1e12,
        "collapsed": ["ghost", "history", "history"],
        "heights": {"ghost": 100, "navigation": "tall", "history": 1e9, "shapeData": -3},
        "lastSide": {"ghost": "left", "history": "right"}
    });
    // A list with a number in it is not a layout at all: the whole state is rejected, not panicked.
    assert!(serde_json::from_value::<PaneLayout>(junk.clone()).is_err());
    let mut j = junk.clone();
    j["left"] = json!(["ghost", "navigation", "navigation"]);
    j["heights"] = json!({"ghost": 100, "navigation": null, "history": 1e9, "shapeData": -3});
    assert!(serde_json::from_value::<PaneLayout>(j.clone()).is_err(), "a null height is rejected");
    j["heights"] = json!({"ghost": 100, "history": 1e9, "shapeData": -3});
    let mut l: PaneLayout = serde_json::from_value(j).unwrap();
    l.sanitize();
    assert_eq!(l.left, ["navigation"]);
    assert_eq!(l.right, ["shapeData"], "a pane cannot be on both sides; unknown keys go");
    assert_eq!((l.left_width, l.right_width), (MIN_WIDTH, MAX_WIDTH));
    assert_eq!(l.collapsed, ["history"]);
    assert!(!l.heights.contains_key("ghost"));
    assert_eq!(l.height("history"), panes::MAX_HEIGHT);
    assert_eq!(l.height("shapeData"), MIN_HEIGHT);
    assert!(!l.last_side.contains_key("ghost"));
    // Loading through prefs sanitises too, and the app then lays out without trouble.
    let mut a = app();
    let mut state = serde_json::to_value(a.prefs()).unwrap();
    state["panes"] = json!({"left": ["ghost"], "right": ["history", "ghost"], "leftWidth": f64::MAX, "rightWidth": 0});
    a.apply_prefs(serde_json::from_value(state).unwrap());
    assert_eq!(a.ui.panes.left, Vec::<String>::new());
    assert_eq!(a.ui.panes.right, ["history"]);
    let h = harness(a);
    assert!(!h.state().session.view.nav_pane);
    assert!(!h.state().session.view.props_pane);
}

#[test]
fn hostile_ui_set_values_never_panic() {
    let mut a = app();
    let values = [
        json!(null),
        json!(0),
        json!(-1),
        json!(1e300),
        json!("x"),
        json!(""),
        json!([]),
        json!([1, 2]),
        json!(["navigation", "navigation", "shapeData"]),
        json!({}),
        json!({"navigation": "right"}),
        json!({"navigation": 1e300}),
        json!(true),
        json!("\u{0}\u{ffff}"),
        json!(["\u{0}", "ghost"]),
    ];
    let paths = [
        "/panes",
        "/panes/left",
        "/panes/right",
        "/panes/leftWidth",
        "/panes/rightWidth",
        "/panes/leftCollapsed",
        "/panes/rightCollapsed",
        "/panes/collapsed",
        "/panes/heights",
        "/panes/heights/navigation",
        "/panes/lastSide",
        "/panes/lastSide/history",
        "/panes/left/0",
        "/panes/nothing",
        "/panes/",
        "panes/left",
    ];
    for path in paths {
        for v in &values {
            let _ = control::set(&mut a, path, v.clone());
            // Whatever got through is a layout the manager can lay out.
            a.ui.panes.sanitize();
            let keys: Vec<String> = a.ui.panes.left.clone();
            let hs = a.ui.panes.heights_for(&keys, 500.0);
            assert_eq!(hs.len(), keys.len());
            assert!(a.ui.panes.left_width.is_finite() && a.ui.panes.right_width.is_finite());
        }
    }
    // And a frame of the whole app with whatever is left.
    let h = harness(a);
    assert!(h.state().frame_ms >= 0.0);
}

#[test]
fn pane_commands_survive_junk_params() {
    let mut a = app();
    let junk =
        [json!({}), json!(null), json!(7), json!({"name": 5, "value": "x", "side": []}), json!({"name": "", "side": ""}), json!({"name": "\u{0}"})];
    for id in ["ui.pane", "ui.paneSide", "ui.panesReset", "ui.collapsePanes", "ui.pane.navigation", "ui.pane."] {
        for j in &junk {
            let _ = a.run(id, j.clone());
        }
    }
    a.run("ui.panesReset", json!({})).unwrap();
    assert_eq!(a.ui.panes, PaneLayout::default());
}

#[test]
fn view_commands_and_pane_commands_agree() {
    let mut h = harness(app());
    // The engine's View command (ribbon, menu) closes the pane in the layout on the next frame.
    h.state_mut().run("view.navigationPane", json!({"value": false})).unwrap();
    h.run_steps(2);
    assert!(!h.state().ui.panes.is_open("navigation"));
    assert!(!h.state().session.view.nav_pane);
    h.state_mut().run("view.navigationPane", json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().ui.panes.side("navigation"), Some(Side::Left));
    // And the other way round: the pane command sets the flag the menus check.
    h.state_mut().run("ui.pane", json!({"name": "shapeData"})).unwrap();
    assert!(!h.state().session.view.props_pane);
    h.run_steps(2);
    assert!(!h.state().session.view.props_pane, "the frame does not reopen it");
    assert!(!h.state().ui.panes.is_open("shapeData"));
    // A moved pane is still "on" for the View menu.
    h.state_mut().run("ui.paneSide", json!({"name": "shapeData", "side": "left"})).unwrap();
    h.run_steps(2);
    assert!(h.state().session.view.props_pane);
    assert_eq!(h.state().ui.panes.left, ["shapes", "navigation", "shapeData"]);
}

#[test]
fn splitter_drag_resizes_the_upper_pane() {
    let mut h = harness(app());
    // Right side: History over the Inspector. History has the default height; the splitter sits
    // in the gap below it. The side column starts under the fold bar.
    let width = 1280.0;
    let x = width - 130.0;
    let top = h.state().ui.panes.heights.get("history").copied().unwrap_or(DEFAULT_HEIGHT);
    assert_eq!(top, DEFAULT_HEIGHT);
    // Find the gap by probing: the pane rects are not exported, so hover down the column until the
    // resize cursor shows.
    let mut gap_y = None;
    let mut y = 150.0;
    while y < 700.0 {
        h.hover_at(egui::pos2(x, y));
        h.run_steps(2);
        if h.output().platform_output.cursor_icon == egui::CursorIcon::ResizeVertical {
            gap_y = Some(y);
            break;
        }
        y += 1.0;
    }
    let gap_y = gap_y.expect("a splitter between History and the Inspector");
    h.drag_at(egui::pos2(x, gap_y));
    h.run_steps(2);
    for i in 1..=10 {
        h.hover_at(egui::pos2(x, gap_y + 8.0 * i as f32));
        h.run_steps(1);
    }
    h.drop_at(egui::pos2(x, gap_y + 80.0));
    h.run_steps(2);
    let after = h.state().ui.panes.height("history");
    assert!((after - (DEFAULT_HEIGHT + 80.0)).abs() < 4.0, "history grew by the drag: {after}");
    assert!(after >= MIN_HEIGHT);
}

#[test]
fn ui_set_width_resizes_the_side_live() {
    let mut h = harness(app());
    assert_eq!(h.state().ui.panes.left_width, 240.0, "the default width after a few frames");
    // `ui.set panes.leftWidth` (the control channel) changes the layout; the next frame pushes it
    // into the panel, and the frames after keep it (the manager reads the panel's width back).
    control::set(h.state_mut(), "panes.leftWidth", json!(300)).unwrap();
    h.run_steps(3);
    assert!((h.state().ui.panes.left_width - 300.0).abs() < 1.0, "left side is 300 wide: {}", h.state().ui.panes.left_width);
    control::set(h.state_mut(), "panes.rightWidth", json!(400)).unwrap();
    h.run_steps(3);
    assert!((h.state().ui.panes.right_width - 400.0).abs() < 1.0, "right side is 400 wide: {}", h.state().ui.panes.right_width);
    // Back to a narrower width too, and a reset returns the defaults.
    control::set(h.state_mut(), "panes.leftWidth", json!(180)).unwrap();
    h.run_steps(3);
    assert!((h.state().ui.panes.left_width - 180.0).abs() < 1.0);
    h.state_mut().run("ui.panesReset", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ui.panes.left_width, 240.0);
    assert_eq!(h.state().ui.panes.right_width, 260.0);
}

#[test]
fn strip_click_unfolds_a_side() {
    let mut a = app();
    a.run("ui.collapsePanes", json!({"side": "left", "value": true})).unwrap();
    let mut h = harness(a);
    assert!(h.state().ui.panes.left_collapsed);
    // The strip's unfold button sits at the top left of the side, under the ribbon.
    let mut y = 150.0;
    let mut done = false;
    while y < 400.0 && !done {
        h.hover_at(egui::pos2(13.0, y));
        h.run_steps(1);
        h.drag_at(egui::pos2(13.0, y));
        h.run_steps(1);
        h.drop_at(egui::pos2(13.0, y));
        h.run_steps(2);
        done = !h.state().ui.panes.left_collapsed;
        y += 6.0;
    }
    assert!(done, "clicking the strip unfolds the side");
}

/// Number of master tiles in the window labelled `label` (other widgets may share a label, so
/// only buttons count).
fn tiles(h: &Harness<'_, CraftApp>, label: &str) -> usize {
    h.query_all_by_role_and_label(egui::accesskit::Role::Button, label).count()
}

fn search_box<'a>(h: &'a Harness<'_, CraftApp>) -> egui_kittest::Node<'a> {
    h.get_by(|n| n.placeholder() == Some("Search shapes"))
}

#[test]
fn typing_in_the_shapes_search_shows_only_matching_tiles() {
    let mut h = harness(app());
    assert_eq!(tiles(&h, "Rectangle"), 1, "every set shows with an empty query");
    assert!(h.query_by_label("Results").is_none());
    search_box(&h).focus();
    h.run_steps(1);
    search_box(&h).type_text("decis");
    h.run_steps(3);
    assert_eq!(h.state().shapes_query, "decis");
    h.get_by_label("Results");
    let all: Vec<_> = vizcraft_stencils::sets().iter().flat_map(|s| s.masters.iter()).collect();
    for m in &all {
        let want = usize::from(m.id == "flowchart.decision");
        assert_eq!(tiles(&h, m.label), want, "{} with query decis", m.id);
    }
    // A result tile still inserts on click.
    let n = h.state().session.doc.page().unwrap().shapes.len();
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Decision").click();
    h.run_steps(3);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.shapes.len(), n + 1);
    assert_eq!(page.shapes.last().unwrap().stencil, "flowchart.decision");
    // The clear button brings the sets back.
    h.get_by_label("Clear search").click();
    h.run_steps(3);
    assert!(h.state().shapes_query.is_empty());
    assert_eq!(tiles(&h, "Rectangle"), 1);
    assert!(h.query_by_label("Results").is_none());
}

#[test]
fn escape_clears_the_shapes_search() {
    let mut h = harness(app());
    search_box(&h).focus();
    h.run_steps(1);
    search_box(&h).type_text("cloud");
    h.run_steps(2);
    assert_eq!(tiles(&h, "Rectangle"), 0);
    assert_eq!(tiles(&h, "Cloud"), 1);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().shapes_query.is_empty());
    assert_eq!(tiles(&h, "Rectangle"), 1);
}

#[test]
fn shapes_search_with_no_match_says_so() {
    let mut h = harness(app());
    let r = h.state_mut().run("ui.shapesSearch", json!({"query": "zebra crossing"})).unwrap();
    assert_eq!(r["results"], json!([]));
    h.run_steps(3);
    h.get_by_label("No shapes match");
    assert!(h.query_by_label("Results").is_none());
    assert_eq!(tiles(&h, "Rectangle") + tiles(&h, "Decision"), 0);
}

#[test]
fn ui_shapes_search_sets_the_query_opens_the_pane_and_is_not_saved() {
    let mut a = app();
    a.run("ui.pane", json!({"name": "shapes", "value": false})).unwrap();
    let r = a.run("ui.shapesSearch", json!({"query": "data"})).unwrap();
    assert_eq!(r["query"], "data");
    assert_eq!(r["results"][0], "flowchart.data", "{r}");
    assert!(a.ui.panes.is_open("shapes"), "searching opens the pane");
    let prefs = serde_json::to_string(&a.prefs()).unwrap();
    assert!(!prefs.contains("shapesQuery") && !prefs.contains("shapes_query"), "{prefs}");
    // Junk clears instead of failing; a huge query is capped.
    for junk in [json!({}), json!({"query": 5}), json!(null)] {
        let r = a.run("ui.shapesSearch", junk).unwrap();
        assert_eq!(r["query"], "");
        assert!(a.shapes_query.is_empty());
    }
    a.run("ui.shapesSearch", json!({"query": "x".repeat(100_000)})).unwrap();
    assert!(a.shapes_query.chars().count() <= 256);
    // Empty query: no results listed, the pane shows the sets.
    assert_eq!(a.run("ui.shapesSearch", json!({"query": "  "})).unwrap()["results"], json!([]));
}
