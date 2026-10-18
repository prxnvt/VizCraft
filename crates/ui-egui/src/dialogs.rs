//! Dialogs: Quit (Save / Don't Save / Cancel), About, Preferences, Zoom, the command palette,
//! Keyboard Shortcuts and a generic Message. Every dialog ends by running a command, so agents
//! get the same result without it.
//!
//! All of them share one modal frame ([`show`]): a dimmed backdrop, a centred window with a
//! title, buttons right-aligned at the bottom, Escape (or a click on the backdrop) cancels and
//! Enter presses the default button. Colours come from [`Tokens`] only.

use egui::{Align2, Id, Sense, Ui, vec2};
use serde::Serialize;
use serde_json::json;

use crate::theme::{Tokens, regular, semibold};
use crate::{CraftApp, UI_COMMANDS, keys, menus, widgets};

/// The open dialog and its editable state.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "dialog", rename_all = "camelCase")]
pub enum Dialog {
    /// Unsaved changes on close: Save / Don't Save / Cancel.
    Quit,
    About,
    Preferences {
        author: String,
    },
    Zoom {
        percent: f32,
    },
    /// The command palette: a search box over every command, arrows pick, Enter runs.
    Commands {
        query: String,
        selected: usize,
    },
    /// The Keyboard Shortcuts table, with a filter box.
    Shortcuts {
        filter: String,
    },
    /// A message (an error, a notice) with a Close button.
    Message {
        title: String,
        text: String,
    },
    /// Page Setup for the active page (`crate::page_setup`).
    PageSetup(crate::page_setup::PageSetup),
    /// Snap and Glue: what dragged items snap to, and how strongly.
    SnapSettings(crate::snap_dialog::State),
    /// Define Shape Data: a data key for the active page.
    DefineShapeData(crate::data_dialog::State),
    /// Print Preview: pages, paper, fit, the sheets each page prints on, and Print.
    PrintPreview(crate::print_preview::State),
}

impl Dialog {
    /// The name `ui.dialog {name}` opens it by.
    pub fn name(&self) -> &'static str {
        match self {
            Dialog::Quit => "quit",
            Dialog::About => "about",
            Dialog::Preferences { .. } => "preferences",
            Dialog::Zoom { .. } => "zoom",
            Dialog::Commands { .. } => "commands",
            Dialog::Shortcuts { .. } => "shortcuts",
            Dialog::Message { .. } => "message",
            Dialog::PageSetup(_) => "pageSetup",
            Dialog::SnapSettings(_) => "snapSettings",
            Dialog::DefineShapeData(_) => "defineShapeData",
            Dialog::PrintPreview(_) => "printPreview",
        }
    }

    /// A dialog by name, seeded from the app's state. `None` for an unknown name (`message`
    /// needs a title and text: `ui.dialog {"name": "message", "title", "text"}` builds it).
    pub fn open(name: &str, app: &CraftApp) -> Option<Dialog> {
        Some(match name {
            "quit" => Dialog::Quit,
            "about" => Dialog::About,
            "preferences" => Dialog::Preferences { author: app.session.author.clone() },
            "zoom" => Dialog::Zoom { percent: (app.session.view.zoom * 100.0).round() },
            "commands" => Dialog::Commands { query: String::new(), selected: 0 },
            "shortcuts" => Dialog::Shortcuts { filter: String::new() },
            "pageSetup" => Dialog::PageSetup(crate::page_setup::PageSetup::new(app)),
            "snapSettings" => Dialog::SnapSettings(crate::snap_dialog::open(app)),
            "defineShapeData" => Dialog::DefineShapeData(crate::data_dialog::open(app)),
            "printPreview" => Dialog::PrintPreview(crate::print_preview::open(app)),
            _ => return None,
        })
    }

    fn title(&self) -> String {
        match self {
            Dialog::Quit => "Unsaved Changes".into(),
            Dialog::About => "About VizCraft".into(),
            Dialog::Preferences { .. } => "Preferences".into(),
            Dialog::Zoom { .. } => "Zoom".into(),
            Dialog::Commands { .. } => "Command Palette".into(),
            Dialog::Shortcuts { .. } => "Keyboard Shortcuts".into(),
            Dialog::Message { title, .. } => title.clone(),
            Dialog::PageSetup(_) => "Page Setup".into(),
            Dialog::SnapSettings(_) => "Snap and Glue".into(),
            Dialog::DefineShapeData(_) => "Define Shape Data".into(),
            Dialog::PrintPreview(_) => "Print Preview".into(),
        }
    }

    fn width(&self) -> f32 {
        match self {
            Dialog::Commands { .. } => 520.0,
            Dialog::Shortcuts { .. } => 460.0,
            Dialog::About => 400.0,
            Dialog::PrintPreview(_) => 560.0,
            _ => 360.0,
        }
    }
}

/// Keys read once at the top of a frame, before any text field consumes them.
#[derive(Clone, Copy)]
struct Keys {
    enter: bool,
    escape: bool,
    up: bool,
    down: bool,
}

/// Show the open dialog, if any, as a modal over the window.
pub fn show(app: &mut CraftApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let t = Tokens::get(ctx);
    let keys = ctx.input(|i| Keys {
        enter: i.key_pressed(egui::Key::Enter),
        escape: i.key_pressed(egui::Key::Escape),
        up: i.key_pressed(egui::Key::ArrowUp),
        down: i.key_pressed(egui::Key::ArrowDown),
    });
    // The dialog owns these keys; nothing under it (the canvas, the ribbon) acts on them.
    ctx.input_mut(|i| {
        for k in [egui::Key::Enter, egui::Key::Escape, egui::Key::ArrowUp, egui::Key::ArrowDown] {
            i.consume_key(egui::Modifiers::NONE, k);
        }
    });
    let frame = egui::Frame::NONE
        .fill(t.menu)
        .stroke(egui::Stroke::new(1.0, t.border_strong))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(18))
        .shadow(ctx.global_style().visuals.window_shadow);
    let width = d.width();
    let title = d.title();
    let modal = egui::Modal::new(Id::new("craft_dialog")).frame(frame).backdrop_color(t.shadow).show(ctx, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        ui.label(egui::RichText::new(title).font(semibold(15.0)).color(t.text));
        ui.add_space(2.0);
        body(app, ui, &mut d, keys)
    });
    let close = modal.inner || modal.backdrop_response.clicked() || keys.escape;
    // A dialog opened by the body (About from the palette, say) wins over the one closing.
    if app.dialog.is_none() && !close {
        app.dialog = Some(d);
    }
}

/// The button row: the default action on the right, then Cancel. Returns (ok, cancel); Enter
/// and Escape press them.
fn buttons(ui: &mut Ui, ok: &str, cancel: Option<&str>, keys: Keys) -> (bool, bool) {
    let mut r = (false, false);
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if widgets::primary_button(ui, ok).clicked() || keys.enter {
            r.0 = true;
        }
        if let Some(c) = cancel
            && (ui.add(egui::Button::new(c).min_size(vec2(72.0, 24.0))).clicked() || keys.escape)
        {
            r.1 = true;
        }
    });
    r
}

/// Returns true to close.
fn body(app: &mut CraftApp, ui: &mut Ui, d: &mut Dialog, keys: Keys) -> bool {
    let t = Tokens::get(ui.ctx());
    match d {
        Dialog::Quit => {
            if !app.session.dirty {
                app.quit_confirmed = true;
                app.quit_requested = true;
                return true;
            }
            ui.label(egui::RichText::new(format!("Do you want to save the changes you made to \u{201c}{}\u{201d}?", app.title_stem())).color(t.text));
            ui.label(egui::RichText::new("Your changes will be lost if you don't save them.").color(t.text_dim));
            ui.add_space(8.0);
            let mut done = false;
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new("Don't Save").min_size(vec2(84.0, 24.0))).clicked() {
                    app.quit_confirmed = true;
                    app.quit_requested = true;
                    done = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(ui, "Save").clicked() || keys.enter {
                        // With no file yet this runs Save As (a native picker); cancelling it
                        // leaves the document dirty and the window open.
                        let _ = app.run("file.save", json!({}));
                        if !app.session.dirty {
                            app.quit_confirmed = true;
                            app.quit_requested = true;
                        }
                        done = true;
                    }
                    if ui.add(egui::Button::new("Cancel").min_size(vec2(72.0, 24.0))).clicked() || keys.escape {
                        done = true;
                    }
                });
            });
            done
        }
        Dialog::About => {
            ui.label(egui::RichText::new("VizCraft").font(semibold(22.0)).color(t.text));
            ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(t.text_dim));
            ui.label(
                egui::RichText::new(
                    "A diagramming app for flowcharts, org charts and system designs,\nwith a control channel for scripts and agents.",
                )
                .color(t.text),
            );
            // The link comes from the package metadata, so a fork links to its own repository (or
            // shows none) instead of VizCraft's.
            let repo = env!("CARGO_PKG_REPOSITORY");
            if !repo.is_empty() {
                ui.add_space(4.0);
                let shown = repo.trim_start_matches("https://").trim_start_matches("http://");
                ui.hyperlink_to(format!("Source code: {shown}"), repo);
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Licensed under MIT OR Apache-2.0. Fonts: Inter and JetBrains Mono (SIL Open Font License).")
                    .font(regular(11.0))
                    .color(t.text_dim),
            );
            let (ok, cancel) = buttons(ui, "Close", None, keys);
            ok || cancel
        }
        Dialog::Preferences { author } => {
            egui::Grid::new("prefs").num_columns(2).spacing(vec2(12.0, 10.0)).show(ui, |ui| {
                ui.label(egui::RichText::new("User name").color(t.text_dim));
                ui.add(egui::TextEdit::singleline(author).desired_width(220.0));
                ui.end_row();
                ui.label(egui::RichText::new("Appearance").color(t.text_dim));
                let mut dark = app.session.view.dark_mode;
                if ui.checkbox(&mut dark, "Dark mode").changed() {
                    let _ = app.run("view.dark", json!({"value": dark}));
                }
                ui.end_row();
                if cfg!(target_os = "macos") {
                    ui.label(egui::RichText::new("Menus").color(t.text_dim));
                    let mut inwin = app.ui.in_window_menus;
                    if ui.checkbox(&mut inwin, "Show the menu bar in the window").changed() {
                        let _ = app.run("ui.inWindowMenus", json!({"value": inwin}));
                    }
                    ui.end_row();
                }
                ui.label(egui::RichText::new("Panes").color(t.text_dim));
                if ui.button("Reset pane layout").on_hover_text("Every pane back on its default side at its default size").clicked() {
                    let _ = app.run("ui.panesReset", json!({}));
                }
                ui.end_row();
            });
            let (ok, cancel) = buttons(ui, "OK", Some("Cancel"), keys);
            if ok && !author.trim().is_empty() && *author != app.session.author {
                let _ = app.run("file.setAuthor", json!({"name": author}));
            }
            ok || cancel
        }
        Dialog::Zoom { percent } => {
            ui.horizontal(|ui| {
                for p in [200.0, 100.0, 75.0, 50.0] {
                    ui.radio_value(percent, p, format!("{p}%"));
                }
            });
            ui.spacing_mut().slider_width = 220.0;
            let range = vizcraft_engine::cmd::view::MIN_ZOOM..=vizcraft_engine::cmd::view::MAX_ZOOM;
            widgets::slider(ui, egui::Slider::new(percent, range).suffix("%"), 320.0);
            let (ok, cancel) = buttons(ui, "OK", Some("Cancel"), keys);
            if ok {
                let _ = app.run("view.zoom", json!({"value": *percent}));
            }
            ok || cancel
        }
        Dialog::Commands { query, selected } => palette(app, ui, query, selected, keys),
        Dialog::PrintPreview(s) => {
            crate::print_preview::body(app, ui, s);
            let (print, cancel) = buttons(ui, "Print", Some("Cancel"), keys);
            crate::print_preview::finish(app, s, print, cancel)
        }
        Dialog::SnapSettings(s) => {
            crate::snap_dialog::body(ui, s);
            let (ok, cancel) = buttons(ui, "OK", Some("Cancel"), keys);
            crate::snap_dialog::finish(app, s, ok, cancel)
        }
        Dialog::DefineShapeData(s) => {
            crate::data_dialog::body(ui, s);
            let (ok, cancel) = buttons(ui, "OK", Some("Cancel"), keys);
            crate::data_dialog::finish(app, s, ok, cancel)
        }
        Dialog::Shortcuts { filter } => {
            let r = ui.add(egui::TextEdit::singleline(filter).hint_text("Filter").desired_width(f32::INFINITY));
            if filter.is_empty() {
                r.request_focus();
            }
            let q = filter.to_lowercase();
            let rows: Vec<(String, String)> = keys::shortcut_table(app)
                .into_iter()
                .filter(|(l, k)| q.is_empty() || l.to_lowercase().contains(&q) || k.to_lowercase().contains(&q))
                .collect();
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                egui::Grid::new("shortcuts").num_columns(2).spacing(vec2(24.0, 4.0)).min_col_width(200.0).show(ui, |ui| {
                    for (label, keys_text) in &rows {
                        ui.label(egui::RichText::new(label).color(t.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(keys_text).color(t.text_dim));
                        });
                        ui.end_row();
                    }
                });
                if rows.is_empty() {
                    ui.label(egui::RichText::new("No shortcut matches.").color(t.text_dim));
                }
            });
            let (ok, cancel) = buttons(ui, "Close", None, keys);
            ok || cancel
        }
        Dialog::Message { text, .. } => {
            ui.label(egui::RichText::new(text.as_str()).color(t.text));
            let (ok, cancel) = buttons(ui, "Close", None, keys);
            ok || cancel
        }
        Dialog::PageSetup(p) => {
            crate::page_setup::fields(ui, p);
            let (ok, cancel) = buttons(ui, "OK", Some("Cancel"), keys);
            if ok {
                crate::page_setup::apply(app, p);
            }
            ok || cancel
        }
    }
}

/// One palette hit: id, label, location, platform shortcut, enabled.
struct Hit {
    id: String,
    label: String,
    location: String,
    shortcut: String,
    enabled: bool,
}

/// The commands matching `query`, best first: label prefix matches, then label, id and location
/// substring matches. Capped so the list stays readable.
fn palette_hits(app: &CraftApp, query: &str) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    let mut hits: Vec<(u8, Hit)> = app
        .session
        .commands()
        .into_iter()
        .map(|c| (c.id, c.label, c.location, c.shortcut))
        .chain(UI_COMMANDS.iter().map(|(id, label, sc, _)| (id.to_string(), label.to_string(), "UI".to_string(), sc.to_string())))
        .filter_map(|(id, label, location, sc)| {
            let l = label.to_lowercase();
            let rank = if q.is_empty() || l.starts_with(&q) {
                0
            } else if l.contains(&q) {
                1
            } else if id.to_lowercase().contains(&q) || location.to_lowercase().contains(&q) {
                2
            } else {
                return None;
            };
            let enabled = !menus::PLACEHOLDERS.contains(&id.as_str()) && widgets::enabled(app, &id);
            Some((rank, Hit { id, label, location, shortcut: keys::pretty(&sc), enabled }))
        })
        .collect();
    hits.sort_by_key(|(rank, _)| *rank);
    hits.into_iter().map(|(_, h)| h).take(40).collect()
}

/// The command palette body. Returns true to close.
fn palette(app: &mut CraftApp, ui: &mut Ui, query: &mut String, selected: &mut usize, keys: Keys) -> bool {
    let t = Tokens::get(ui.ctx());
    let r = ui.add(egui::TextEdit::singleline(query).hint_text("Type a command, e.g. \"zoom\"").desired_width(f32::INFINITY).font(regular(14.0)));
    if r.changed() {
        *selected = 0;
    }
    r.request_focus();
    let hits = palette_hits(app, query);
    if hits.is_empty() {
        *selected = 0;
    } else {
        if keys.down {
            *selected = (*selected + 1).min(hits.len() - 1);
        }
        if keys.up {
            *selected = selected.saturating_sub(1);
        }
        *selected = (*selected).min(hits.len() - 1);
    }
    let mut run: Option<String> = None;
    egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
        for (k, h) in hits.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, h.enabled, k == *selected, h.label.as_str()));
            if k == *selected {
                ui.painter().rect_filled(rect, 4.0, if h.enabled { t.checked } else { t.hover });
                if keys.down || keys.up {
                    ui.scroll_to_rect(rect, None);
                }
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 4.0, t.hover);
            }
            let color = if h.enabled { t.text } else { t.text_disabled };
            let g = ui.painter().text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, &h.label, regular(12.5), color);
            ui.painter().text(g.right_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, &h.location, regular(11.0), t.text_dim);
            if !h.shortcut.is_empty() {
                ui.painter().text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, &h.shortcut, regular(12.0), t.text_dim);
            }
            let resp = resp.on_hover_text(&h.id);
            if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                *selected = k;
            }
            if resp.clicked() && h.enabled {
                run = Some(h.id.clone());
            }
        }
        if hits.is_empty() {
            ui.label(egui::RichText::new("No command matches.").color(t.text_dim));
        }
    });
    if keys.enter
        && let Some(h) = hits.get(*selected)
        && h.enabled
    {
        run = Some(h.id.clone());
    }
    let Some(id) = run else { return false };
    let ctx = ui.ctx().clone();
    menus::run_item(app, &ctx, &id, json!({}));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default())
    }

    #[test]
    fn dialogs_open_by_name_and_report_it() {
        let app = app();
        for name in ["quit", "about", "preferences", "zoom", "commands", "shortcuts"] {
            let d = Dialog::open(name, &app).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(d.name(), name);
            let v = serde_json::to_value(&d).unwrap();
            assert_eq!(v["dialog"], name);
        }
        assert_eq!(Dialog::open("nothing", &app), None);
        let m = Dialog::Message { title: "Oops".into(), text: "No.".into() };
        assert_eq!(m.name(), "message");
        assert_eq!(m.title(), "Oops");
    }

    #[test]
    fn message_dialog_opens_through_the_command() {
        let mut a = app();
        a.run("ui.dialog", json!({"name": "message", "title": "Import failed", "text": "The file is not a document."})).unwrap();
        assert_eq!(a.dialog.as_ref().map(|d| d.name()), Some("message"));
        a.run("ui.dialog", json!({"name": "shortcuts"})).unwrap();
        assert_eq!(a.dialog.as_ref().map(|d| d.name()), Some("shortcuts"));
    }

    #[test]
    fn palette_ranks_prefix_matches_first_and_marks_placeholders() {
        let a = app();
        let hits = palette_hits(&a, "zoom");
        assert!(hits.first().is_some_and(|h| h.label.to_lowercase().starts_with("zoom")), "{:?}", hits.iter().map(|h| &h.label).collect::<Vec<_>>());
        let print = palette_hits(&a, "print");
        assert!(print.iter().any(|h| h.id == "ui.print" && h.enabled));
        let find = palette_hits(&a, "find");
        assert!(find.iter().any(|h| h.id == "ui.find" && !h.enabled));
        assert!(palette_hits(&a, "").len() <= 40);
    }
}
