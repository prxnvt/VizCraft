//! The menu tree and the in-window menu bar, driven headlessly with egui_kittest: every item is
//! a command, shortcut text is the platform's, the bar opens, runs and closes, keys dispatch, and
//! the palette runs a command.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::menus::{self, Item};
use vizcraft_ui_egui::{CraftApp, Services, UI_COMMANDS};

fn app() -> CraftApp {
    let mut app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    // The bar is drawn on every platform when the preference says so.
    app.ui.in_window_menus = true;
    app
}

/// A harness that runs the app the way a host does: `logic` then `ui`, every frame.
fn harness(mut app: CraftApp) -> Harness<'static, CraftApp> {
    // The ribbon and the History pane repeat menu labels (Select All, Undo); these tests look
    // for the menu's, so the window shows neither.
    app.ui.ribbon_collapsed = true;
    app.ui.panes.close("history");
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 720.0)).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    // The first two frames only install fonts.
    h.run_steps(4);
    h
}

fn is_command(app: &CraftApp, id: &str) -> bool {
    app.session.registry.get(id).is_some() || UI_COMMANDS.iter().any(|(c, ..)| *c == id)
}

#[test]
fn every_cmd_item_resolves_to_a_command() {
    let app = app();
    fn walk(app: &CraftApp, items: &[Item]) {
        for it in items {
            match it {
                Item::Cmd(label, id, _) | Item::Check(label, id, _) => {
                    assert!(is_command(app, id), "{label}: {id} is neither a registry nor a UI command");
                    let e = menus::entry(app, id);
                    assert!(!e.label.is_empty(), "{id} has no label");
                }
                Item::Sub(_, sub) => walk(app, sub),
                _ => {}
            }
        }
    }
    let tree = menus::menu_tree(&app);
    let titles: Vec<&str> = tree.iter().map(|m| m.title).collect();
    assert_eq!(titles, ["File", "Edit", "View", "Window", "Help"]);
    for m in &tree {
        walk(&app, &m.items);
    }
    assert!(tree.iter().any(|m| m.role == menus::MenuRole::Window));
    assert!(tree.iter().any(|m| m.role == menus::MenuRole::Help));
}

#[test]
fn entry_shortcut_text_is_the_platforms() {
    let app = app();
    let save = menus::entry(&app, "file.save");
    let redo = menus::entry(&app, "edit.redo");
    if cfg!(target_os = "macos") {
        assert_eq!(save.shortcut, "\u{2318}S");
        assert_eq!(redo.shortcut, "\u{2318}\u{21e7}Z", "first alternative, as glyphs");
    } else {
        assert_eq!(save.shortcut, "Ctrl+S");
        assert_eq!(redo.shortcut, "Ctrl+Shift+Z");
    }
    assert!(menus::entry(&app, "ui.print").shortcut.contains('P'));
    assert!(menus::entry(&app, "ui.print").enabled, "Print opens Print Preview");
    assert!(!menus::entry(&app, "ui.find").enabled, "placeholders are disabled");
    assert_eq!(menus::entry(&app, "file.open").label, "Open...");
}

#[test]
fn recent_files_appear_in_open_recent_with_clear() {
    let mut app = app();
    assert!(
        menus::menu_tree(&app)
            .iter()
            .flat_map(|m| &m.items)
            .any(|it| matches!(it, Item::Sub("Open Recent", sub) if matches!(sub.first(), Some(Item::Header(_)))))
    );
    app.ui.recent = vec!["/tmp/a.craft.json".into(), "/tmp/b.craft.json".into()];
    let tree = menus::menu_tree(&app);
    let sub = tree.iter().flat_map(|m| &m.items).find_map(|it| match it {
        Item::Sub("Open Recent", sub) => Some(sub),
        _ => None,
    });
    let sub = sub.expect("Open Recent submenu");
    assert_eq!(sub.iter().filter(|i| matches!(i, Item::Recent(_))).count(), 2);
    assert!(matches!(sub.last(), Some(Item::Cmd("Clear Recent", "ui.clearRecent", _))));
    app.run("ui.clearRecent", json!({})).unwrap();
    assert!(app.ui.recent.is_empty());
}

#[test]
fn clicking_edit_undo_in_the_bar_undoes_a_change() {
    let mut a = app();
    a.run("edit.setTitle", json!({"title": "changed by the test"})).unwrap();
    assert!(a.session.can_undo());
    let mut h = harness(a);
    h.get_by_label("Edit").click();
    h.run_steps(2);
    assert!(menus::is_open(&h.ctx), "the Edit menu opened");
    let undo = h.state().session.undo_label().map(|l| format!("Undo {l}")).unwrap();
    h.get_by_label(undo.as_str()).click();
    h.run_steps(2);
    assert!(!h.state().session.can_undo(), "Undo ran");
    assert!(!menus::is_open(&h.ctx), "the menu closed after running an item");
}

#[test]
fn escape_closes_the_menu_and_hover_switches_menus() {
    let mut h = harness(app());
    h.get_by_label("File").click();
    h.run_steps(2);
    assert!(menus::is_open(&h.ctx));
    // Moving onto another title switches to it (two moves: a resting pointer does not switch).
    let edit = h.get_by_label("Edit").rect();
    h.hover_at(edit.center() + egui::vec2(-3.0, 0.0));
    h.run_steps(1);
    h.hover_at(edit.center());
    h.run_steps(2);
    assert!(h.query_by_label("Select All").is_some(), "the Edit menu is showing");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(!menus::is_open(&h.ctx), "Escape closed the menu");
    assert!(h.query_by_label("Select All").is_none());
}

#[test]
fn clicking_outside_closes_the_menu_without_acting_underneath() {
    let mut a = app();
    let tab = a.ui.tab.clone();
    a.ui.tab = tab;
    let mut h = harness(a);
    h.get_by_label("View").click();
    h.run_steps(2);
    assert!(menus::is_open(&h.ctx));
    // The canvas, far from the bar.
    h.hover_at(egui::pos2(550.0, 500.0));
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: egui::pos2(550.0, 500.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: egui::pos2(550.0, 500.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert!(!menus::is_open(&h.ctx), "a click elsewhere closed the menu");
}

#[test]
fn keyboard_walks_the_menu() {
    let mut a = app();
    a.run("edit.setTitle", json!({"title": "typed"})).unwrap();
    let mut h = harness(a);
    h.get_by_label("Edit").click();
    h.run_steps(2);
    // Down selects the first row (Undo), Enter runs it.
    h.key_press(Key::ArrowDown);
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(!h.state().session.can_undo(), "Enter ran the highlighted Undo");
    assert!(!menus::is_open(&h.ctx));
}

#[test]
fn shortcuts_dispatch_and_respect_dialogs() {
    let mut a = app();
    a.run("edit.setTitle", json!({"title": "typed"})).unwrap();
    let mut h = harness(a);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert!(!h.state().session.can_undo(), "Mod+Z undid");
    assert!(h.state().session.can_redo());
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::P);
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("commands"));
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(2);
    assert!(h.state().session.can_redo(), "a dialog owns the keyboard: Mod+Shift+Z did not redo");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().dialog.is_none(), "Escape closed the palette");
    let collapsed = h.state().ui.ribbon_collapsed;
    h.key_press_modifiers(Modifiers::COMMAND, Key::F1);
    h.run_steps(2);
    assert_ne!(h.state().ui.ribbon_collapsed, collapsed, "an F-key combination fires");
}

#[test]
fn the_palette_runs_a_command() {
    let mut h = harness(app());
    h.state_mut().run("ui.commands", json!({})).unwrap();
    h.run_steps(2);
    let before = h.state().session.view.zoom;
    // The search box takes focus when the palette opens, so typing lands in it.
    h.event(egui::Event::Text("zoom in".into()));
    h.run_steps(2);
    assert!(h.query_by_label("Dark Mode").is_none(), "the list is filtered: {:?}", h.state().dialog);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().dialog.is_none(), "the palette closed");
    assert!(h.state().session.view.zoom > before, "Zoom In ran");
}

#[test]
fn structure_key_is_stable_across_frames() {
    let a = app();
    let k = menus::structure_key(&a);
    let h = harness(a);
    assert_eq!(menus::structure_key(h.state()), k);
}

#[test]
fn a_blank_document_has_a_disabled_undo() {
    let a = CraftApp::new(Session::new(Document::new()), Services::default());
    assert!(!menus::entry(&a, "edit.undo").enabled);
    assert_eq!(menus::entry(&a, "edit.undo").label, "Undo");
}
