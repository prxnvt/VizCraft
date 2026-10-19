//! The Inspector's Members section: the attribute, operation and literal rows of a UML element,
//! one list per member compartment. Each row has a visibility choice, a text field holding the row
//! without its visibility, and up, down and remove buttons; an Add field closes the list. Every
//! change is one `uml.*Member` command, so one undo step. A multiplicity that does not read is
//! shown in red under its row; it is still stored.

use egui::{RichText, Ui};
use serde_json::{Value, json};
use vizcraft_notation::uml_text::{self, Member, Shown, Visibility};

use super::Runs;
use crate::CraftApp;
use crate::theme::Tokens;

/// The member lists an element type declares: `(property key, heading)` for each `rowsFrom`
/// compartment of the type's own declaration.
pub(super) fn lists_of(type_id: &str) -> Vec<(String, String)> {
    vizcraft_notation::catalog()
        .compartments_in(type_id, "")
        .iter()
        .filter(|d| !d.rows_from.is_empty())
        .map(|d| {
            let heading = d.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| {
                let mut c = d.rows_from.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
            });
            (d.rows_from.clone(), heading)
        })
        .collect()
}

/// The rows of list `key` in an `inspector.get` result.
fn rows_in(d: &Value, key: &str) -> Vec<String> {
    let text = d["properties"].as_array().and_then(|a| a.iter().find(|p| p["key"] == key)).and_then(|p| p["value"].as_str()).unwrap_or_default();
    uml_text::rows(text).map(str::to_string).collect()
}

/// The visibility marks the combo offers; the empty one is none.
const MARKS: [&str; 5] = ["", "+", "-", "#", "~"];

/// A row split for editing: its visibility symbol and the rest of the row.
fn split(row: &str) -> (&'static str, String) {
    let m = uml_text::parse_member(row);
    let mark = m.visibility.map_or("", Visibility::symbol);
    let body = uml_text::format_member(&Member { visibility: None, ..m }, Shown::Hidden);
    (mark, body)
}

/// The row text for a mark and a body: a body that carries its own mark keeps it.
fn join(mark: &str, body: &str) -> String {
    let body = body.trim();
    if body.chars().next().is_some_and(|c| ['+', '-', '#', '~'].contains(&c)) { body.to_string() } else { format!("{mark}{body}") }
}

/// The Members section for the element in `d` (`inspector.get`).
pub(super) fn section(ui: &mut Ui, t: &Tokens, d: &Value, runs: &mut Runs) {
    let Some(el) = d["element"]["id"].as_str() else { return };
    for (key, heading) in lists_of(d["element"]["type"].as_str().unwrap_or_default()) {
        let rows = rows_in(d, &key);
        ui.label(RichText::new(format!("{heading} ({})", rows.len())).strong());
        for (i, row) in rows.iter().enumerate() {
            let (mark, body) = split(row);
            ui.horizontal(|ui| {
                // The pane is narrow: the field takes what the mark and the three buttons leave.
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.spacing_mut().button_padding.x = 4.0;
                let field_w = (ui.available_width() - 48.0 - 3.0 * 22.0 - 12.0).max(50.0);
                let shown = if mark.is_empty() { "-" } else { mark };
                egui::ComboBox::from_id_salt(("member-mark", el, &key, i, row)).selected_text(shown).width(24.0).show_ui(ui, |ui| {
                    for m in MARKS {
                        let label = if m.is_empty() { "(none)" } else { m };
                        if ui.selectable_label(m == mark, label).clicked() && m != mark {
                            runs.push(("uml.setMember", json!({"element": el, "list": key, "index": i, "text": format!("{m}{body}")})));
                        }
                    }
                });
                if let Some(text) = super::text_cell(ui, &format!("{el}|{key}|{i}|{row}"), &body, "", field_w) {
                    runs.push(("uml.setMember", json!({"element": el, "list": key, "index": i, "text": join(mark, &text)})));
                }
                for (glyph, tip, enabled, run) in [
                    ("^", "Move up", i > 0, ("uml.moveMember", json!({"element": el, "list": key, "from": i, "to": i.saturating_sub(1)}))),
                    ("v", "Move down", i + 1 < rows.len(), ("uml.moveMember", json!({"element": el, "list": key, "from": i, "to": i + 1}))),
                    ("x", "Remove member", true, ("uml.removeMember", json!({"element": el, "list": key, "index": i}))),
                ] {
                    let resp = ui.add_enabled(enabled, egui::Button::new(glyph).small()).on_hover_text(tip);
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
                    if resp.clicked() {
                        runs.push(run);
                    }
                }
            });
            let m = uml_text::parse_member(row);
            if !m.multiplicity.is_empty()
                && let Err(why) = uml_text::parse_multiplicity(&m.multiplicity)
            {
                ui.label(RichText::new(format!("Multiplicity [{}]: {why}", m.multiplicity)).small().color(t.red));
            }
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Add").color(t.text_dim));
            let add_w = (ui.available_width() - 12.0).max(60.0);
            if let Some(text) = super::text_cell(ui, &format!("{el}|{key}|add|{}", rows.len()), "", "- name: Type [1]", add_w)
                && !text.trim().is_empty()
            {
                runs.push(("uml.addMember", json!({"element": el, "list": key, "text": text})));
            }
        });
        ui.add_space(4.0);
    }
}

/// The `members` field of `ui.inspect`'s inspector report: the selected element's lists and rows,
/// or null for an element without member lists.
pub(super) fn inspect(app: &CraftApp) -> Value {
    let Some(d) = one_element(app) else { return Value::Null };
    let lists = lists_of(d["element"]["type"].as_str().unwrap_or_default());
    if lists.is_empty() {
        return Value::Null;
    }
    let map: serde_json::Map<String, Value> = lists.iter().map(|(k, _)| (k.clone(), json!(rows_in(&d, k)))).collect();
    json!({"element": d["element"]["id"], "lists": map})
}

/// `inspector.get` of the one selected shape that draws an element.
pub(super) fn one_element(app: &CraftApp) -> Option<Value> {
    let [one] = app.session.sel.items.as_slice() else { return None };
    let d = vizcraft_engine::cmd::inspector::describe(&app.session, one).ok()?;
    d["element"].is_object().then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_splits_into_its_mark_and_body_and_joins_back() {
        let (mark, body) = split("-total: Money [0..1]");
        assert_eq!((mark, body.as_str()), ("-", "total: Money [0..1]"));
        assert_eq!(join(mark, &body), "-total: Money [0..1]");
        assert_eq!(join("+", "+other: int"), "+other: int", "a body with its own mark keeps it");
        assert_eq!(split("plain").0, "");
    }
}
