//! Ribbon widgets: large and small command buttons, groups, menu buttons, tooltips.
//! Each button runs a command by id, so the ribbon never has behaviour of its own.

use egui::{Align2, CornerRadius, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use crate::theme::{Tokens, regular};
use crate::{CraftApp, UI_COMMANDS, icons};

/// Height of the ribbon content area (without group labels).
pub const CONTENT_H: f32 = 66.0;
/// Height of the group label row under the content.
pub const LABEL_H: f32 = 16.0;

/// The portable shortcut string for a command (engine or UI), first alternative only.
pub fn shortcut_of(app: &CraftApp, id: &str) -> &'static str {
    let sc = match app.session.registry.get(id) {
        Some(spec) => spec.shortcut,
        None => UI_COMMANDS.iter().find(|(i, ..)| *i == id).map(|(_, _, sc, _)| *sc).unwrap_or(""),
    };
    sc.split(" / ").next().unwrap_or("")
}

/// Shortcut text for a command as the platform shows it (`Cmd S` glyphs on macOS, `Ctrl+S` elsewhere).
pub fn shortcut_text(app: &CraftApp, id: &str) -> String {
    let sc = shortcut_of(app, id);
    if sc.is_empty() {
        return String::new();
    }
    if cfg!(target_os = "macos") {
        sc.replace("Mod+", "\u{2318}").replace("Shift+", "\u{21e7}").replace("Alt+", "\u{2325}").replace("Ctrl+", "\u{2303}")
    } else {
        sc.replace("Mod+", "Ctrl+")
    }
}

/// The label a command is registered with (engine or UI).
pub fn label_of(app: &CraftApp, id: &str) -> &'static str {
    match app.session.registry.get(id) {
        Some(spec) => spec.label,
        None => UI_COMMANDS.iter().find(|(i, ..)| *i == id).map(|(_, l, ..)| *l).unwrap_or(""),
    }
}

/// Whether a command exists and is enabled. UI commands are always enabled.
pub fn enabled(app: &CraftApp, id: &str) -> bool {
    match app.session.registry.get(id) {
        Some(s) => (s.enabled)(&app.session).is_none(),
        None => UI_COMMANDS.iter().any(|(i, ..)| *i == id),
    }
}

/// Why a command is disabled, if it is.
pub fn disabled_reason(app: &CraftApp, id: &str) -> Option<&'static str> {
    app.session.registry.get(id).and_then(|s| (s.enabled)(&app.session))
}

/// A tooltip with the label, shortcut, location and (when disabled) the reason.
pub fn tooltip(app: &CraftApp, resp: Response, label: &str, id: &str) -> Response {
    let sc = shortcut_text(app, id);
    let desc = app.session.registry.get(id).map(|s| s.location).unwrap_or("");
    let reason = disabled_reason(app, id);
    resp.on_hover_ui(|ui| {
        ui.set_max_width(260.0);
        let head = if sc.is_empty() { label.to_string() } else { format!("{label} ({sc})") };
        ui.label(egui::RichText::new(head).font(crate::theme::semibold(12.0)));
        if !desc.is_empty() {
            ui.label(egui::RichText::new(desc).small().weak());
        }
        if let Some(why) = reason {
            ui.label(egui::RichText::new(format!("Not available: {why}")).small().weak());
        }
    })
}

/// Hover, pressed and checked backgrounds shared by every button.
fn bg(ui: &Ui, r: Rect, resp: &Response, checked: bool, t: &Tokens) {
    let fill = if resp.is_pointer_button_down_on() {
        t.pressed
    } else if checked {
        t.checked
    } else if resp.hovered() {
        t.hover
    } else {
        return;
    };
    ui.painter().rect_filled(r, CornerRadius::same(4), fill);
    if checked {
        ui.painter().rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, t.accent.linear_multiply(0.5)), egui::StrokeKind::Inside);
    }
}

fn text_width(ui: &Ui, s: &str, font: egui::FontId) -> f32 {
    let t = Tokens::get(ui.ctx());
    ui.ctx().fonts_mut(|f| f.layout_no_wrap(s.to_string(), font, t.text).size().x)
}

/// A large ribbon button: 32 px icon over a (possibly two-line) label.
pub fn big(ui: &mut Ui, app: &mut CraftApp, icon: &str, label: &str, id: &str, params: Value) -> Response {
    let t = Tokens::get(ui.ctx());
    let galley_w = label.split('\n').map(|l| text_width(ui, l, regular(11.5))).fold(0.0, f32::max);
    let w = (galley_w + 12.0).max(44.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, CONTENT_H), Sense::click());
    let on = enabled(app, id);
    bg(ui, r, &resp, false, &t);
    let ic = Rect::from_center_size(pos2(r.center().x, r.min.y + 20.0), vec2(32.0, 32.0));
    let (c, a) = if on { (t.icon, t.accent) } else { (t.text_disabled, t.text_disabled) };
    icons::paint(ui.painter(), ic, icon, c, a);
    let mut y = r.min.y + 43.0;
    for l in label.split('\n') {
        ui.painter().text(pos2(r.center().x, y), Align2::CENTER_TOP, l, regular(11.5), if on { t.text } else { t.text_disabled });
        y += 13.0;
    }
    let resp = tooltip(app, resp, &label.replace('\n', " "), id);
    if resp.clicked() && on {
        let _ = app.run(id, params);
    }
    resp
}

/// A small button: 16 px icon with an optional label; `checked` draws the toggled state.
pub fn small(ui: &mut Ui, app: &mut CraftApp, icon: &str, label: Option<&str>, tip: &str, id: &str, params: Value, checked: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let text_w = label.map(|l| text_width(ui, l, regular(11.5)) + 6.0).unwrap_or(0.0);
    let (r, resp) = ui.allocate_exact_size(vec2(24.0 + text_w, 22.0), Sense::click());
    let on = enabled(app, id);
    bg(ui, r, &resp, checked, &t);
    let ic = Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(17.0, 17.0));
    let (c, a) = if on { (t.icon, t.accent) } else { (t.text_disabled, t.text_disabled) };
    icons::paint(ui.painter(), ic, icon, c, a);
    if let Some(l) = label {
        ui.painter().text(pos2(r.min.x + 24.0, r.center().y), Align2::LEFT_CENTER, l, regular(11.5), if on { t.text } else { t.text_disabled });
    }
    let resp = tooltip(app, resp, tip, id);
    if resp.clicked() && on {
        let _ = app.run(id, params);
    }
    resp
}

/// A button that only opens a menu (icon, label and a dropdown arrow).
pub fn menu_button(ui: &mut Ui, app: &mut CraftApp, icon: &str, label: &str, tip: &str, big_btn: bool, menu: impl FnOnce(&mut Ui, &mut CraftApp)) {
    let t = Tokens::get(ui.ctx());
    let resp = if big_btn {
        let galley_w = label.split('\n').map(|l| text_width(ui, l, regular(11.5))).fold(0.0, f32::max);
        let w = (galley_w + 22.0).max(44.0);
        let (r, resp) = ui.allocate_exact_size(vec2(w, CONTENT_H), Sense::click());
        bg(ui, r, &resp, false, &t);
        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.center().x, r.min.y + 20.0), vec2(32.0, 32.0)), icon, t.icon, t.accent);
        let lines: Vec<&str> = label.split('\n').collect();
        let mut y = r.min.y + 43.0;
        for (i, l) in lines.iter().enumerate() {
            let g = ui.painter().text(pos2(r.center().x, y), Align2::CENTER_TOP, *l, regular(11.5), t.text);
            if i + 1 == lines.len() {
                icons::paint(ui.painter(), Rect::from_center_size(pos2(g.max.x + 6.0, g.center().y), vec2(9.0, 9.0)), "dropdown", t.icon, t.accent);
            }
            y += 13.0;
        }
        resp
    } else {
        let text_w = text_width(ui, label, regular(11.5)) + 6.0;
        let (r, resp) = ui.allocate_exact_size(vec2(24.0 + text_w + 10.0, 22.0), Sense::click());
        bg(ui, r, &resp, false, &t);
        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(17.0, 17.0)), icon, t.icon, t.accent);
        ui.painter().text(pos2(r.min.x + 24.0, r.center().y), Align2::LEFT_CENTER, label, regular(11.5), t.text);
        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.max.x - 6.0, r.center().y), vec2(9.0, 9.0)), "dropdown", t.icon, t.accent);
        resp
    };
    let resp = resp.on_hover_text(tip);
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(200.0);
        menu(ui, app);
    });
}

/// A menu item that runs a command and closes the menu. Shows the shortcut; disabled when the
/// command is.
pub fn menu_item(ui: &mut Ui, app: &mut CraftApp, label: &str, id: &str, params: Value) {
    let sc = shortcut_text(app, id);
    let on = enabled(app, id);
    let resp = ui.add_enabled(on, egui::Button::new(label).shortcut_text(sc).min_size(vec2(200.0, 0.0)));
    if resp.clicked() {
        let _ = app.run(id, params);
        ui.close();
    }
}

/// A ribbon group: content, a centred label below and a divider on the right.
pub fn group(ui: &mut Ui, title: &str, app: &mut CraftApp, add: impl FnOnce(&mut Ui, &mut CraftApp)) {
    let t = Tokens::get(ui.ctx());
    let label_w = text_width(ui, title, regular(11.0));
    let start = ui.cursor().min;
    let inner = ui.vertical(|ui| {
        ui.set_min_width(label_w + 10.0);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), CONTENT_H), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
            add(ui, app);
        });
    });
    let r = inner.response.rect;
    let r = Rect::from_min_max(start, pos2(r.max.x, start.y + CONTENT_H + LABEL_H));
    ui.painter().text(pos2(r.center().x, r.max.y - 8.0), Align2::CENTER_CENTER, title, regular(11.0), t.group_label);
    ui.add_space(5.0);
    let x = ui.cursor().min.x;
    ui.painter().line_segment([pos2(x, start.y + 4.0), pos2(x, start.y + CONTENT_H + LABEL_H - 4.0)], Stroke::new(1.0, t.border));
    ui.add_space(6.0);
}

/// A vertical stack of small buttons inside a group.
pub fn stack(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
        add(ui);
    });
}

/// A plain icon button (title bar, status bar). Returns true when clicked.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str, size: f32, enabled: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(size + 8.0, size + 6.0), Sense::click());
    if resp.hovered() && enabled {
        ui.painter().rect_filled(r, 4.0, t.hover);
    }
    let c = if enabled { t.icon } else { t.text_disabled };
    icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(size, size)), icon, c, if enabled { t.accent } else { c });
    resp.on_hover_text(tip).clicked() && enabled
}

/// A filled accent button for the primary action of a dialog.
pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Button::new(egui::RichText::new(label).color(t.on_accent)).fill(t.accent).min_size(vec2(72.0, 24.0)))
}
