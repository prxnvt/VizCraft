//! Dialogs: Quit (Save / Don't Save / Cancel), About, Preferences, Zoom and the command
//! palette. Every dialog ends by running a command, so agents get the same result without it.

use egui::{Ui, vec2};
use serde::Serialize;
use serde_json::json;

use crate::theme::{Tokens, semibold};
use crate::{CraftApp, UI_COMMANDS, widgets};

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
    /// The command palette.
    Commands {
        query: String,
    },
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
        }
    }

    /// A dialog by name, seeded from the app's state. `None` for an unknown name.
    pub fn open(name: &str, app: &CraftApp) -> Option<Dialog> {
        Some(match name {
            "quit" => Dialog::Quit,
            "about" => Dialog::About,
            "preferences" => Dialog::Preferences { author: app.session.author.clone() },
            "zoom" => Dialog::Zoom { percent: (app.session.view.zoom * 100.0).round() },
            "commands" => Dialog::Commands { query: String::new() },
            _ => return None,
        })
    }

    fn title(&self) -> &'static str {
        match self {
            Dialog::Quit => "ProtoCraft",
            Dialog::About => "About ProtoCraft",
            Dialog::Preferences { .. } => "Preferences",
            Dialog::Zoom { .. } => "Zoom",
            Dialog::Commands { .. } => "Search Commands",
        }
    }
}

/// Show the open dialog, if any, centred over the window.
pub fn show(app: &mut CraftApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let mut open = true;
    let mut close = false;
    egui::Window::new(d.title()).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -40.0)).open(&mut open).show(
        ctx,
        |ui| {
            ui.set_min_width(340.0);
            close = body(app, ui, &mut d);
        },
    );
    // A dialog opened by the body (About from the palette, say) wins over the one closing.
    if app.dialog.is_none() && open && !close {
        app.dialog = Some(d);
    }
}

/// OK and Cancel buttons; returns (ok, cancel). Enter and Escape press them.
fn buttons(ui: &mut Ui, ok: &str) -> (bool, bool) {
    let mut r = (false, false);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(egui::Button::new("Cancel").min_size(vec2(72.0, 24.0))).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                r.1 = true;
            }
            if widgets::primary_button(ui, ok).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                r.0 = true;
            }
        });
    });
    r
}

/// Returns true to close.
fn body(app: &mut CraftApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    match d {
        Dialog::Quit => {
            if !app.session.dirty {
                app.quit_confirmed = true;
                app.quit_requested = true;
                return true;
            }
            ui.label(egui::RichText::new(format!("Do you want to save the changes you made to \u{201c}{}\u{201d}?", app.title_stem())).strong());
            ui.label("Your changes will be lost if you don't save them.");
            ui.add_space(8.0);
            let mut done = false;
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new("Don't Save").min_size(vec2(84.0, 24.0))).clicked() {
                    app.quit_confirmed = true;
                    app.quit_requested = true;
                    done = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(ui, "Save").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        // With no file yet this runs Save As (a native picker); cancelling it
                        // leaves the document dirty and the window open.
                        let _ = app.run("file.save", json!({}));
                        if !app.session.dirty {
                            app.quit_confirmed = true;
                            app.quit_requested = true;
                        }
                        done = true;
                    }
                    if ui.add(egui::Button::new("Cancel").min_size(vec2(72.0, 24.0))).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        done = true;
                    }
                });
            });
            done
        }
        Dialog::About => {
            ui.label(egui::RichText::new("ProtoCraft").font(semibold(22.0)));
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("The baseline every craft app starts from: window chrome, menus, ribbon, panes,\npreferences and a control channel, over a tiny sample document.");
            ui.add_space(6.0);
            ui.hyperlink_to("Source code: github.com/storytold/protocraft", "https://github.com/storytold/protocraft");
            ui.add_space(6.0);
            ui.label(egui::RichText::new("MIT OR Apache-2.0. Fonts: Inter and JetBrains Mono (SIL Open Font License).").small().weak());
            let (ok, cancel) = buttons(ui, "Close");
            ok || cancel
        }
        Dialog::Preferences { author } => {
            egui::Grid::new("prefs").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
                ui.label("User name:");
                ui.add(egui::TextEdit::singleline(author).desired_width(220.0));
                ui.end_row();
                ui.label("Appearance:");
                let mut dark = app.session.view.dark_mode;
                if ui.checkbox(&mut dark, "Dark mode").changed() {
                    let _ = app.run("view.dark", json!({"value": dark}));
                }
                ui.end_row();
                if cfg!(target_os = "macos") {
                    ui.label("Menus:");
                    let mut inwin = app.ui.in_window_menus;
                    if ui.checkbox(&mut inwin, "Show the menu bar in the window").changed() {
                        let _ = app.run("ui.inWindowMenus", json!({"value": inwin}));
                    }
                    ui.end_row();
                }
            });
            let (ok, cancel) = buttons(ui, "OK");
            if ok && !author.trim().is_empty() {
                let _ = app.run("file.setAuthor", json!({"name": author}));
            }
            ok || cancel
        }
        Dialog::Zoom { percent } => {
            ui.horizontal(|ui| {
                for p in [200.0, 100.0, 75.0] {
                    ui.radio_value(percent, p, format!("{p}%"));
                }
            });
            ui.add(egui::Slider::new(percent, protocraft_engine::cmd::view::MIN_ZOOM..=protocraft_engine::cmd::view::MAX_ZOOM).suffix("%"));
            let (ok, cancel) = buttons(ui, "OK");
            if ok {
                let _ = app.run("view.zoom", json!({"value": *percent}));
            }
            ok || cancel
        }
        Dialog::Commands { query } => {
            let r = ui.add(egui::TextEdit::singleline(query).hint_text("Type a command, e.g. \"zoom\"").desired_width(380.0));
            r.request_focus();
            let q = query.to_lowercase();
            // Engine commands, then UI ones: (id, label, location).
            let mut hits: Vec<(String, String, String)> = app
                .session
                .commands()
                .into_iter()
                .map(|c| (c.id, c.label, c.location))
                .chain(UI_COMMANDS.iter().map(|(id, label, ..)| (id.to_string(), label.to_string(), "UI".to_string())))
                .filter(|(id, label, loc)| q.is_empty() || label.to_lowercase().contains(&q) || id.contains(&q) || loc.to_lowercase().contains(&q))
                .collect();
            hits.truncate(14);
            let mut close = false;
            let t = Tokens::get(ui.ctx());
            for (id, label, loc) in hits {
                let sc = widgets::shortcut_text(app, &id);
                let on = widgets::enabled(app, &id);
                let text = egui::RichText::new(format!("{label}   {loc}")).color(if on { t.text } else { t.text_disabled });
                if ui.add_enabled(on, egui::Button::new(text).shortcut_text(sc).min_size(vec2(380.0, 0.0))).on_hover_text(&id).clicked() {
                    let _ = app.run(&id, json!({}));
                    close = true;
                }
            }
            close || ui.input(|i| i.key_pressed(egui::Key::Escape))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialogs_open_by_name_and_report_it() {
        let app = CraftApp::new(protocraft_engine::Session::new(protocraft_engine::Document::new()), crate::Services::default());
        for name in ["quit", "about", "preferences", "zoom", "commands"] {
            let d = Dialog::open(name, &app).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(d.name(), name);
            let v = serde_json::to_value(&d).unwrap();
            assert_eq!(v["dialog"], name);
        }
        assert_eq!(Dialog::open("nothing", &app), None);
    }
}
