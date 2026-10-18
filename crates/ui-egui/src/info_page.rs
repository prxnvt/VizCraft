//! The Info page of the File page (Backstage): the document's properties, its statistics, and
//! Inspect for Hidden Data. Properties are edited in place and written with `doc.setProperties`
//! when a field loses focus; Inspect runs `doc.inspectHidden`, and Remove runs `doc.removeHidden`
//! for the kinds ticked, which is one undo step.
//!
//! The backstage owns the navigation: its Info row calls [`set_open`] and its page area calls
//! [`show`] while [`is_open`]. Nothing here keeps document state; the edit buffers and the last
//! inspection live in egui memory.

use egui::{Id, Ui, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::Document;

use crate::CraftApp;
use crate::theme::{Tokens, medium, regular, semibold};

/// The editable properties: `(key, label, multiline)`.
const PROPERTIES: [(&str, &str, bool); 5] = [
    ("title", "Title", false),
    ("subject", "Subject", false),
    ("author", "Author", false),
    ("keywords", "Keywords", false),
    ("description", "Description", true),
];

/// Labels for the kinds of hidden data, in `doc.inspectHidden` order.
const KINDS: [(&str, &str); 7] = [
    ("comments", "Comments"),
    ("hiddenShapes", "Hidden shapes"),
    ("hiddenLayers", "Hidden layers"),
    ("screenTips", "Screen tips"),
    ("dataRows", "Shape data rows"),
    ("images", "Pictures"),
    ("properties", "Document properties"),
];

fn open_id() -> Id {
    Id::new("info_page_open")
}

/// Whether the Info page is the one showing in the File page.
pub fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(open_id())).unwrap_or(false)
}

/// Show or leave the Info page.
pub fn set_open(ctx: &egui::Context, open: bool) {
    ctx.data_mut(|d| d.insert_temp(open_id(), open));
}

/// What the document holds, for the statistics list.
pub fn stats(doc: &Document) -> Vec<(&'static str, usize)> {
    let shapes = || doc.pages.iter().flat_map(|p| &p.shapes);
    vec![
        ("Pages", doc.pages.len()),
        ("Shapes", shapes().count()),
        ("Connectors", doc.pages.iter().map(|p| p.connectors.len()).sum()),
        ("Model elements", doc.model.elements.len()),
        ("Comments", doc.comments.len()),
        ("Pictures", doc.images.len()),
        ("Data rows", shapes().map(|s| s.data.rows.len()).sum()),
    ]
}

/// The last inspection and which kinds are ticked for removal.
#[derive(Clone, Default)]
struct Inspection {
    counts: Option<Vec<(String, u64)>>,
    unticked: Vec<String>,
    message: String,
}

fn inspection_id() -> Id {
    Id::new("info_page_inspection")
}

fn property(app: &CraftApp, key: &str) -> String {
    let p = &app.session.doc.properties;
    match key {
        "title" => p.title.clone(),
        "subject" => p.subject.clone(),
        "author" => p.author.clone(),
        "keywords" => p.keywords.clone(),
        _ => p.description.clone(),
    }
}

/// One property row: a field that writes the property when it loses focus.
fn property_row(app: &mut CraftApp, ui: &mut Ui, key: &'static str, label: &str, multiline: bool) {
    let t = Tokens::get(ui.ctx());
    let id = Id::new(("info_property", key));
    let current = property(app, key);
    let mut buf = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.clone());
    ui.label(egui::RichText::new(label).color(t.text_dim));
    let edit = if multiline { egui::TextEdit::multiline(&mut buf).desired_rows(3) } else { egui::TextEdit::singleline(&mut buf) };
    let r = ui.add(edit.desired_width(380.0).id_salt(id));
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(id, buf.clone()));
    }
    if r.lost_focus() {
        if buf != current {
            let _ = app.run("doc.setProperties", json!({ key: buf }));
        }
        ui.data_mut(|d| d.remove::<String>(id));
    }
    ui.end_row();
}

fn inspect(app: &mut CraftApp, ui: &Ui) {
    let mut st = ui.data(|d| d.get_temp::<Inspection>(inspection_id())).unwrap_or_default();
    match app.run("doc.inspectHidden", json!({})) {
        Ok(r) => {
            st.counts = Some(
                KINDS.iter().map(|(k, _)| (k.to_string(), r.get("counts").and_then(|c| c.get(*k)).and_then(Value::as_u64).unwrap_or(0))).collect(),
            );
            st.message.clear();
        }
        Err(e) => st.message = e,
    }
    ui.data_mut(|d| d.insert_temp(inspection_id(), st));
}

/// Draw the Info page into `ui` (the backstage's page area).
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().id_salt("info_page_scroll").auto_shrink([false, false]).show(ui, |ui| {
        ui.label(egui::RichText::new("Info").font(semibold(24.0)).color(t.text));
        ui.label(egui::RichText::new(app.title_stem()).font(regular(12.0)).color(t.text_dim));
        ui.add_space(18.0);

        ui.label(egui::RichText::new("Properties").font(medium(14.0)).color(t.text));
        ui.add_space(6.0);
        egui::Grid::new("info_properties").num_columns(2).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
            for (key, label, multi) in PROPERTIES {
                property_row(app, ui, key, label, multi);
            }
        });

        ui.add_space(20.0);
        ui.label(egui::RichText::new("Statistics").font(medium(14.0)).color(t.text));
        ui.add_space(6.0);
        egui::Grid::new("info_stats").num_columns(2).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
            for (label, n) in stats(&app.session.doc) {
                ui.label(egui::RichText::new(label).color(t.text_dim));
                ui.label(n.to_string());
                ui.end_row();
            }
        });

        ui.add_space(20.0);
        ui.label(egui::RichText::new("Inspect for Hidden Data").font(medium(14.0)).color(t.text));
        ui.label(
            egui::RichText::new("Comments, hidden shapes and layers, screen tips, data, pictures and properties that the drawing does not show.")
                .font(regular(11.5))
                .color(t.text_dim),
        );
        ui.add_space(6.0);
        let mut st = ui.data(|d| d.get_temp::<Inspection>(inspection_id())).unwrap_or_default();
        if ui.add(egui::Button::new("Inspect Document").min_size(vec2(140.0, 24.0))).clicked() {
            inspect(app, ui);
            st = ui.data(|d| d.get_temp::<Inspection>(inspection_id())).unwrap_or_default();
        }
        if !st.message.is_empty() {
            ui.label(egui::RichText::new(&st.message).color(t.red));
        }
        if let Some(counts) = st.counts.clone() {
            ui.add_space(6.0);
            let mut changed = false;
            egui::Grid::new("info_hidden").num_columns(3).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                for ((key, n), (_, label)) in counts.iter().zip(KINDS) {
                    let mut on = !st.unticked.contains(key) && *n > 0;
                    let r = ui.add_enabled(*n > 0, egui::Checkbox::without_text(&mut on));
                    if r.changed() {
                        changed = true;
                        st.unticked.retain(|k| k != key);
                        if !on {
                            st.unticked.push(key.clone());
                        }
                    }
                    ui.label(label);
                    ui.label(egui::RichText::new(n.to_string()).color(t.text_dim));
                    ui.end_row();
                }
            });
            let chosen: Vec<&str> = counts.iter().filter(|(k, n)| *n > 0 && !st.unticked.contains(k)).map(|(k, _)| k.as_str()).collect();
            ui.add_space(6.0);
            if ui.add_enabled(!chosen.is_empty(), egui::Button::new("Remove All Selected").min_size(vec2(160.0, 24.0))).clicked() {
                match app.run("doc.removeHidden", json!({"what": chosen})) {
                    Ok(_) => st.message.clear(),
                    Err(e) => st.message = e,
                }
                ui.data_mut(|d| d.insert_temp(inspection_id(), st.clone()));
                inspect(app, ui);
                return;
            }
            if changed {
                ui.data_mut(|d| d.insert_temp(inspection_id(), st));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_count_the_sample_document() {
        let doc = vizcraft_engine::sample::sample_document();
        let s = stats(&doc);
        assert_eq!(s.len(), 7);
        assert_eq!(s[0], ("Pages", doc.pages.len()));
        assert_eq!(s[1].1, doc.pages.iter().map(|p| p.shapes.len()).sum::<usize>());
        assert_eq!(s[3].1, doc.model.elements.len());
    }

    #[test]
    fn open_state_is_per_context() {
        let ctx = egui::Context::default();
        assert!(!is_open(&ctx));
        set_open(&ctx, true);
        assert!(is_open(&ctx));
        set_open(&ctx, false);
        assert!(!is_open(&ctx));
    }
}
