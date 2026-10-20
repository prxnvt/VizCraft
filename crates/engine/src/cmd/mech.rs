//! Mechanical commands (`mech.*`): mobility, ratio and Mason's gain run the analyses of the
//! same names over the elements a page draws (`analyze::run_kind`, so they make no undo step and
//! fill the Analysis dock); Assign Causality writes every bond's causal stroke in one undo step.
//!
//! [`page_scope`] and [`with_scope`] are shared with `rel.*`.

use serde_json::{Map, Value, json};
use vizcraft_analysis::mech::{self as m, Scope};
use vizcraft_model::PropValue;

use super::analyze::{CatalogTypes, run_kind};
use super::model::{cat, element_of, has_elements, page_param};
use super::{opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the mechanical commands live in the UI.
const LOCATION: &str = "Mechanical > Analyze";

/// The mechanical commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("mech.mobility", "Mobility", LOCATION, mobility)
            .params(r#"{"page"?: page id, name or index (default: the active page), "space"?: "planar"|"spatial"} -> as analyze results with {"n", "j1", "j2", "m", "formula", "grounded"}; the Kutzbach count of the joints and bodies the page draws; no undo step"#)
            .when(has_elements),
        CommandSpec::new("mech.ratio", "Ratio", LOCATION, ratio)
            .params(r#"{"from"?: element or shape id, "to"?: element or shape id (default: the first two selected shapes), "speed"?: number, "page"?} -> {"ratio": "1:6", "value", "sign", "path", "conflicts", "speed"?}; the speed of to per unit speed of from along shafts, meshes, belts and chains; no undo step"#)
            .when(has_elements),
        CommandSpec::new("mech.mason", "Mason's Gain", LOCATION, mason)
            .params(r#"{"input"?: element or shape id, "output"?: element or shape id, "page"?} -> {"formula", "value"?, "paths", "loops", "nonTouching", "delta"}; the transfer function of the block diagram on the page by Mason's gain formula; no undo step"#)
            .when(has_elements),
        CommandSpec::new("mech.causality", "Assign Causality", LOCATION, causality)
            .params(r#"{"page"?} -> {"assigned": number, "conflicts": [element id]}; the sequential causality assignment over the bond graph on the page (sources, then integral causality, then resistors), written to every bond's causal property in one undo step"#)
            .when(has_elements),
    ]
}

/// The element ids page `page` (param, default the active page) draws.
pub fn page_scope(s: &Session, v: &Value) -> Result<Vec<String>, EngineError> {
    let pi = page_param(&s.doc, v)?;
    let page = s.doc.pages.get(pi).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let mut out: Vec<String> = Vec::new();
    for sh in &page.shapes {
        if let Some(e) = &sh.element
            && !out.contains(e)
        {
            out.push(e.clone());
        }
    }
    Ok(out)
}

/// `v` without `page` and with `scope` set to the page's elements, element params (`keys`)
/// resolved from shape ids.
pub fn with_scope(s: &Session, v: &Value, keys: &[&str]) -> Result<Value, EngineError> {
    let mut o: Map<String, Value> = match v {
        Value::Object(o) => o.clone(),
        Value::Null => Map::new(),
        _ => return Err(EngineError::Params("params must be an object".into())),
    };
    let scope = page_scope(s, v)?;
    o.remove("page");
    o.insert("scope".into(), json!(scope));
    for k in keys {
        if let Some(id) = opt_str(v, k)? {
            let id = super::model::check_len(k, id)?;
            o.insert((*k).to_string(), json!(element_of(&s.doc, id).unwrap_or_else(|| id.to_string())));
        }
    }
    Ok(Value::Object(o))
}

fn mobility(s: &mut Session, v: &Value) -> CmdResult {
    let p = with_scope(s, v, &[])?;
    run_kind(s, "mobility", &p)
}

/// The elements of the selected shapes, primary first, each once.
pub fn selected_elements(s: &Session) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in s.sel.primary.iter().chain(s.sel.items.iter()) {
        if let Some(e) = element_of(&s.doc, id)
            && !out.contains(&e)
        {
            out.push(e);
        }
    }
    out
}

fn ratio(s: &mut Session, v: &Value) -> CmdResult {
    let mut p = with_scope(s, v, &["from", "to"])?;
    let sel = selected_elements(s);
    if let Some(o) = p.as_object_mut() {
        for (k, i) in [("from", 0usize), ("to", 1)] {
            if o.get(k).is_none_or(Value::is_null) {
                let id = sel.get(i).ok_or_else(|| EngineError::Params(format!("`{k}` is required; give it, or select two components")))?;
                o.insert(k.into(), json!(id));
            }
        }
    }
    run_kind(s, "ratio", &p)
}

fn mason(s: &mut Session, v: &Value) -> CmdResult {
    let p = with_scope(s, v, &["input", "output"])?;
    run_kind(s, "mason", &p)
}

fn causality(s: &mut Session, v: &Value) -> CmdResult {
    let scope: Scope = Some(page_scope(s, v)?.into_iter().collect());
    let c = {
        let ty = CatalogTypes::new(&s.doc.model, cat());
        m::assign_causality(&s.doc.model, &ty, &scope)
    };
    if c.assigned.is_empty() {
        return Err(EngineError::Params("the page draws no bonds".into()));
    }
    for (id, end) in &c.assigned {
        if let Some(r) = s.doc.model.relationship_mut(id) {
            r.properties.insert("causal".into(), PropValue::Text((*end).to_string()));
        }
    }
    s.status = if c.conflicts.is_empty() {
        format!("Causality assigned to {} bonds", c.assigned.len())
    } else {
        format!("Causality assigned to {} bonds; {} elements conflict", c.assigned.len(), c.conflicts.len())
    };
    let conflicts: Vec<String> = c.conflicts.iter().map(|x| short(x)).collect();
    Ok(json!({"assigned": c.assigned.len(), "conflicts": conflicts}))
}

#[cfg(test)]
#[path = "mech_tests.rs"]
mod tests;
