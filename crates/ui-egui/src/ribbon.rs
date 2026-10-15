//! The ribbon: the tab strip and each tab's groups. Every control runs a command by id.

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::theme::{Tokens, medium, semibold};
use crate::widgets::{CONTENT_H, LABEL_H, big, group, menu_button, menu_item, small, stack};
use crate::{CraftApp, widgets};

/// The ribbon tabs, in order. `File` opens the File page instead of a ribbon.
pub const TABS: [&str; 5] = ["File", "Home", "Insert", "View", "Help"];

/// Height of the tab strip.
pub const TAB_STRIP_H: f32 = 30.0;

/// Tab strip plus the ribbon of the current tab.
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("tabs")
        .exact_size(TAB_STRIP_H)
        .frame(egui::Frame::NONE.fill(t.tab_strip).inner_margin(egui::Margin { left: 8, right: 10, top: 0, bottom: 0 }))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                for tab in TABS {
                    let w = ui.ctx().fonts_mut(|f| f.layout_no_wrap(tab.to_string(), medium(12.5), t.text).size().x) + 18.0;
                    let (r, resp) = ui.allocate_exact_size(vec2(w, TAB_STRIP_H), Sense::click());
                    let active = app.ui.tab == tab;
                    if resp.hovered() && !active {
                        ui.painter().rect_filled(r.shrink2(vec2(0.0, 4.0)), 4.0, t.hover);
                    }
                    let color = if active || tab == "File" { t.accent_text } else { t.text };
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, tab, if active { semibold(12.5) } else { medium(12.5) }, color);
                    if active {
                        let u = Rect::from_center_size(pos2(r.center().x, r.max.y - 2.0), vec2(w - 16.0, 3.0));
                        ui.painter().rect_filled(u, 2.0, t.accent);
                    }
                    if resp.clicked() {
                        let _ = app.run("ui.tab", json!({"tab": tab}));
                        if tab != "File" && app.ui.ribbon_collapsed {
                            // A collapsed ribbon shows again when a tab is picked.
                            let _ = app.run("ui.collapseRibbon", json!({"value": false}));
                        }
                    }
                    if resp.double_clicked() && tab != "File" {
                        let _ = app.run("ui.collapseRibbon", json!({}));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tip = if app.ui.ribbon_collapsed { "Show the ribbon" } else { "Collapse the ribbon" };
                    let icon = if app.ui.ribbon_collapsed { "plus" } else { "minus" };
                    if widgets::icon_button(ui, icon, tip, 12.0, true) {
                        let _ = app.run("ui.collapseRibbon", json!({}));
                    }
                });
            });
        });
    if app.ui.ribbon_collapsed {
        return;
    }
    egui::Panel::top("ribbon")
        .exact_size(CONTENT_H + LABEL_H + 10.0)
        .frame(
            egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin { left: 8, right: 8, top: 4, bottom: 4 }).stroke(Stroke::new(1.0, t.border)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::horizontal().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                    match app.ui.tab.as_str() {
                        "Insert" => insert(app, ui),
                        "View" => view(app, ui),
                        "Help" => help(app, ui),
                        _ => home(app, ui),
                    }
                });
            });
        });
}

fn home(app: &mut CraftApp, ui: &mut Ui) {
    group(ui, "Clipboard", app, |ui, app| {
        big(ui, app, "open", "Paste", "edit.paste", json!({}));
        stack(ui, |ui| {
            small(ui, app, "close", Some("Cut"), "Cut", "edit.cut", json!({}), false);
            small(ui, app, "new", Some("Copy"), "Copy", "edit.copy", json!({}), false);
            small(ui, app, "more", Some("Select All"), "Select All", "edit.selectAll", json!({}), false);
        });
    });
    group(ui, "Undo", app, |ui, app| {
        big(ui, app, "undo", "Undo", "edit.undo", json!({}));
        big(ui, app, "redo", "Redo", "edit.redo", json!({}));
    });
    group(ui, "Notes", app, |ui, app| {
        big(ui, app, "note", "New\nNote", "insert.note", json!({}));
        big(ui, app, "close", "Delete\nNote", "edit.deleteNote", json!({}));
    });
    group(ui, "File", app, |ui, app| {
        big(ui, app, "save", "Save", "file.save", json!({}));
        stack(ui, |ui| {
            small(ui, app, "new", Some("New"), "New", "file.new", json!({}), false);
            small(ui, app, "open", Some("Open"), "Open", "file.open", json!({}), false);
            small(ui, app, "save", Some("Save As"), "Save As", "file.saveAs", json!({}), false);
        });
    });
}

fn insert(app: &mut CraftApp, ui: &mut Ui) {
    group(ui, "Notes", app, |ui, app| {
        big(ui, app, "note", "Note", "insert.note", json!({}));
        menu_button(ui, app, "plus", "Note\nFrom", "Insert a note with content", true, |ui, app| {
            menu_item(ui, app, "Empty note", "insert.note", json!({}));
            menu_item(ui, app, "Today's date", "insert.note", json!({"title": "Today", "body": basecraft_engine::cmd::now_iso()}));
            menu_item(ui, app, "Checklist", "insert.note", json!({"title": "Checklist", "body": "[ ] First\n[ ] Second\n[ ] Third"}));
        });
    });
}

fn view(app: &mut CraftApp, ui: &mut Ui) {
    group(ui, "Show", app, |ui, app| {
        let nav = app.session.view.nav_pane;
        let props = app.session.view.props_pane;
        stack(ui, |ui| {
            small(ui, app, "more", Some("Navigation Pane"), "Navigation Pane", "view.navigationPane", json!({}), nav);
            small(ui, app, "info", Some("Properties Pane"), "Properties Pane", "view.propertiesPane", json!({}), props);
            let collapsed = app.ui.ribbon_collapsed;
            small(ui, app, "minus", Some("Collapse Ribbon"), "Collapse the Ribbon", "ui.collapseRibbon", json!({}), collapsed);
        });
    });
    group(ui, "Zoom", app, |ui, app| {
        big(ui, app, "search", "Zoom", "view.zoom", json!({}));
        big(ui, app, "plus", "Zoom\nIn", "view.zoomIn", json!({}));
        big(ui, app, "minus", "Zoom\nOut", "view.zoomOut", json!({}));
        big(ui, app, "info", "100%", "view.zoom100", json!({}));
    });
    group(ui, "Appearance", app, |ui, app| {
        let dark = app.session.view.dark_mode;
        let menus = app.ui.in_window_menus;
        stack(ui, |ui| {
            small(ui, app, "settings", Some("Dark Mode"), "Dark Mode", "view.dark", json!({}), dark);
            if cfg!(target_os = "macos") {
                small(ui, app, "more", Some("Menu Bar in Window"), "Menu Bar in Window", "ui.inWindowMenus", json!({}), menus);
            }
        });
    });
}

fn help(app: &mut CraftApp, ui: &mut Ui) {
    group(ui, "Help", app, |ui, app| {
        big(ui, app, "search", "Search\nCommands", "ui.commands", json!({}));
        big(ui, app, "info", "About", "help.about", json!({}));
    });
    group(ui, "Settings", app, |ui, app| {
        big(ui, app, "settings", "Preferences", "app.preferences", json!({}));
    });
}
