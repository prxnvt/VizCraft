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
