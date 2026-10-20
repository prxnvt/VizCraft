//! The Inspector's Electrical section for elements of the electrical pack:
//!
//! - a device's designation parts (tag, class code, prefix and the project's format), read only:
//!   the sync pass writes them from the tag format and the device's place;
//! - a coil's (or overload's) contacts, each with its form, where it is and Go to;
//! - a contact's coil with Go to, or a choice of coils to link it to (`electrical.link`);
//! - a terminal strip's terminals as a grid (Number, Type, Bridge, Internal and External wire)
//!   with Add Terminal and Renumber, a cable's cores (Number, Colour, Wire) and an I/O module's
//!   points (Channel, Address, Device, Signal).
//!
//! Every edit is one command (`element.setProperty`, `terminal.add`, `terminal.renumber`,
//! `electrical.link`); every row offers Go to. The section reads the model directly, which costs
//! no command run per frame.

use egui::{RichText, Ui};
use serde_json::{Value, json};
use vizcraft_diagram::vizcraft_model::{Element, PropValue};
use vizcraft_engine::doc::Document;

use super::{Runs, text_cell};
use crate::CraftApp;
use crate::theme::Tokens;

/// Whether element `el` gets the section: a device with a designation, or a coil, contact,
/// terminal strip, cable or I/O module.
pub(super) fn applies(app: &CraftApp, el: &str) -> bool {
    app.session.doc.model.element(el).is_some_and(|e| {
        e.type_id.starts_with("electrical.")
            && (e.prop("code").is_some()
                || matches!(
                    e.type_id.as_str(),
                    "electrical.Coil" | "electrical.Overload" | "electrical.Contact" | "electrical.TerminalStrip" | "electrical.Cable" | "electrical.IoModule"
                ))
    })
}

fn prop(e: &Element, k: &str) -> String {
    e.prop(k).map(PropValue::display).unwrap_or_default()
}

/// What a row names: the tag, else the name, else the id.
fn label(e: &Element) -> String {
    [e.tag.as_str(), e.name.as_str()].into_iter().find(|t| !t.trim().is_empty()).unwrap_or(e.id.as_str()).to_string()
}

/// The first view of element `id`: page id and shape id.
fn view_of(doc: &Document, id: &str) -> Option<(String, String)> {
    doc.pages.iter().find_map(|p| p.shapes.iter().find(|s| s.element.as_deref() == Some(id)).map(|s| (p.id.clone(), s.id.clone())))
}

/// A Go to button for element `id`: shows its page and selects its view. An element drawn on
/// no page says so instead.
fn go_to(app: &CraftApp, ui: &mut Ui, id: &str, what: &str, runs: &mut Runs) {
    let Some((page, shape)) = view_of(&app.session.doc, id) else {
        ui.label(egui::RichText::new("not drawn").small()).on_hover_text(format!("{what} is not drawn on any page"));
        return;
    };
    let b = ui.add(egui::Button::new("Go to").small()).on_hover_text(format!("Show {what} and select it"));
    let name = format!("Go to {what}");
    b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name.clone()));
    if b.clicked() {
        if app.session.doc.page().map(|p| p.id.as_str()) != Some(page.as_str()) {
            runs.push(("page.setActive", json!({"page": page})));
        }
        runs.push(("edit.select", json!({"ids": [shape]})));
    }
}

/// The Electrical section for element `el`.
pub(super) fn section(app: &CraftApp, ui: &mut Ui, t: &Tokens, el: &str, runs: &mut Runs) {
    let doc = &app.session.doc;
    let Some(e) = doc.model.element(el) else { return };
    if e.prop("iecCode").is_some() || e.prop("code").is_some() {
        designation(app, ui, t, e);
    }
    match e.type_id.as_str() {
        "electrical.Coil" | "electrical.Overload" => contacts(app, ui, t, e, runs),
        "electrical.Contact" => coil_of(app, ui, t, e, runs),
        "electrical.TerminalStrip" => terminals(app, ui, t, e, runs),
        "electrical.Cable" => cores(app, ui, t, e, runs),
        "electrical.IoModule" => points(app, ui, t, e, runs),
        _ => {}
    }
}

fn designation(app: &CraftApp, ui: &mut Ui, t: &Tokens, e: &Element) {
    let format = match vizcraft_notation::electrical_pass::tag_format(&app.session.doc) {
        "jic" => "JIC",
        "y1444" => "ASME Y14.44",
        _ => "IEC 81346",
    };
    egui::Grid::new(("electrical-designation", &e.id)).num_columns(2).spacing([8.0, 3.0]).show(ui, |ui| {
        for (k, v) in [("Designation", e.tag.clone()), ("Class code", prop(e, "code")), ("Prefix", prop(e, "prefix")), ("Format", format.to_string())] {
            ui.label(RichText::new(k).color(t.text_dim));
            ui.label(if v.is_empty() { "-".to_string() } else { v });
            ui.end_row();
        }
    });
}

fn contacts(app: &CraftApp, ui: &mut Ui, t: &Tokens, coil: &Element, runs: &mut Runs) {
    let doc = &app.session.doc;
    let list: Vec<&Element> = doc
        .model
        .relationships
        .iter()
        .filter(|r| r.type_id == vizcraft_notation::electrical_pass::CONTACT_OF && r.target == coil.id)
        .filter_map(|r| doc.model.element(&r.source))
        .collect();
    ui.label(RichText::new(format!("Contacts ({})", list.len())).strong());
    if list.is_empty() {
        ui.label(RichText::new("No contacts carry this tag yet. Give a contact the same tag, or select contacts and the coil and click Link to Coil.").color(t.text_dim));
        return;
    }
    egui::Grid::new(("electrical-contacts", &coil.id)).num_columns(4).spacing([8.0, 3.0]).striped(true).show(ui, |ui| {
        for h in ["Contact", "Form", "Where", ""] {
            ui.label(RichText::new(h).small().color(t.text_dim));
        }
        ui.end_row();
        for c in list {
            let at = vizcraft_notation::sheet_pass::location(doc, &c.id).map(|l| format!("{}.{}", l.sheet, l.zone)).unwrap_or_else(|| "-".into());
            let form = match prop(c, "form").as_str() {
                "nc" => "break",
                "co" => "changeover",
                _ => "make",
            };
            ui.label(label(c));
            ui.label(form);
            ui.label(at);
            go_to(app, ui, &c.id, &format!("contact {}", label(c)), runs);
            ui.end_row();
        }
    });
}

fn coil_of(app: &CraftApp, ui: &mut Ui, t: &Tokens, contact: &Element, runs: &mut Runs) {
    let doc = &app.session.doc;
    let coil = doc
        .model
        .relationships
        .iter()
        .find(|r| r.type_id == vizcraft_notation::electrical_pass::CONTACT_OF && r.source == contact.id)
        .and_then(|r| doc.model.element(&r.target));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Coil").color(t.text_dim));
        match coil {
            Some(c) => {
                ui.label(label(c));
                go_to(app, ui, &c.id, &format!("coil {}", label(c)), runs);
            }
            None => {
                let coils: Vec<&Element> =
                    doc.model.elements.iter().filter(|e| matches!(e.type_id.as_str(), "electrical.Coil" | "electrical.Overload") && !e.tag.is_empty()).collect();
                egui::ComboBox::from_id_salt(("electrical-link", &contact.id)).selected_text("Link to Coil").show_ui(ui, |ui| {
                    for c in coils {
                        if ui.selectable_label(false, label(c)).clicked() {
                            runs.push(("electrical.link", json!({"contacts": [contact.id], "coil": c.id})));
                        }
                    }
                });
            }
        }
    });
}

/// A text cell writing property `key` of element `id` on commit.
fn prop_cell(ui: &mut Ui, id: &str, key: &'static str, current: &str, width: f32, runs: &mut Runs) {
    if let Some(v) = text_cell(ui, &format!("electrical-{key}-{id}"), current, "", width) {
        let value = if v.trim().is_empty() { Value::Null } else { json!(v.trim()) };
        runs.push(("element.setProperty", json!({"id": id, "key": key, "value": value})));
    }
}

fn owned<'a>(doc: &'a Document, owner: &str, ty: &str) -> Vec<&'a Element> {
    doc.model.elements.iter().filter(|e| e.type_id == ty && e.owner.as_deref() == Some(owner)).collect()
}

fn terminals(app: &CraftApp, ui: &mut Ui, t: &Tokens, strip: &Element, runs: &mut Runs) {
    let doc = &app.session.doc;
    let mut list = owned(doc, &strip.id, "electrical.Terminal");
    list.sort_by_key(|e| prop(e, "number").parse::<i64>().unwrap_or(i64::MAX));
    ui.label(RichText::new(format!("Terminals ({})", list.len())).strong());
    egui::ScrollArea::horizontal().id_salt(("electrical-terminals-scroll", &strip.id)).show(ui, |ui| {
        egui::Grid::new(("electrical-terminals", &strip.id)).num_columns(6).spacing([6.0, 3.0]).striped(true).show(ui, |ui| {
            for h in ["No.", "Type", "Bridge", "Internal", "External", ""] {
                ui.label(RichText::new(h).small().color(t.text_dim));
            }
            ui.end_row();
            for e in &list {
                prop_cell(ui, &e.id, "number", &prop(e, "number"), 32.0, runs);
                ui.label(prop(e, "type"));
                prop_cell(ui, &e.id, "bridge", &prop(e, "bridge"), 40.0, runs);
                ui.label(prop(e, "netIn"));
                ui.label(prop(e, "netOut"));
                go_to(app, ui, &e.id, &format!("terminal {}", label(e)), runs);
                ui.end_row();
            }
        });
    });
    ui.horizontal(|ui| {
        if ui.button("Add Terminal").clicked() {
            runs.push(("terminal.add", json!({"strip": strip.id})));
        }
        if ui.button("Renumber").on_hover_text("Number the terminals from 1 in their drawn order").clicked() {
            runs.push(("terminal.renumber", json!({"strip": strip.id})));
        }
    });
}

fn cores(app: &CraftApp, ui: &mut Ui, t: &Tokens, cable: &Element, runs: &mut Runs) {
    // Cores are listed, not drawn, so their rows have no Go to.
    let doc = &app.session.doc;
    let mut list = owned(doc, &cable.id, "electrical.Core");
    list.sort_by_key(|e| prop(e, "number").parse::<i64>().unwrap_or(i64::MAX));
    ui.label(RichText::new(format!("Cores ({})", list.len())).strong());
    egui::Grid::new(("electrical-cores", &cable.id)).num_columns(3).spacing([6.0, 3.0]).striped(true).show(ui, |ui| {
        for h in ["No.", "Colour", "Wire"] {
            ui.label(RichText::new(h).small().color(t.text_dim));
        }
        ui.end_row();
        for e in &list {
            ui.label(prop(e, "number"));
            prop_cell(ui, &e.id, "colour", &prop(e, "colour"), 50.0, runs);
            ui.label(prop(e, "net"));
            ui.end_row();
        }
    });
}

fn points(app: &CraftApp, ui: &mut Ui, t: &Tokens, module: &Element, runs: &mut Runs) {
    let doc = &app.session.doc;
    let mut list = owned(doc, &module.id, "electrical.IoPoint");
    list.sort_by_key(|e| e.prop("channel").and_then(PropValue::as_f64).map_or(i64::MAX, |c| c as i64));
    ui.label(RichText::new(format!("I/O points ({})", list.len())).strong());
    if list.is_empty() {
        ui.label(RichText::new("Select a device with the module and click Assign I/O to give it a channel.").color(t.text_dim));
        return;
    }
    egui::Grid::new(("electrical-points", &module.id)).num_columns(4).spacing([6.0, 3.0]).striped(true).show(ui, |ui| {
        for h in ["Ch.", "Address", "Device", "Signal"] {
            ui.label(RichText::new(h).small().color(t.text_dim));
        }
        ui.end_row();
        for e in &list {
            ui.label(prop(e, "channel"));
            ui.label(prop(e, "address"));
            prop_cell(ui, &e.id, "device", &prop(e, "device"), 56.0, runs);
            ui.label(prop(e, "signal"));
            ui.end_row();
        }
    });
}
