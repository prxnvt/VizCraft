//! The Inspector's Sequence group, shown on a page with an interaction. What it edits follows the
//! selection:
//!
//! - a message: its sort (`sequence.setSort`), whether it activates its receiver or deactivates
//!   its sender (`sequence.setActivation`), and its place in the order (read only);
//! - a fragment: its operator, one guard per operand with a remove button (`sequence.setFragment`,
//!   `sequence.removeOperand`), the lifelines it covers (none ticked covers what its content
//!   touches) and Unwrap;
//! - a lifeline: its column and whether it is an actor (read only);
//! - nothing: the page's activation mode (`sequence.setActivations`) and its row height and
//!   column gap (`sequence.layout`, which re-packs the columns).
//!
//! Every choice is the user's; the template's defaults are auto activations, sync messages, a
//! 36 unit row and a 60 unit column gap.

use egui::{RichText, Ui};
use serde_json::{Value, json};
use vizcraft_diagram::sequence::{self as seq, Activations, Event, Operator, Part, Sort};
use vizcraft_diagram::{Page, ShapeKind};

use crate::CraftApp;
use crate::theme::Tokens;

/// Commands the Inspector runs after drawing.
type Runs = Vec<(&'static str, Value)>;

/// What the group is about.
enum Subject<'a> {
    Message(&'a vizcraft_diagram::Connector),
    Fragment(&'a vizcraft_diagram::Shape),
    Lifeline(&'a vizcraft_diagram::Shape),
    Page,
}

fn subject<'a>(app: &'a CraftApp, page: &'a Page) -> Option<Subject<'a>> {
    match app.session.sel.items.as_slice() {
        [] => Some(Subject::Page),
        [one] => {
            if let Some(c) = page.connector(one).filter(|c| c.message.is_some()) {
                return Some(Subject::Message(c));
            }
            let s = page.shape(one)?;
            match s.sequence {
                Some(Part::Fragment { .. }) => Some(Subject::Fragment(s)),
                Some(Part::Lifeline { .. }) => Some(Subject::Lifeline(s)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether the group shows: the page has an interaction and the selection is nothing, or one
/// message, fragment or lifeline.
pub fn shown(app: &CraftApp) -> bool {
    app.session.doc.page().filter(|p| p.sequence.is_some()).and_then(|p| subject(app, p)).is_some()
}

/// Whether the one selected item is a message (the Net group is hidden for it).
pub fn is_message(app: &CraftApp) -> bool {
    let Some(page) = app.session.doc.page() else { return false };
    matches!(app.session.sel.items.as_slice(), [one] if page.connector(one).is_some_and(|c| c.message.is_some()))
}

/// A one-line field that returns its text when Enter or a click away commits a change.
fn field(ui: &mut Ui, salt: &str, current: &str, width: f32) -> Option<String> {
    let id = egui::Id::new(("inspector-sequence-edit", salt));
    let mut buf: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_else(|| current.to_string());
    let resp = ui.add(egui::TextEdit::singleline(&mut buf).id(id.with("field")).desired_width(width));
    let mut out = None;
    if resp.lost_focus() {
        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) && buf != current {
            out = Some(buf);
        }
        ui.ctx().data_mut(|d| d.remove::<String>(id));
    } else if resp.has_focus() {
        ui.ctx().data_mut(|d| d.insert_temp(id, buf));
    } else {
        ui.ctx().data_mut(|d| d.remove::<String>(id));
    }
    out
}

fn row(ui: &mut Ui, t: &Tokens, label: &str, body: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(label).color(t.text_dim));
    body(ui);
    ui.end_row();
}

/// The lifeline columns left to right, as (id, name).
fn columns(page: &Page) -> Vec<(String, String)> {
    let mut cols: Vec<&vizcraft_diagram::Shape> = page.shapes.iter().filter(|s| matches!(s.sequence, Some(Part::Lifeline { .. }))).collect();
    cols.sort_by(|a, b| (a.x + a.w / 2.0).total_cmp(&(b.x + b.w / 2.0)));
    cols.into_iter().map(|s| (s.id.clone(), if s.text.trim().is_empty() { s.id.clone() } else { s.text.clone() })).collect()
}

/// Whether the run of activation events directly after message `id` holds `want`.
fn after_message(events: &[Event], id: &str, want: Option<Event>) -> bool {
    let Some(at) = events.iter().position(|e| matches!(e, Event::Message(m) if m == id)) else { return false };
    let Some(want) = want else { return false };
    events.iter().skip(at + 1).take_while(|e| matches!(e, Event::Activate(_) | Event::Deactivate(_))).any(|e| *e == want)
}

/// Draw the group's body, queueing the commands its edits run.
pub fn show(app: &CraftApp, ui: &mut Ui, t: &Tokens, runs: &mut Runs) {
    let Some(page) = app.session.doc.page() else { return };
    let Some(q) = page.sequence.as_ref() else { return };
    let Some(subject) = subject(app, page) else { return };
    egui::Grid::new("inspector-sequence").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| match subject {
        Subject::Message(c) => {
            let sort = c.message.as_ref().map(|m| m.sort).unwrap_or_default();
            row(ui, t, "Sort", |ui| {
                egui::ComboBox::from_id_salt("inspector-sequence-sort").selected_text(sort.name()).width(120.0).show_ui(ui, |ui| {
                    for s in Sort::ALL {
                        if ui.selectable_label(s == sort, s.name()).clicked() && s != sort {
                            runs.push(("sequence.setSort", json!({"id": c.id, "sort": s.name()})));
                        }
                    }
                });
            });
            let activates = after_message(&q.events, &c.id, c.end.shape().map(|l| Event::Activate(l.into())));
            let deactivates = after_message(&q.events, &c.id, c.start.shape().map(|l| Event::Deactivate(l.into())));
            row(ui, t, "Receiver", |ui| {
                let mut on = activates;
                if ui.checkbox(&mut on, "Activate").changed() {
                    runs.push(("sequence.setActivation", json!({"id": c.id, "activate": on})));
                }
            });
            row(ui, t, "Sender", |ui| {
                let mut on = deactivates;
                if ui.checkbox(&mut on, "Deactivate").changed() {
                    runs.push(("sequence.setActivation", json!({"id": c.id, "deactivate": on})));
                }
            });
            let order = seq::block_of(&q.events, &c.id).map_or("-".to_string(), |r| (r.start + 1).to_string());
            row(ui, t, "Order", |ui| {
                ui.label(order);
            });
        }
        Subject::Fragment(f) => {
            let Some(Part::Fragment { operator, operands, covers }) = &f.sequence else { return };
            row(ui, t, "Operator", |ui| {
                egui::ComboBox::from_id_salt("inspector-sequence-operator").selected_text(operator.name()).width(120.0).show_ui(ui, |ui| {
                    for o in Operator::ALL {
                        if ui.selectable_label(o == *operator, o.name()).clicked() && o != *operator {
                            runs.push(("sequence.setFragment", json!({"id": f.id, "operator": o.name()})));
                        }
                    }
                });
            });
            let guards: Vec<String> = operands.iter().map(|o| o.guard.clone()).collect();
            for (k, g) in guards.iter().enumerate() {
                row(ui, t, &format!("Guard {}", k + 1), |ui| {
                    ui.horizontal(|ui| {
                        if let Some(v) = field(ui, &format!("{}-{k}", f.id), g, 120.0) {
                            let mut all = guards.clone();
                            if let Some(slot) = all.get_mut(k) {
                                *slot = v.trim().to_string();
                            }
                            runs.push(("sequence.setFragment", json!({"id": f.id, "guards": all})));
                        }
                        if guards.len() > 1 && ui.small_button("Remove").on_hover_text("Its messages join the operand before it").clicked() {
                            runs.push(("sequence.removeOperand", json!({"id": f.id, "operand": k})));
                        }
                    });
                });
            }
            let cols = columns(page);
            row(ui, t, "Covers", |ui| {
                ui.vertical(|ui| {
                    if covers.is_empty() {
                        ui.label(RichText::new("Auto: the lifelines its content touches").color(t.text_dim));
                    }
                    for (id, name) in &cols {
                        let mut on = covers.contains(id);
                        if ui.checkbox(&mut on, name).changed() {
                            let next: Vec<String> = cols.iter().map(|(i, _)| i.clone()).filter(|i| covers.contains(i) != (i == id)).collect();
                            runs.push(("sequence.setFragment", json!({"id": f.id, "covers": next})));
                        }
                    }
                });
            });
            row(ui, t, "", |ui| {
                if ui.button("Unwrap").on_hover_text("Remove the fragment and keep its content").clicked() {
                    runs.push(("sequence.unwrap", json!({"id": f.id})));
                }
            });
        }
        Subject::Lifeline(s) => {
            let col = columns(page).iter().position(|(id, _)| *id == s.id).map_or("-".to_string(), |k| (k + 1).to_string());
            row(ui, t, "Column", |ui| {
                ui.label(col);
            });
            row(ui, t, "Actor", |ui| {
                ui.label(if s.kind == ShapeKind::ActorLifeline { "yes" } else { "no" });
            });
        }
        Subject::Page => {
            row(ui, t, "Activations", |ui| {
                ui.horizontal(|ui| {
                    for (m, label) in [(Activations::Auto, "Auto"), (Activations::Explicit, "Explicit")] {
                        if ui.selectable_label(q.activations == m, label).clicked() && q.activations != m {
                            runs.push(("sequence.setActivations", json!({"mode": m.name()})));
                        }
                    }
                });
            });
            let num = |v: f64| if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v}") };
            row(ui, t, "Row", |ui| {
                if let Some(v) = field(ui, "row", &num(q.row), 60.0).and_then(|v| v.trim().parse::<f64>().ok()) {
                    runs.push(("sequence.layout", json!({"row": v.clamp(16.0, 400.0)})));
                }
            });
            row(ui, t, "Column gap", |ui| {
                ui.horizontal(|ui| {
                    if let Some(v) = field(ui, "gap", &num(q.column_gap), 60.0).and_then(|v| v.trim().parse::<f64>().ok()) {
                        runs.push(("sequence.layout", json!({"columnGap": v.clamp(0.0, 2000.0)})));
                    }
                    ui.label(RichText::new("re-packs the columns").color(t.text_dim));
                });
            });
        }
    });
}

/// What `ui.inspect` reports under `inspector.sequence`: the page's activation mode, row and
/// column gap, and what the group shows for the selection (`null` off a Sequence page).
pub fn inspect(app: &CraftApp) -> Value {
    let Some(page) = app.session.doc.page() else { return Value::Null };
    let Some(q) = page.sequence.as_ref() else { return Value::Null };
    let selected = match subject(app, page) {
        Some(Subject::Message(c)) => json!({
            "kind": "message",
            "id": c.id,
            "sort": c.message.as_ref().map(|m| m.sort.name()),
            "order": seq::block_of(&q.events, &c.id).map(|r| r.start),
            "activate": after_message(&q.events, &c.id, c.end.shape().map(|l| Event::Activate(l.into()))),
            "deactivate": after_message(&q.events, &c.id, c.start.shape().map(|l| Event::Deactivate(l.into()))),
        }),
        Some(Subject::Fragment(f)) => match &f.sequence {
            Some(Part::Fragment { operator, operands, covers }) => json!({
                "kind": "fragment", "id": f.id, "operator": operator.name(),
                "guards": operands.iter().map(|o| o.guard.clone()).collect::<Vec<_>>(), "covers": covers,
            }),
            _ => Value::Null,
        },
        Some(Subject::Lifeline(s)) => json!({
            "kind": "lifeline", "id": s.id, "column": columns(page).iter().position(|(id, _)| *id == s.id), "actor": s.kind == ShapeKind::ActorLifeline,
        }),
        Some(Subject::Page) | None => Value::Null,
    };
    json!({"activations": q.activations.name(), "row": q.row, "columnGap": q.column_gap, "selected": selected})
}
