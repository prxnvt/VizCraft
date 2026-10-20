//! Reliability commands (`rel.*`): cut sets, availability and outcomes run the analyses of the
//! same names over the elements a page draws (no undo step, results in the Analysis dock);
//! Recompute RPN writes each FMEA row's risk priority number in one undo step.

use serde_json::{Value, json};
use vizcraft_analysis::reliability::types::FAILURE_MODE;
use vizcraft_model::PropValue;
use vizcraft_notation::rel_rules::rpn_of;

use super::analyze::run_kind;
use super::mech::with_scope;
use super::model::{cat, element_of, has_elements};
use super::short;
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the reliability commands live in the UI.
const LOCATION: &str = "Reliability > Analyze";

/// Most ids `rel.rpn` takes.
const MAX_IDS: usize = 10_000;

/// The reliability commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("rel.cutsets", "Cut Sets", LOCATION, cutsets)
            .params(r#"{"top"?: element or shape id, "maxOrder"?: 1..64, "limit"?: 1..10000, "page"?} -> {"sets": [{"events", "order", "probability"}], "rareEvent", "upperBound", "importance", "notes"}; the minimal cut sets of the fault tree on the page; no undo step"#)
            .when(has_elements),
        CommandSpec::new("rel.availability", "Availability", LOCATION, availability)
            .params(r#"{"start"?: element or shape id, "end"?: element or shape id, "time"?: hours, "page"?} -> {"availability", "reliability"?, "blocks", "notes"}; the system availability of the block diagram on the page; no undo step"#)
            .when(has_elements),
        CommandSpec::new("rel.outcomes", "Outcomes", LOCATION, outcomes)
            .params(r#"{"initiator"?: element or shape id, "page"?} -> {"initiator", "frequency", "outcomes": [{"id", "frequency"}], "notes"}; the frequency of each outcome of the event tree on the page; no undo step"#)
            .when(has_elements),
        CommandSpec::new("rel.rpn", "Recompute RPN", "Reliability > FMEA", rpn)
            .params(r#"{"ids"?: [failure mode, worksheet or shape id] (default: every failure mode)} -> {"updated": [id]}; sets each failure mode's rpn to severity times occurrence times detection, in one undo step; a worksheet stands for the rows it owns"#)
            .when(has_elements),
    ]
}

fn cutsets(s: &mut Session, v: &Value) -> CmdResult {
    let p = with_scope(s, v, &["top"])?;
    run_kind(s, "cutsets", &p)
}

fn availability(s: &mut Session, v: &Value) -> CmdResult {
    let p = with_scope(s, v, &["start", "end"])?;
    run_kind(s, "availability", &p)
}

fn outcomes(s: &mut Session, v: &Value) -> CmdResult {
    let p = with_scope(s, v, &["initiator"])?;
    run_kind(s, "outcomes", &p)
}

/// The ids of every failure mode, built with one type index.
fn modes(s: &Session) -> std::collections::HashSet<String> {
    let facts = vizcraft_notation::infer::Facts::new(&s.doc.model, cat());
    s.doc.model.elements.iter().filter(|e| facts.is_a(&e.type_id, FAILURE_MODE)).map(|e| e.id.clone()).collect()
}

fn rpn(s: &mut Session, v: &Value) -> CmdResult {
    let asked: Option<Vec<String>> = match v.get("ids") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) if a.len() <= MAX_IDS => Some(
            a.iter()
                .map(|x| x.as_str().map(str::to_string).ok_or_else(|| EngineError::Params("`ids` must be strings".into())))
                .collect::<Result<_, _>>()?,
        ),
        Some(_) => return Err(EngineError::Params(format!("`ids` must be a list of at most {MAX_IDS} ids"))),
    };
    let modes = modes(s);
    let is_mode = |_: &Session, id: &str| modes.contains(id);
    let mut targets: Vec<String> = Vec::new();
    match asked {
        None => targets.extend(s.doc.model.elements.iter().filter(|e| is_mode(s, &e.id)).map(|e| e.id.clone())),
        Some(ids) => {
            for id in ids {
                let e = element_of(&s.doc, &id).unwrap_or(id.clone());
                if is_mode(s, &e) {
                    targets.push(e);
                } else if s.doc.model.element(&e).is_some() {
                    // A worksheet (or any owner) stands for the rows it owns.
                    let rows: Vec<String> = s
                        .doc
                        .model
                        .elements
                        .iter()
                        .filter(|x| x.owner.as_deref() == Some(e.as_str()) && is_mode(s, &x.id))
                        .map(|x| x.id.clone())
                        .collect();
                    if rows.is_empty() {
                        return Err(EngineError::Params(format!("`{}` is not a failure mode or a worksheet with rows", short(&id))));
                    }
                    targets.extend(rows);
                } else {
                    return Err(EngineError::Params(format!("`{}` is not an element", short(&id))));
                }
            }
        }
    }
    let mut updated = Vec::new();
    for id in &targets {
        let Some(e) = s.doc.model.element(id) else { continue };
        let Some(want) = rpn_of(e) else { continue };
        if e.prop("rpn").and_then(PropValue::as_f64) == Some(want) {
            continue;
        }
        if let Some(e) = s.doc.model.element_mut(id) {
            e.properties.insert("rpn".into(), PropValue::Number(want));
            updated.push(id.clone());
        }
    }
    s.status = format!("RPN recomputed on {} rows", updated.len());
    Ok(json!({"updated": updated}))
}

#[cfg(test)]
#[path = "rel_tests.rs"]
mod tests;
