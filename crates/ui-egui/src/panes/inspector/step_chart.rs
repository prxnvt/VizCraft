//! The Inspector's Step Chart section: a displacement-step chart (`fluidpower.StepChart`) as a
//! grid of rows by step columns, each cell a toggle that runs one `fluid.setStep`; buttons that
//! add a step at the end (`fluid.insertStep`) or remove the last (`fluid.deleteStep`); and the
//! chart's findings (rows naming no actuator, steps no valve position can drive, idle steps)
//! under the grid, read from the same circuit model as the Problems tab.

use egui::{RichText, Ui};
use serde_json::{Value, json};
use vizcraft_notation::fluid::{self as fl, Circuit, steps as st};

use super::Runs;
use crate::CraftApp;
use crate::theme::Tokens;

/// Whether elements of `type_id` are step charts.
pub(super) fn is_chart(type_id: &str) -> bool {
    fl::is_a(vizcraft_notation::catalog(), type_id, fl::STEP_CHART)
}

/// The chart of element `el`: its rows text read as its first view draws it.
fn chart_of(app: &CraftApp, el: &str) -> st::Chart {
    let doc = &app.session.doc;
    let cat = vizcraft_notation::catalog();
    let Some(e) = doc.model.element(el) else { return st::Chart::default() };
    let text = doc
        .pages
        .iter()
        .find_map(|p| p.shapes.iter().find(|s| s.element.as_deref() == Some(el)).map(|s| fl::rules::chart_text(cat, s, e, fl::page_variant(p))))
        .unwrap_or_else(|| e.prop("rows").and_then(|v| v.as_str()).unwrap_or_default().to_string());
    st::parse(&text)
}

/// The findings about the chart of element `el`.
fn findings(app: &CraftApp, chart: &st::Chart) -> Vec<st::Finding> {
    let doc = &app.session.doc;
    let cat = vizcraft_notation::catalog();
    st::findings(&Circuit::build(doc, cat, None), cat, &doc.model, chart)
}

/// The Step Chart section for chart element `el`.
pub(super) fn section(app: &CraftApp, ui: &mut Ui, t: &Tokens, el: &str, runs: &mut Runs) {
    let chart = chart_of(app, el);
    let n = chart.states();
    if chart.rows.is_empty() {
        ui.label(RichText::new("No rows: write one line per actuator in Rows (`1A1 0 1 0`).").color(t.text_dim));
    } else {
        egui::ScrollArea::horizontal().id_salt(("inspector-steps-scroll", el)).show(ui, |ui| {
            egui::Grid::new(("inspector-steps", el)).num_columns(n + 1).spacing([3.0, 3.0]).striped(true).show(ui, |ui| {
                ui.label(RichText::new("Code").small().color(t.text_dim));
                for i in 0..n {
                    ui.label(RichText::new((i + 1).to_string()).small().color(t.text_dim));
                }
                ui.end_row();
                for (r, row) in chart.rows.iter().enumerate() {
                    ui.label(&row.code);
                    let last = row.states.last().copied().unwrap_or(false);
                    for i in 0..n {
                        let on = row.states.get(i).copied().unwrap_or(last);
                        let resp = ui.add(egui::Button::selectable(on, if on { "1" } else { "0" }).small());
                        let label = format!("{} at step {}", row.code, i + 1);
                        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, label.clone()));
                        if resp.clicked() {
                            runs.push(("fluid.setStep", json!({"chart": el, "row": r, "step": i, "state": !on})));
                        }
                    }
                    ui.end_row();
                }
            });
        });
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(n < st::MAX_STATES, egui::Button::new("+ Step").small()).on_hover_text("Add a step at the end").clicked() {
            runs.push(("fluid.insertStep", json!({"chart": el})));
        }
        if ui.add_enabled(n > st::MIN_STATES, egui::Button::new("- Step").small()).on_hover_text("Remove the last step").clicked() {
            runs.push(("fluid.deleteStep", json!({"chart": el, "at": n.saturating_sub(1)})));
        }
    });
    for f in findings(app, &chart) {
        // Faults read in the normal text colour, idle steps dimmed; the words say which.
        let colour = if f.idle { t.text_dim } else { t.text };
        ui.label(RichText::new(f.message).small().color(colour));
    }
}

/// The `stepChart` field of `ui.inspect`'s inspector report: the selected chart's rows and
/// findings, or null.
pub(super) fn inspect(app: &CraftApp) -> Value {
    let Some(d) = super::members::one_element(app) else { return Value::Null };
    if !is_chart(d["element"]["type"].as_str().unwrap_or_default()) {
        return Value::Null;
    }
    let el = d["element"]["id"].as_str().unwrap_or_default();
    let chart = chart_of(app, el);
    let rows: Vec<Value> = chart.rows.iter().map(|r| json!({"code": r.code, "states": r.states.iter().map(|b| u8::from(*b)).collect::<Vec<_>>()})).collect();
    let found: Vec<Value> = findings(app, &chart).into_iter().map(|f| json!({"message": f.message, "idle": f.idle})).collect();
    json!({"element": el, "rows": rows, "findings": found})
}
