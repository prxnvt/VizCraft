//! The Inspector's relationship and key controls: the relationship section for a selected
//! connector (type, name and the properties at each end) and "Include in key".

use egui::{RichText, Ui};
use serde_json::{Value, json};

use super::Runs;
use crate::CraftApp;
use crate::theme::Tokens;

/// The type or kind id the key lists the one selected shape or connector under: an element's type
/// id, or a connector's kind (else its relationship's type). `None` for anything else.
pub(super) fn key_ref(app: &CraftApp) -> Option<String> {
    let doc = &app.session.doc;
    let page = doc.page()?;
    let [one] = app.session.sel.items.as_slice() else { return None };
    if let Some(s) = page.shape(one) {
        return s.element.as_deref().and_then(|e| doc.model.element(e)).map(|e| e.type_id.clone());
    }
    let c = page.connector(one)?;
    if !c.kind.is_empty() {
        return Some(c.kind.clone());
    }
    c.relationship.as_deref().and_then(|r| doc.model.relationship(r)).map(|r| r.type_id.clone())
}

/// "Include in key": shown when the page has a key. Unticking adds the id to the key's `exclude`
/// list and rebuilds the key in place (`key.insert {exclude}`, one undo step); ticking takes it
/// off again. The key never refreshes on its own: this is the one edit that rebuilds it.
pub(super) fn include_in_key(ui: &mut Ui, t: &Tokens, key: &Value, reference: &str, runs: &mut Runs) {
    if key.get("id").is_none_or(Value::is_null) {
        return;
    }
    let exclude: Vec<String> = key["exclude"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let mut on = !exclude.iter().any(|e| e == reference);
    if ui.checkbox(&mut on, "Include in key").changed() {
        let mut next: Vec<String> = exclude.into_iter().filter(|e| e != reference).collect();
        if !on {
            next.push(reference.to_string());
        }
        runs.push(("key.insert", json!({"exclude": next})));
    }
    ui.label(RichText::new("The key is rebuilt now; it never refreshes on its own.").small().color(t.text_dim));
}

/// Whether rule `id` is an `endMarkerMatchesProperty` check (a UML association's drawn ends
/// against its navigability).
fn is_end_marker_rule(id: &str) -> bool {
    vizcraft_notation::catalog()
        .packs()
        .iter()
        .flat_map(|p| p.rules.iter())
        .any(|r| r.id == id && matches!(r.check, vizcraft_notation::schema::Check::EndMarkerMatchesProperty { .. }))
}

/// The repair `Fix ends` runs: the first fix of an `endMarkerMatchesProperty` issue naming this
/// relationship or connector.
fn end_fix(app: &CraftApp, relationship: &str, connector: &str) -> Option<vizcraft_diagram::vizcraft_model::issue::Fix> {
    app.session
        .issues
        .iter()
        .find(|i| is_end_marker_rule(&i.rule) && (i.subjects.iter().any(|s| s == relationship) || i.items.iter().any(|s| s == connector)))
        .and_then(|i| i.fixes.first().cloned())
}

/// The relationship types that may join a connector's two ends, as the picker lists them (legal
/// first, illegal marked), with the current type first when it is not among them.
fn type_choices(app: &CraftApp, connector: &str, current: &str) -> Vec<(String, String, bool)> {
    let doc = &app.session.doc;
    let ends = doc.page().and_then(|p| p.connector(connector)).and_then(|c| Some((c.start.shape()?.to_string(), c.end.shape()?.to_string())));
    let mut out: Vec<(String, String, bool)> = match ends {
        Some((a, b)) => vizcraft_engine::suggest::connect(doc, &a, &b).into_iter().filter(|c| !c.id.is_empty()).map(|c| (c.id, c.label, c.legal)).collect(),
        None => Vec::new(),
    };
    if !out.iter().any(|(id, ..)| id == current) {
        let label = vizcraft_notation::catalog().relationship_type(current).map(|(_, t)| t.label.clone()).unwrap_or_else(|| current.to_string());
        out.insert(0, (current.to_string(), label, true));
    }
    out
}

/// One property editor of a relationship, with the reason a multiplicity does not read in red
/// under it (the value is still written: the rules lint, they do not block).
fn property(ui: &mut Ui, t: &Tokens, rid: &str, p: &Value, runs: &mut Runs) {
    let key = p["key"].as_str().unwrap_or_default();
    ui.label(RichText::new(p["label"].as_str().unwrap_or(key)).color(t.text_dim));
    if let Some(v) = super::prop_editor(ui, t, &format!("{rid}|{key}"), p) {
        runs.push(("relationship.setProperty", json!({"id": rid, "key": key, "value": v})));
    }
    if key.ends_with("Multiplicity")
        && let Some(text) = p["value"].as_str().filter(|s| !s.trim().is_empty())
        && let Err(why) = vizcraft_notation::uml_text::parse_multiplicity(text)
    {
        ui.label(RichText::new(format!("Multiplicity: {why}")).small().color(t.red));
    }
}

/// The Relationship section of a selected connector: its type, name, the properties at each end
/// (headed by the end's element) and the others. `d` is `inspector.get` of the connector.
pub(super) fn section(app: &CraftApp, ui: &mut Ui, t: &Tokens, d: &Value, connector: &str, runs: &mut Runs) {
    let rel = &d["relationship"];
    let Some(rid) = rel["id"].as_str() else {
        ui.label(RichText::new("This connector is not a model relationship.").color(t.text_dim));
        return;
    };
    let current = rel["type"].as_str().unwrap_or_default();
    egui::Grid::new("inspector-relationship").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        super::field(ui, t, "Type", |ui| {
            let label = d["type"]["label"].as_str().filter(|l| !l.is_empty()).unwrap_or(current);
            egui::ComboBox::from_id_salt(("inspector-rel-type", rid)).selected_text(label).width(150.0).show_ui(ui, |ui| {
                for (id, label, legal) in type_choices(app, connector, current) {
                    let shown = if legal { label } else { format!("{label} (not allowed)") };
                    if ui.selectable_label(id == current, shown).clicked() && id != current {
                        runs.push(("relationship.retype", json!({"id": rid, "type": id})));
                    }
                }
            });
        });
        super::field(ui, t, "Name", |ui| {
            if let Some(n) = super::text_cell(ui, &format!("{rid}|name"), rel["name"].as_str().unwrap_or_default(), "", 150.0) {
                runs.push(("connector.setText", json!({"id": connector, "text": n})));
            }
        });
    });
    let props = d["properties"].as_array().map(Vec::as_slice).unwrap_or_default();
    let with_end = |end: Option<&str>| -> Vec<&Value> { props.iter().filter(|p| p["end"].as_str() == end).collect() };
    let (start, end, other) = (with_end(Some("start")), with_end(Some("end")), with_end(None));
    if !start.is_empty() || !end.is_empty() {
        ui.add_space(4.0);
        ui.columns(2, |cols| {
            for (col, (heading, list)) in cols.iter_mut().zip([(rel["sourceName"].as_str(), &start), (rel["targetName"].as_str(), &end)]) {
                let name = heading.filter(|n| !n.is_empty()).unwrap_or("(unnamed)");
                col.label(RichText::new(name).strong());
                egui::Grid::new(("inspector-rel-end", col.id())).num_columns(1).spacing([6.0, 2.0]).show(col, |ui| {
                    for p in list {
                        ui.vertical(|ui| property(ui, t, rid, p, runs));
                        ui.end_row();
                    }
                });
            }
        });
    }
    if !other.is_empty() {
        ui.add_space(4.0);
        egui::Grid::new("inspector-rel-props").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
            for p in other {
                property(ui, t, rid, p, runs);
                ui.end_row();
            }
        });
    }
    if let Some(fix) = end_fix(app, rid, connector)
        && ui.button("Fix ends").on_hover_text(&fix.label).clicked()
    {
        runs.push(("edit.batch", json!({"label": fix.label, "commands": [{"command": fix.command, "params": fix.params}]})));
    }
}

/// The `relationship` field of `ui.inspect`'s inspector report: the selected connector's
/// relationship and the property keys at each end, or null.
pub(super) fn inspect(app: &CraftApp) -> Value {
    let [one] = app.session.sel.items.as_slice() else { return Value::Null };
    let Ok(d) = vizcraft_engine::cmd::inspector::describe(&app.session, one) else { return Value::Null };
    if d["relationship"].is_null() {
        return Value::Null;
    }
    let keys = |end: Option<&str>| -> Vec<Value> {
        d["properties"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter(|p| p["end"].as_str() == end).map(|p| p["key"].clone()).collect()
    };
    let fix = end_fix(app, d["relationship"]["id"].as_str().unwrap_or_default(), one).map(|f| f.label);
    json!({
        "id": d["relationship"]["id"], "type": d["relationship"]["type"], "name": d["relationship"]["name"],
        "sourceName": d["relationship"]["sourceName"], "targetName": d["relationship"]["targetName"],
        "ends": {"start": keys(Some("start")), "end": keys(Some("end")), "other": keys(None)},
        "fixEnds": fix,
    })
}
