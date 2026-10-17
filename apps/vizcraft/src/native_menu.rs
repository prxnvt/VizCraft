//! macOS: the native menu bar, built from the shared menu tree (`menus::menu_tree`) so it shows
//! the same labels, shortcuts and enablement as the in-window bar. The app menu (About,
//! Preferences, Services, Hide, Hide Others, Show All, Quit) and the Window menu are added here.
//!
//! Chosen items come back to the host as command ids with their params; the host runs them in
//! the window in front through `menus::run_item` (the same path as the in-window bar and the
//! shortcuts, so File > New opens a window in both), except `app.quit` and `file.close`, which
//! act across windows.

use std::str::FromStr;
use std::sync::mpsc::{Receiver, channel};

use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use serde_json::{Value, json};
use vizcraft_ui_egui::menus::{self, Item, MenuRole};
use vizcraft_ui_egui::{CraftApp, widgets};

/// The native menu bar and the command behind each of its items.
pub struct NativeMenu {
    /// The main menu; replaced wholesale when the tree's structure changes.
    _menu: Menu,
    bound: Vec<Bound>,
    /// The tree the bar was built from, to notice structural changes (recent files).
    tree: Vec<menus::Menu>,
    events: Receiver<MenuEvent>,
}

/// One of our items: the command it runs and its last known display state.
struct Bound {
    cmd: String,
    params: Value,
    handle: Handle,
    enabled: bool,
    checked: bool,
}

enum Handle {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

/// Keys the native menu takes from egui. AppKit matches a key press against the menu bar's key
/// equivalents before the window (and so winit, egui and its text fields) sees it, so every
/// accelerator we give muda is a key egui never gets. Cmd+N and Cmd+W are given away on purpose:
/// they open and close windows, which is the host's job. Cmd+C, Cmd+V, Cmd+Z, Cmd+A and the
/// rest stay with egui, so a focused text field keeps its own copy, paste, undo and select all.
/// Cmd+Q is on the app menu's own Quit item, so quitting goes through our unsaved-changes prompt.
const MUDA_OWNED: &[&str] = &["CMD+N", "CMD+W"];

/// Translate a shortcut as the registry writes it (`Mod+Shift+S`, `Mod+,`, `Mod+F1`) or as the
/// macOS menu text shows it (the Cmd, Shift, Alt and Ctrl glyphs) into muda's accelerator syntax
/// (`CMD+SHIFT+S`). Only the first of several alternatives counts. `None` when there is none.
pub fn accelerator_text(shortcut: &str) -> Option<String> {
    let first = shortcut.split(" / ").next().unwrap_or("").trim();
    if first.is_empty() {
        return None;
    }
    let mut mods = Vec::new();
    let mut rest = first;
    loop {
        let (m, tail) = if let Some(t) = rest.strip_prefix("Mod+").or_else(|| rest.strip_prefix('\u{2318}')) {
            ("CMD", t)
        } else if let Some(t) = rest.strip_prefix("Shift+").or_else(|| rest.strip_prefix('\u{21e7}')) {
            ("SHIFT", t)
        } else if let Some(t) = rest.strip_prefix("Alt+").or_else(|| rest.strip_prefix('\u{2325}')) {
            ("ALT", t)
        } else if let Some(t) = rest.strip_prefix("Ctrl+").or_else(|| rest.strip_prefix('\u{2303}')) {
            ("CTRL", t)
        } else {
            break;
        };
        if !mods.contains(&m) {
            mods.push(m);
        }
        rest = tail;
    }
    if rest.is_empty() || rest.contains('+') {
        return None;
    }
    mods.push(rest);
    Some(mods.join("+").to_uppercase())
}

/// The accelerator for a command, if the native menu is allowed to own it (see [`MUDA_OWNED`]).
fn owned_accelerator(shortcut: &str) -> Option<Accelerator> {
    let text = accelerator_text(shortcut)?;
    if !MUDA_OWNED.contains(&text.as_str()) {
        return None;
    }
    Accelerator::from_str(&text).ok()
}

impl NativeMenu {
    /// Build the menu bar from the app's menu tree and install it as the application's main menu.
    pub fn install(ctx: &egui::Context, app: &CraftApp) -> NativeMenu {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = tx.send(e);
            // The UI thread only runs when something asks it to.
            ctx.request_repaint();
        }));
        let mut m = NativeMenu { _menu: Menu::new(), bound: Vec::new(), tree: Vec::new(), events: rx };
        m.rebuild(app);
        m
    }

    /// Rebuild the bar when the tree's structure changed (a recent file came or went), then
    /// bring every item's enabled and checked state up to date. Cheap when nothing changed:
    /// a tree comparison and a few flag reads, with AppKit told only about differences.
    pub fn refresh(&mut self, app: &CraftApp) {
        let tree = menus::menu_tree(app);
        if tree != self.tree {
            self.rebuild(app);
        }
        for b in &mut self.bound {
            let e = menus::entry(app, &b.cmd);
            // Recent files are not commands of their own; they are always available.
            let enabled = e.enabled || b.cmd == "file.open";
            if enabled != b.enabled {
                b.enabled = enabled;
                match &b.handle {
                    Handle::Plain(i) => i.set_enabled(enabled),
                    Handle::Check(i) => i.set_enabled(enabled),
                }
            }
            if let Handle::Check(i) = &b.handle
                && e.checked != b.checked
            {
                b.checked = e.checked;
                i.set_checked(e.checked);
            }
        }
    }

    fn rebuild(&mut self, app: &CraftApp) {
        self.bound.clear();
        self.tree = menus::menu_tree(app);
        let menu = Menu::new();
        let app_menu = Submenu::new("VizCraft", true);
        let _ = app_menu.append_items(&[&self.item(app, "About VizCraft", "app.about", json!({})), &PredefinedMenuItem::separator()]);
        // An `App` menu in the tree contributes its items to the app menu.
        for m in self.tree.clone().iter().filter(|m| m.role == MenuRole::App) {
            self.fill(app, &app_menu, &m.items);
            let _ = app_menu.append(&PredefinedMenuItem::separator());
        }
        // Not the predefined Quit: that terminates the process without a close request, so
        // unsaved changes would never be asked about. Ours asks each window in turn.
        let quit = MenuItem::with_id("app.quit", widgets::label_of(app, "app.quit"), true, Accelerator::from_str("CMD+Q").ok());
        let _ = app_menu.append_items(&[
            &self.item(app, "Preferences...", "app.preferences", json!({})),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &quit,
        ]);
        self.bound.push(Bound { cmd: "app.quit".into(), params: json!({}), handle: Handle::Plain(quit), enabled: true, checked: false });
        let _ = menu.append(&app_menu);
        for m in self.tree.clone() {
            if m.role == MenuRole::App {
                continue;
            }
            let sub = Submenu::new(m.title, true);
            self.fill(app, &sub, &m.items);
            match m.role {
                MenuRole::Window => {
                    // AppKit lists the open windows below our items.
                    sub.set_as_windows_menu_for_nsapp();
                }
                MenuRole::Help => sub.set_as_help_menu_for_nsapp(),
                MenuRole::Normal | MenuRole::App => {}
            }
            let _ = menu.append(&sub);
        }
        menu.init_for_nsapp();
        self._menu = menu;
    }

    /// Append the tree's items to a submenu.
    fn fill(&mut self, app: &CraftApp, sub: &Submenu, items: &[Item]) {
        for item in items {
            match item {
                // The tree's Minimize and Zoom are placeholders; AppKit's own items do the job.
                Item::Cmd(_, "window.minimize", _) => {
                    let _ = sub.append(&PredefinedMenuItem::minimize(None));
                }
                Item::Cmd(_, "window.zoom", _) => {
                    let _ = sub.append(&PredefinedMenuItem::maximize(None));
                }
                Item::Cmd(label, id, params) => {
                    let _ = sub.append(&self.item(app, label, id, params.clone()));
                }
                Item::Check(label, id, params) => {
                    let e = menus::entry(app, id);
                    let i = CheckMenuItem::with_id(self.next_id(), *label, e.enabled, e.checked, None);
                    let _ = sub.append(&i);
                    self.bound.push(Bound {
                        cmd: id.to_string(),
                        params: params.clone(),
                        handle: Handle::Check(i),
                        enabled: e.enabled,
                        checked: e.checked,
                    });
                }
                Item::Sub(label, inner) => {
                    let s = Submenu::new(*label, true);
                    self.fill(app, &s, inner);
                    let _ = sub.append(&s);
                }
                Item::Sep => {
                    let _ = sub.append(&PredefinedMenuItem::separator());
                }
                Item::Header(label) => {
                    let _ = sub.append(&PredefinedMenuItem::section_header(label));
                }
                Item::Recent(path) => {
                    let name = std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
                    let i = MenuItem::with_id(self.next_id(), name, true, None);
                    let _ = sub.append(&i);
                    self.bound.push(Bound {
                        cmd: "file.open".into(),
                        params: json!({"path": path}),
                        handle: Handle::Plain(i),
                        enabled: true,
                        checked: false,
                    });
                }
            }
        }
    }

    /// A plain item for a command, bound so [`take_events`](Self::take_events) can name it.
    fn item(&mut self, app: &CraftApp, label: &str, id: &str, params: Value) -> MenuItem {
        let e = menus::entry(app, id);
        let i = MenuItem::with_id(self.next_id(), label, e.enabled, owned_accelerator(widgets::shortcut_of(app, id)));
        self.bound.push(Bound { cmd: id.to_string(), params, handle: Handle::Plain(i.clone()), enabled: e.enabled, checked: false });
        i
    }

    /// Our items are numbered; the number is the index into `bound`.
    fn next_id(&self) -> String {
        self.bound.len().to_string()
    }

    /// Take the bar down again (the user turned "Menu Bar in Window" on): the window draws its
    /// own from the same tree from the next frame.
    pub fn remove(self) {
        MenuEvent::set_event_handler::<fn(MenuEvent)>(None);
        self._menu.remove_for_nsapp();
    }

    /// Menu items chosen since the last call: (command id, params). The host runs them in the
    /// front window with `menus::run_item`, except `app.quit` and `file.close`, which it handles
    /// across windows.
    pub fn take_events(&self) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        while let Ok(ev) = self.events.try_recv() {
            let id = ev.id.as_ref();
            let bound = if id == "app.quit" {
                self.bound.iter().find(|b| b.cmd == "app.quit")
            } else {
                id.parse::<usize>().ok().and_then(|i| self.bound.get(i))
            };
            if let Some(b) = bound {
                out.push((b.cmd.clone(), b.params.clone()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_become_muda_accelerators() {
        assert_eq!(accelerator_text("Mod+N").as_deref(), Some("CMD+N"));
        assert_eq!(accelerator_text("Mod+Shift+S").as_deref(), Some("CMD+SHIFT+S"));
        assert_eq!(accelerator_text("Mod+,").as_deref(), Some("CMD+,"));
        assert_eq!(accelerator_text("Mod+F1").as_deref(), Some("CMD+F1"));
        assert_eq!(accelerator_text("Ctrl+Alt+Delete").as_deref(), Some("CTRL+ALT+DELETE"));
        assert_eq!(accelerator_text("Mod+= / Mod+Plus").as_deref(), Some("CMD+="), "first alternative only");
        // The glyph form the macOS menu text uses.
        assert_eq!(accelerator_text("\u{2318}\u{21e7}Z").as_deref(), Some("CMD+SHIFT+Z"));
        assert_eq!(accelerator_text(""), None);
        assert_eq!(accelerator_text("Mod+"), None);
    }

    #[test]
    fn muda_parses_what_we_produce_and_owns_only_window_keys() {
        for sc in ["Mod+N", "Mod+Shift+S", "Mod+,", "Mod+F1", "Mod+-", "Mod+="] {
            let text = accelerator_text(sc).unwrap();
            assert!(Accelerator::from_str(&text).is_ok(), "{sc} -> {text}");
        }
        assert!(owned_accelerator("Mod+N").is_some());
        assert!(owned_accelerator("Mod+W").is_some());
        assert!(owned_accelerator("Mod+S").is_none(), "Save stays with egui");
        assert!(owned_accelerator("Mod+C").is_none(), "text fields keep copy");
    }
}
