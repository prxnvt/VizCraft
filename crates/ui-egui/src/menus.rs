//! The menu tree, shared by the in-window menu bar (egui) and the desktop app's native macOS
//! menu bar (muda, built by the host from [`menu_tree`]). Items are command ids, so both bars
//! show the same labels, shortcuts and enablement, and the control channel can press any of them.

use egui::Ui;
use serde_json::{Value, json};

use crate::{CraftApp, widgets};

/// One entry of a menu.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A command: label, id, params.
    Cmd(&'static str, &'static str, Value),
    /// A submenu.
    Sub(&'static str, Vec<Item>),
    Sep,
    /// A non-interactive heading (recent files, say).
    Header(&'static str),
    /// A command shown with a check mark when [`entry`] says it is checked.
    Check(&'static str, &'static str, Value),
    /// A recent file: opens it. Owned, because the path comes from the preferences.
    Recent(String),
}

/// What a top-level menu is for; the native bar places `App`, `Window` and `Help` specially.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuRole {
    Normal,
    App,
    Window,
    Help,
}

/// A top-level menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub title: &'static str,
    pub role: MenuRole,
    pub items: Vec<Item>,
}

/// How a command should look in a menu right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    /// Platform shortcut text (`Cmd S` glyphs or `Ctrl+S`); empty when there is none.
    pub shortcut: String,
    pub enabled: bool,
    pub checked: bool,
}

/// The menus: File, Edit, View, Window, Help (the app menu is the host's, on macOS).
pub fn menu_tree(app: &CraftApp) -> Vec<Menu> {
    let mut file = vec![
        Item::Cmd("New", "file.new", json!({})),
        Item::Cmd("New Window", "ui.newWindow", json!({})),
        Item::Cmd("Open...", "file.open", json!({})),
    ];
    if !app.ui.recent.is_empty() {
        file.push(Item::Sub("Open Recent", app.ui.recent.iter().map(|p| Item::Recent(p.clone())).collect()));
    }
    file.extend([
        Item::Sep,
        Item::Cmd("Close", "file.close", json!({})),
        Item::Cmd("Save", "file.save", json!({})),
        Item::Cmd("Save As...", "file.saveAs", json!({})),
    ]);
    if !cfg!(target_os = "macos") {
        // On macOS these live in the app menu.
        file.extend([Item::Sep, Item::Cmd("Preferences...", "app.preferences", json!({})), Item::Cmd("Quit", "app.quit", json!({}))]);
    }
    vec![
        Menu { title: "File", role: MenuRole::Normal, items: file },
        Menu {
            title: "Edit",
            role: MenuRole::Normal,
            items: vec![
                Item::Cmd("Undo", "edit.undo", json!({})),
                Item::Cmd("Redo", "edit.redo", json!({})),
                Item::Sep,
                Item::Cmd("Cut", "edit.cut", json!({})),
                Item::Cmd("Copy", "edit.copy", json!({})),
                Item::Cmd("Paste", "edit.paste", json!({})),
                Item::Cmd("Select All", "edit.selectAll", json!({})),
                Item::Sep,
                Item::Cmd("Insert Note", "insert.note", json!({})),
                Item::Cmd("Delete Note", "edit.deleteNote", json!({})),
            ],
        },
        Menu {
            title: "View",
            role: MenuRole::Normal,
            items: vec![
                Item::Check("Navigation Pane", "view.navigationPane", json!({})),
                Item::Check("Properties Pane", "view.propertiesPane", json!({})),
                Item::Check("Collapse the Ribbon", "ui.collapseRibbon", json!({})),
                Item::Sep,
                Item::Cmd("Zoom In", "view.zoomIn", json!({})),
                Item::Cmd("Zoom Out", "view.zoomOut", json!({})),
                Item::Cmd("Actual Size", "view.zoom100", json!({})),
                Item::Cmd("Zoom...", "view.zoom", json!({})),
                Item::Sep,
                Item::Check("Dark Mode", "view.dark", json!({})),
                Item::Check("Menu Bar in Window", "ui.inWindowMenus", json!({})),
            ],
        },
        Menu { title: "Window", role: MenuRole::Window, items: vec![Item::Cmd("New Window", "ui.newWindow", json!({}))] },
        Menu {
            title: "Help",
            role: MenuRole::Help,
            items: vec![Item::Cmd("Search Commands...", "ui.commands", json!({})), Item::Sep, Item::Cmd("About BaseCraft", "help.about", json!({}))],
        },
    ]
}

/// How to show the command `id` right now: its label, shortcut, enablement and check state.
pub fn entry(app: &CraftApp, id: &str) -> Entry {
    let checked = match id {
        "view.navigationPane" => app.session.view.nav_pane,
        "view.propertiesPane" => app.session.view.props_pane,
        "view.dark" => app.session.view.dark_mode,
        "ui.collapseRibbon" => app.ui.ribbon_collapsed,
        "ui.inWindowMenus" => app.ui.in_window_menus,
        _ => false,
    };
    Entry { label: widgets::label_of(app, id).to_string(), shortcut: widgets::shortcut_text(app, id), enabled: widgets::enabled(app, id), checked }
}

/// Whether the menu bar is drawn inside the window: always off macOS (nothing else has one),
/// and on macOS when the preference says so.
pub fn in_window_menus(app: &CraftApp) -> bool {
    !cfg!(target_os = "macos") || app.ui.in_window_menus
}

/// The in-window menu bar, from [`menu_tree`].
pub fn menu_bar(app: &mut CraftApp, ui: &mut Ui) {
    let t = crate::theme::Tokens::get(ui.ctx());
    egui::Panel::top("menu_bar").exact_size(26.0).frame(egui::Frame::NONE.fill(t.tab_strip).inner_margin(egui::Margin::symmetric(4, 0))).show(
        ui,
        |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                for menu in menu_tree(app) {
                    ui.menu_button(menu.title, |ui| items(app, ui, &menu.items));
                }
            });
        },
    );
}

fn items(app: &mut CraftApp, ui: &mut Ui, list: &[Item]) {
    ui.set_min_width(220.0);
    for item in list {
        match item {
            Item::Cmd(label, id, params) => widgets::menu_item(ui, app, label, id, params.clone()),
            Item::Check(label, id, params) => {
                let e = entry(app, id);
                let text = if e.checked { format!("\u{2713} {label}") } else { format!("    {label}") };
                let resp = ui.add_enabled(e.enabled, egui::Button::new(text).shortcut_text(e.shortcut).min_size(egui::vec2(200.0, 0.0)));
                if resp.clicked() {
                    let _ = app.run(id, params.clone());
                    ui.close();
                }
            }
            Item::Sub(label, sub) => {
                ui.menu_button(*label, |ui| items(app, ui, sub));
            }
            Item::Sep => {
                ui.separator();
            }
            Item::Header(label) => {
                ui.label(egui::RichText::new(*label).small().weak());
            }
            Item::Recent(path) => {
                let name = std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
                if ui.add(egui::Button::new(name).min_size(egui::vec2(200.0, 0.0))).on_hover_text(path).clicked() {
                    let _ = app.run("file.open", json!({"path": path}));
                    ui.close();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_item_is_a_known_command() {
        let app = CraftApp::new(basecraft_engine::Session::new(basecraft_engine::Document::new()), crate::Services::default());
        fn walk(app: &CraftApp, items: &[Item]) {
            for i in items {
                match i {
                    Item::Cmd(_, id, _) | Item::Check(_, id, _) => {
                        assert!(
                            app.session.registry.get(id).is_some() || crate::UI_COMMANDS.iter().any(|(c, ..)| c == id),
                            "menu item {id} is not a command"
                        );
                        assert!(!entry(app, id).label.is_empty(), "{id} has no label");
                    }
                    Item::Sub(_, sub) => walk(app, sub),
                    _ => {}
                }
            }
        }
        for m in menu_tree(&app) {
            walk(&app, &m.items);
        }
        assert_eq!(entry(&app, "view.navigationPane").checked, app.session.view.nav_pane);
        assert!(!entry(&app, "edit.undo").enabled);
    }
}
