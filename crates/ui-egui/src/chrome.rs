//! Title bar (quick access toolbar, centred title, command search, account) and status bar
//! (document facts, status message, zoom slider).

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::theme::{Tokens, regular, semibold};
use crate::{CraftApp, icons, widgets};

/// Height of the title bar in points.
pub const TITLE_BAR_H: f32 = 38.0;
/// Height of the status bar in points.
pub const STATUS_BAR_H: f32 = 26.0;

/// A quick access toolbar button: icon only, runs `id`.
fn qat_button(ui: &mut Ui, app: &mut CraftApp, icon: &str, tip: &str, id: &str) {
    let enabled = widgets::enabled(app, id);
    let sc = widgets::shortcut_text(app, id);
    let tip = if sc.is_empty() { tip.to_string() } else { format!("{tip} ({sc})") };
    if widgets::icon_button(ui, icon, &tip, 16.0, enabled) {
        let _ = app.run(id, json!({}));
    }
}

/// The title bar: a top panel the window can be dragged by.
pub fn title_bar(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    // On macOS the traffic lights sit in our bar; leave them room.
    let left = if app.integrated_titlebar { 78 } else { 8 };
    egui::Panel::top("title_bar")
        .exact_size(TITLE_BAR_H)
        .frame(egui::Frame::NONE.fill(t.title_bar).inner_margin(egui::Margin { left, right: 10, top: 0, bottom: 0 }))
        .show(ui, |ui| {
            let full = ui.max_rect();
            // Drag the window by the title bar (there is no system one on macOS).
            let drag = ui.interact(full, ui.id().with("title_drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
            }
            let mut qat_end = full.min.x;
            let mut right_start = full.max.x;
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                qat_button(ui, app, "save", "Save", "file.save");
                let undo_tip = format!("Undo {}", app.session.undo_label().unwrap_or(""));
                qat_button(ui, app, "undo", undo_tip.trim(), "edit.undo");
                let redo_tip = format!("Redo {}", app.session.redo_label().unwrap_or(""));
                qat_button(ui, app, "redo", redo_tip.trim(), "edit.redo");
                qat_button(ui, app, "more", "All commands", "ui.commands");
                qat_end = ui.min_rect().max.x;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Account: initials of the user name.
                    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                    ui.painter().circle_filled(r.center(), 12.0, t.accent);
                    let initials: String = app.session.author.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, initials, semibold(10.5), t.on_accent);
                    if resp.on_hover_text(format!("{} (set your name in Preferences)", app.session.author)).clicked() {
                        let _ = app.run("app.preferences", json!({}));
                    }
                    ui.add_space(8.0);
                    // Command search.
                    let (r, resp) = ui.allocate_exact_size(vec2(260.0, 26.0), Sense::click());
                    ui.painter().rect(r, 6.0, if resp.hovered() { t.input } else { t.ribbon }, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
                    icons::paint(ui.painter(), Rect::from_min_size(r.min + vec2(8.0, 5.0), vec2(16.0, 16.0)), "search", t.text_dim, t.accent);
                    ui.painter().text(pos2(r.min.x + 30.0, r.center().y), Align2::LEFT_CENTER, "Search commands", regular(12.0), t.text_dim);
                    if resp.clicked() {
                        let _ = app.run("ui.commands", json!({}));
                    }
                    right_start = ui.min_rect().min.x;
                });
            });
            // Centred title, drawn only when it fits between the toolbar and the search box.
            let state = if app.session.dirty {
                "Edited"
            } else if app.session.path.is_some() {
                "Saved"
            } else {
                "Not saved"
            };
            let title = format!("{} - {state}", app.title_stem());
            let g = ui.ctx().fonts_mut(|f| f.layout_no_wrap(title, semibold(12.5), t.text));
            let w = g.size().x;
            let cx = full.center().x;
            if cx - w / 2.0 > qat_end + 10.0 && cx + w / 2.0 < right_start - 10.0 {
                ui.painter().galley(pos2(cx - w / 2.0, full.center().y - g.size().y / 2.0), g, t.text);
            }
        });
}

/// A status bar icon button for a command. Its name carries the shortcut (`Zoom In (Cmd =)`),
/// which also keeps it apart from the ribbon's button of the same label in tests.
fn status_button(ui: &mut Ui, app: &mut CraftApp, icon: &str, id: &str) {
    let label = widgets::label_of(app, id);
    let sc = widgets::shortcut_text(app, id);
    let tip = if sc.is_empty() { label.to_string() } else { format!("{label} ({sc})") };
    if widgets::icon_button(ui, icon, &tip, 12.0, widgets::enabled(app, id)) {
        let _ = app.run(id, json!({}));
    }
}

/// The status bar: note position and size on the left, status message, zoom on the right.
pub fn status_bar(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("status")
        .exact_size(STATUS_BAR_H)
        .frame(
            egui::Frame::NONE
                .fill(t.status_bar)
                .inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 0 })
                .stroke(Stroke::new(1.0, t.border)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing = vec2(14.0, 0.0);
                let st = |ui: &mut Ui, s: &str| {
                    ui.add(egui::Label::new(egui::RichText::new(s).font(regular(11.5)).color(t.text_dim)).sense(Sense::click()))
                };
                let n = app.session.doc.notes.len();
                st(ui, &format!("Note {} of {n}", app.session.sel.note + 1));
                let chars = app.session.doc.char_count();
                let sel = app.session.selected_text().chars().count();
                let txt = if sel > 0 { format!("{sel} of {chars} characters") } else { format!("{chars} characters") };
                st(ui, &txt);
                if let Some((msg, at)) = &app.status_msg
                    && crate::now_ms() - at < 6000.0
                {
                    ui.label(egui::RichText::new(msg).font(regular(11.5)).color(t.accent_text));
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                    let z = (app.session.view.zoom * 100.0).round();
                    if st(ui, &format!("{z}%")).on_hover_text("Zoom").clicked() {
                        let _ = app.run("ui.dialog", json!({"name": "zoom"}));
                    }
                    status_button(ui, app, "plus", "view.zoomIn");
                    let mut v = app.session.view.zoom;
                    let range = protocraft_engine::cmd::view::MIN_ZOOM / 100.0..=protocraft_engine::cmd::view::MAX_ZOOM / 100.0;
                    ui.spacing_mut().slider_width = 110.0;
                    if ui.add(egui::Slider::new(&mut v, range).logarithmic(true).show_value(false)).changed() {
                        let _ = app.run("view.zoom", json!({"value": (v * 100.0).round()}));
                    }
                    status_button(ui, app, "minus", "view.zoomOut");
                });
            });
        });
}
