//! The File page (Backstage): a full-window page with New, Open, Save, Save As, Export, Print,
//! Import, Close, Preferences and About in a coloured column, the recent files and the
//! document's facts beside it. Opened by the File tab (`ui.tab {"tab": "File"}` or
//! `ui.backstage`); Escape or the back arrow returns to the document. Every row runs a command;
//! nothing here has behaviour of its own. New opens a page of its own: a blank drawing, or a
//! notation (the packs grouped by audience) whose first viewpoint the new document starts on.

use egui::{Align2, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::CraftApp;
use crate::theme::{Tokens, medium, regular, semibold};

/// Width of the command column on the left.
const NAV_W: f32 = 200.0;

/// Draw the File page over the whole window.
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        set_new_page(ui.ctx(), false);
        let _ = app.run("ui.backstage", json!({"value": false}));
    }
    egui::Panel::left("backstage_nav")
        .exact_size(NAV_W)
        .frame(egui::Frame::NONE.fill(t.accent).inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: if app.integrated_titlebar { 40 } else { 16 },
            bottom: 12,
        }))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
            if back_button(ui) {
                set_new_page(ui.ctx(), false);
                let _ = app.run("ui.backstage", json!({"value": false}));
            }
            ui.add_space(14.0);
            for (label, id) in [
                ("New", "file.new"),
                ("Open", "file.open"),
                ("Save", "file.save"),
                ("Save As", "file.saveAs"),
                ("Export", "file.export"),
                ("Print", "ui.print"),
                ("Import", "file.import"),
                ("Close", "file.close"),
            ] {
                if nav_item(ui, label, crate::widgets::enabled(app, id)) {
                    if id == "file.new" {
                        set_new_page(ui.ctx(), true);
                        continue;
                    }
                    set_new_page(ui.ctx(), false);
                    let _ = app.run(id, json!({}));
                    if id != "file.close" {
                        let _ = app.run("ui.backstage", json!({"value": false}));
                    }
                }
            }
            ui.add_space(14.0);
            if nav_item(ui, "Preferences", true) {
                let _ = app.run("app.preferences", json!({}));
            }
            if nav_item(ui, "About", true) {
                let _ = app.run("help.about", json!({}));
            }
        });
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(36, 28))).show(ui, |ui| {
        if new_page(ui.ctx()) {
            new_from_notation(app, ui);
            return;
        }
        ui.label(egui::RichText::new(app.title_stem()).font(semibold(24.0)).color(t.text));
        let where_ = app.session.path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "Not saved yet".into());
        ui.label(egui::RichText::new(where_).font(regular(12.0)).color(t.text_dim));
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Recent").font(medium(14.0)).color(t.text));
            if !app.ui.recent.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_width(ui.available_width().min(520.0));
                    if ui.add(egui::Button::new(egui::RichText::new("Clear Recent").font(regular(11.5)).color(t.accent_text)).frame(false)).clicked()
                    {
                        let _ = app.run("ui.clearRecent", json!({}));
                    }
                });
            }
        });
        ui.add_space(6.0);
        if app.ui.recent.is_empty() {
            ui.label(egui::RichText::new("Files you open or save appear here.").color(t.text_dim));
        }
        let recent = app.ui.recent.clone();
        for path in recent {
            let name = std::path::Path::new(&path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width().min(520.0), 40.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 4.0, t.hover);
            }
            ui.painter().text(pos2(r.min.x + 10.0, r.min.y + 12.0), Align2::LEFT_CENTER, &name, medium(12.5), t.text);
            ui.painter().text(pos2(r.min.x + 10.0, r.min.y + 28.0), Align2::LEFT_CENTER, &path, regular(11.0), t.text_dim);
            if resp.clicked() {
                let _ = app.run("file.open", json!({"path": path}));
                let _ = app.run("ui.backstage", json!({"value": false}));
            }
        }
        ui.add_space(24.0);
        ui.label(egui::RichText::new("Document").font(medium(14.0)).color(t.text));
        ui.add_space(6.0);
        let core = app.session.doc.core.clone();
        egui::Grid::new("backstage_facts").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
            for (k, v) in [
                ("Title", app.session.doc.title.clone()),
                ("Author", core.author),
                ("Last saved", core.modified),
                ("Saved by", core.last_modified_by),
                ("Saves", core.revision.to_string()),
            ] {
                if v.is_empty() {
                    continue;
                }
                ui.label(egui::RichText::new(k).color(t.text_dim));
                ui.label(v);
                ui.end_row();
            }
        });
    });
}

/// Whether the New page is showing instead of the document's facts.
fn new_page(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("backstage_new"))).unwrap_or(false)
}

fn set_new_page(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("backstage_new"), on));
}

/// The packs a new document can start from, grouped by the first audience each names (in the order
/// the audiences first appear): a pack with a viewpoint is a notation to start from.
fn notations() -> Vec<(String, Vec<&'static vizcraft_notation::Pack>)> {
    let mut groups: Vec<(String, Vec<&'static vizcraft_notation::Pack>)> = Vec::new();
    for p in vizcraft_notation::catalog().packs().iter().filter(|p| !p.viewpoints.is_empty()) {
        let audience = p.audience.first().cloned().unwrap_or_else(|| "general".into());
        match groups.iter_mut().find(|(a, _)| *a == audience) {
            Some((_, v)) => v.push(p),
            None => groups.push((audience, vec![p])),
        }
    }
    groups
}

/// A word with its first letter capitalised.
fn title_case(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// The New page: a blank drawing, then each notation as a card with its name and description.
/// Choosing one runs `file.new`, then `model.setViewpoint` with the pack's first viewpoint, so the
/// palette and the notation tab follow.
fn new_from_notation(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new("New").font(semibold(24.0)).color(t.text));
    ui.add_space(14.0);
    let mut chosen: Option<Option<String>> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        if card(ui, "Blank drawing", "An empty page with the basic shapes.") {
            chosen = Some(None);
        }
        for (audience, packs) in notations() {
            ui.add_space(14.0);
            ui.label(egui::RichText::new(title_case(&audience)).font(medium(14.0)).color(t.text));
            ui.add_space(4.0);
            for p in packs {
                if card(ui, &p.name, &p.description) {
                    chosen = Some(p.viewpoints.first().map(|v| v.id.clone()));
                }
            }
        }
    });
    if let Some(viewpoint) = chosen {
        if app.run("file.new", json!({})).is_ok() {
            if let Some(v) = viewpoint {
                let _ = app.run("model.setViewpoint", json!({"viewpoint": v}));
            }
            set_new_page(ui.ctx(), false);
            let _ = app.run("ui.backstage", json!({"value": false}));
        }
    }
}

/// One clickable card with a name and a line under it. Returns true when clicked.
fn card(ui: &mut Ui, name: &str, text: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width().min(520.0), 46.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    ui.painter().rect(r, 4.0, if resp.hovered() { t.hover } else { t.panel }, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    ui.painter().text(pos2(r.min.x + 12.0, r.min.y + 15.0), Align2::LEFT_CENTER, name, medium(13.0), t.text);
    let clip = ui.painter().with_clip_rect(r.shrink2(vec2(10.0, 0.0)));
    clip.text(pos2(r.min.x + 12.0, r.min.y + 32.0), Align2::LEFT_CENTER, text, regular(11.0), t.text_dim);
    resp.clicked()
}

/// The back arrow at the top of the command column (drawn, not a glyph). Returns true when clicked.
fn back_button(ui: &mut Ui) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 15.0, t.on_accent.gamma_multiply(0.18));
    }
    let c = r.center();
    let s = Stroke::new(1.8, t.on_accent);
    ui.painter().line_segment([c + vec2(-7.0, 0.0), c + vec2(7.0, 0.0)], s);
    ui.painter().line_segment([c + vec2(-7.0, 0.0), c + vec2(-2.0, -5.0)], s);
    ui.painter().line_segment([c + vec2(-7.0, 0.0), c + vec2(-2.0, 5.0)], s);
    resp.on_hover_text("Back to the document (Escape)").clicked()
}

/// A row in the coloured command column. Returns true when clicked.
fn nav_item(ui: &mut Ui, label: &str, enabled: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if resp.hovered() && enabled {
        ui.painter().rect_stroke(r, 4.0, Stroke::new(1.0, t.on_accent), egui::StrokeKind::Inside);
    }
    let color = if enabled { t.on_accent } else { t.on_accent.gamma_multiply(0.5) };
    ui.painter().text(r.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, label, medium(13.0), color);
    resp.clicked() && enabled
}
