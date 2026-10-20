//! The Inspector's Process section, for the process pack's items:
//!
//! - a pipe segment: its Line (the line number, read-only because it is composed, then Size,
//!   Fluid, Class and Insulation, which the number is built from, and where the number is
//!   written), with New Line to start a new line here;
//! - an instrument or a valve: its Loop (the loop's number and variable, the functional letters)
//!   with New Loop, Take Out of Loop and Trace Loop;
//! - a piece of equipment: its Nozzles as a grid (tag, size, Go to) with Add Nozzle by side.
//!
//! Every edit writes the model property the text is built from (`element.setProperty` on the
//! line or the element), so the tags and the line number recompose; reads come straight from the
//! model, never from a command run per frame.

use egui::{RichText, Ui};
use serde_json::{Value, json};
use vizcraft_diagram::vizcraft_model::{Element, PropValue};
use vizcraft_notation::process_pass::{EXT_LINE, is_line_type};

use super::{Runs, field, text_cell};
use crate::CraftApp;
use crate::theme::Tokens;

/// What the section shows for the selection.
pub(super) enum What {
    /// A pipe segment and its line element.
    Line { connector: String, line: String },
    /// An element of the process pack that sits in loops or owns nozzles.
    Element(String),
}

fn text(e: &Element, k: &str) -> String {
    e.prop(k).map(PropValue::display).unwrap_or_default()
}

fn is_a(type_id: &str, ancestor: &str) -> bool {
    let cat = vizcraft_notation::catalog();
    let mut at = Some(type_id.to_string());
    for _ in 0..16 {
        let Some(t) = at else { return false };
        if t == ancestor {
            return true;
        }
        at = cat.element_type(&t).and_then(|(_, et)| et.extends.clone());
    }
    false
}

fn loopable(type_id: &str) -> bool {
    is_a(type_id, "process.ProcessInstrumentationFunction") || is_a(type_id, "process.PipingComponent") || is_a(type_id, "process.ProcessSignalGeneratingFunction")
}

/// What the section shows for the one selected item, if it is a process item.
pub(super) fn shown(app: &CraftApp) -> Option<What> {
    let doc = &app.session.doc;
    let page = doc.page()?;
    let [one] = app.session.sel.items.as_slice() else { return None };
    if let Some(c) = page.connector(one) {
        let line = c.ext.get(EXT_LINE).filter(|l| doc.model.element(l).is_some_and(|e| is_line_type(&e.type_id)))?;
        return Some(What::Line { connector: one.clone(), line: line.clone() });
    }
    let e = page.shape(one)?.element.as_deref().and_then(|e| doc.model.element(e))?;
    (loopable(&e.type_id) || is_a(&e.type_id, "process.Equipment")).then(|| What::Element(e.id.clone()))
}

/// The section's title for what it shows.
pub(super) fn title(w: &What, app: &CraftApp) -> &'static str {
    match w {
        What::Line { .. } => "Line",
        What::Element(e) if app.session.doc.model.element(e).is_some_and(|x| is_a(&x.type_id, "process.Equipment")) => "Nozzles",
        What::Element(_) => "Loop",
    }
}

/// The section body.
pub(super) fn section(app: &CraftApp, ui: &mut Ui, t: &Tokens, w: &What, runs: &mut Runs) {
    match w {
        What::Line { connector, line } => line_section(app, ui, t, connector, line, runs),
        What::Element(e) => {
            let Some(el) = app.session.doc.model.element(e) else { return };
            if is_a(&el.type_id, "process.Equipment") {
                nozzles(app, ui, t, el, runs);
            } else {
                loop_section(app, ui, t, el, runs);
            }
        }
    }
}

/// The editable keys of a line: (key, label).
const LINE_KEYS: [(&str, &str); 4] = [("nominalDiameter", "Size"), ("fluidCode", "Fluid"), ("pipingClass", "Class"), ("insulation", "Insulation")];
/// The editable keys of a duct run.
const DUCT_KEYS: [(&str, &str); 3] = [("service", "System"), ("size", "Size"), ("airflow", "Airflow")];

fn line_section(app: &CraftApp, ui: &mut Ui, t: &Tokens, connector: &str, line: &str, runs: &mut Runs) {
    let Some(l) = app.session.doc.model.element(line) else { return };
    let duct = l.type_id == vizcraft_notation::process_pass::DUCT;
    egui::Grid::new("inspector-process-line").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        field(ui, t, if duct { "Duct number" } else { "Line number" }, |ui| {
            let shown = if l.tag.is_empty() { "-".to_string() } else { l.tag.clone() };
            ui.label(RichText::new(shown).strong()).on_hover_text("Composed from the fields below; it changes when they do");
        });
        let keys: &[(&str, &str)] = if duct { &DUCT_KEYS } else { &LINE_KEYS };
        for (k, label) in keys {
            field(ui, t, label, |ui| {
                if let Some(v) = text_cell(ui, &format!("process-line-{line}-{k}"), &text(l, k), "", 140.0) {
                    let value = if v.trim().is_empty() { Value::Null } else { json!(v.trim()) };
                    runs.push(("element.setProperty", json!({"id": line, "key": k, "value": value})));
                }
            });
        }
        field(ui, t, "Number shown", |ui| {
            let now = l.prop("label").and_then(PropValue::as_str).unwrap_or("once").to_string();
            egui::ComboBox::from_id_salt(("process-line-label", line)).selected_text(label_name(&now)).show_ui(ui, |ui| {
                for v in ["once", "each", "off"] {
                    if ui.selectable_label(now == v, label_name(v)).clicked() && now != v {
                        runs.push(("element.setProperty", json!({"id": line, "key": "label", "value": v})));
                    }
                }
            });
        });
    });
    if ui.button("New Line").on_hover_text("Start a new line from this segment (a branch)").clicked() {
        runs.push(("line.assign", json!({"connectors": [connector], "line": "new"})));
    }
}

fn label_name(v: &str) -> &'static str {
    match v {
        "each" => "On every segment",
        "off" => "Nowhere",
        _ => "Once per sheet",
    }
}

/// The loop that owns `e`, through its owners.
fn loop_of<'a>(app: &'a CraftApp, e: &Element) -> Option<&'a Element> {
    let m = &app.session.doc.model;
    m.ancestors(&e.id).into_iter().filter_map(|a| m.element(&a)).find(|a| a.type_id == "process.InstrumentationLoopFunction")
}

fn loop_section(app: &CraftApp, ui: &mut Ui, t: &Tokens, e: &Element, runs: &mut Runs) {
    let lp = loop_of(app, e);
    egui::Grid::new("inspector-process-loop").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        field(ui, t, "Letters", |ui| {
            if let Some(v) = text_cell(ui, &format!("process-letters-{}", e.id), &text(e, "letters"), "LIC", 100.0) {
                let value = if v.trim().is_empty() { Value::Null } else { json!(v.trim().to_ascii_uppercase()) };
                runs.push(("element.setProperty", json!({"id": e.id, "key": "letters", "value": value})));
            }
        });
        field(ui, t, "Loop", |ui| match lp {
            Some(l) => {
                ui.label(RichText::new(format!("{} ({})", l.tag, text(l, "variable"))).strong());
            }
            None => {
                ui.label(RichText::new("In no loop").color(t.text_dim));
            }
        });
        if let Some(l) = lp {
            field(ui, t, "Loop type", |ui| {
                ui.label(text(l, "loopType"));
            });
        }
    });
    ui.horizontal(|ui| match lp {
        None => {
            if ui.button("New Loop").on_hover_text("A loop numbered for this item's first letter").clicked() {
                runs.push(("loop.create", json!({"members": [e.id]})));
            }
        }
        Some(_) => {
            if ui.button("Take Out of Loop").clicked() {
                runs.push(("loop.assign", json!({"loop": null, "elements": [e.id]})));
            }
            if ui.button("Trace Loop").on_hover_text("Select the loop's chain and say whether it is complete").clicked() {
                runs.push(("ui.loopTrace", json!({})));
            }
        }
    });
}

fn nozzles(app: &CraftApp, ui: &mut Ui, t: &Tokens, e: &Element, runs: &mut Runs) {
    let doc = &app.session.doc;
    let list: Vec<&Element> = doc.model.elements.iter().filter(|n| n.type_id == "process.Nozzle" && n.owner.as_deref() == Some(e.id.as_str())).collect();
    if list.is_empty() {
        ui.label(RichText::new("No nozzles yet").color(t.text_dim));
    } else {
        egui::Grid::new("inspector-process-nozzles").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new("Nozzle").color(t.text_dim));
            ui.label(RichText::new("Size").color(t.text_dim));
            ui.label("");
            ui.end_row();
            for n in list {
                ui.label(if n.tag.is_empty() { text(n, "subTag") } else { n.tag.clone() });
                if let Some(v) = text_cell(ui, &format!("process-nozzle-{}", n.id), &text(n, "nominalDiameter"), "2in", 70.0) {
                    let value = if v.trim().is_empty() { Value::Null } else { json!(v.trim()) };
                    runs.push(("element.setProperty", json!({"id": n.id, "key": "nominalDiameter", "value": value})));
                }
                let views: Vec<String> = vizcraft_diagram::views::views_of(doc, &n.id).into_iter().flat_map(|v| v.items).collect();
                if ui.small_button("Go to").on_hover_text("Select the nozzle").clicked() && !views.is_empty() {
                    runs.push(("edit.select", json!({"ids": views})));
                }
                ui.end_row();
            }
        });
    }
    ui.menu_button("Add Nozzle", |ui| {
        for (side, label) in [("left", "Left"), ("right", "Right"), ("top", "Top"), ("bottom", "Bottom")] {
            if ui.button(label).clicked() {
                runs.push(("plant.addNozzle", json!({"side": side})));
                ui.close();
            }
        }
    });
}

/// `ui.loopTrace`: trace the loop of the selection (`loop.trace`), select its chain on this page
/// and say in the status bar whether it is complete. A selection in no loop gives a hint.
pub fn ui_command(app: &mut CraftApp, id: &str, _p: &Value) -> Option<Result<Value, String>> {
    if id != "ui.loopTrace" {
        return None;
    }
    // A read that cannot apply leaves a hint, never an error, in the status bar.
    let r = match app.session.run("loop.trace", &json!({})) {
        Ok(r) => r,
        Err(_) => {
            app.status("Select an instrument or a valve in a loop to trace it");
            return Some(Ok(json!({"traced": false})));
        }
    };
    let shapes = r.get("shapes").cloned().unwrap_or(json!([]));
    if shapes.as_array().is_some_and(|a| !a.is_empty()) {
        let _ = app.run("edit.select", json!({"ids": shapes}));
    }
    let tag = r.get("tag").and_then(Value::as_str).unwrap_or_default();
    let missing: Vec<&str> = r.get("missing").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let msg = if r.get("complete").and_then(Value::as_bool).unwrap_or(false) {
        format!("Loop {tag} is complete")
    } else {
        format!("Loop {tag} is missing: {}", missing.join(", "))
    };
    app.status(msg);
    Some(Ok(r))
}
