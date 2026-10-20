//! The displacement-step chart commands: make a chart for the actuators on a page, read it with
//! its findings, and edit one cell or one step column at a time. The chart's rows are its
//! element's `rows` property (see `vizcraft_notation::fluid::steps`); the symbol redraws from it.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use vizcraft_model::PropValue;
use vizcraft_notation::fluid::{self as fl, Circuit, rules as frules, steps as st};

use super::super::model::{NewElement, cat, check_len, create_element, element_of, page_param, place_shape};
use super::super::{num, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return the step chart commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("fluid.stepChart", "Step Chart", "Circuit > Sequence", step_chart).params(
            r#"{"page"?: page id, name or index, "actuators"?: [element or shape id], "steps"?: n (1 to 32, default 4), "x"?: n, "y"?: n} -> {"element", "shape", "rows"}; a displacement-step chart (VDI 3260) with one row per actuator (default: every coded actuator on the page) sorted by code, each extending one step after the one before and retracting in the same order when the steps allow; one undo step"#,
        ),
        CommandSpec::new("fluid.steps", "Step Chart Findings", "Circuit > Sequence", read).params(
            r#"{"chart": element or shape id} -> {"chart", "rows": [{"code", "actuator": element or null, "states": [0|1]}], "steps", "findings": [{"row"?, "step"?, "message", "idle"}]}; rows naming no actuator, steps no valve position can drive, idle steps; changes nothing"#,
        ),
        CommandSpec::new("fluid.setStep", "Set Step", "Circuit > Sequence", set_step).params(
            r#"{"chart": element or shape id, "row": n (from 0), "step": n (state column, from 0), "state": 0|1|bool} -> {"chart", "rows"}; one undo step"#,
        ),
        CommandSpec::new("fluid.insertStep", "Insert Step", "Circuit > Sequence", insert_step).params(
            r#"{"chart": element or shape id, "at"?: n (state column, default the end)} -> {"chart", "rows"}; a column holding the states before it (at 0: after it); at most 32 steps; one undo step"#,
        ),
        CommandSpec::new("fluid.deleteStep", "Delete Step", "Circuit > Sequence", delete_step).params(
            r#"{"chart": element or shape id, "at": n (state column)} -> {"chart", "rows"}; at least one step stays; one undo step"#,
        ),
    ]
}

/// The step chart element a `chart` param names.
fn chart_element(s: &Session, v: &Value) -> Result<String, EngineError> {
    let id = check_len("chart", crate::p::req_str(v, "chart")?)?;
    let e = element_of(&s.doc, id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element or a shape drawing one", short(id))))?;
    let t = s.doc.model.element(&e).map(|x| x.type_id.as_str()).unwrap_or_default();
    if !fl::is_a(cat(), t, fl::STEP_CHART) {
        return Err(EngineError::Params(format!("`{}` is not a displacement-step chart", short(id))));
    }
    Ok(e)
}

/// The chart text of element `e`: from its first view, else its property or the type's default.
pub(crate) fn chart_text(s: &Session, e: &str) -> String {
    let Some(el) = s.doc.model.element(e) else { return String::new() };
    for page in &s.doc.pages {
        if let Some(sh) = page.shapes.iter().find(|sh| sh.element.as_deref() == Some(e)) {
            return frules::chart_text(cat(), sh, el, fl::page_variant(page));
        }
    }
    fl::prop_or_default(cat(), el, "rows").and_then(PropValue::as_str).unwrap_or_default().to_string()
}

/// Store `chart` as element `e`'s rows.
fn store(s: &mut Session, e: &str, chart: &st::Chart) -> CmdResult {
    let text = chart.text();
    if let Some(el) = s.doc.model.element_mut(e) {
        el.properties.insert("rows".into(), PropValue::Text(text.clone()));
    }
    Ok(json!({"chart": e, "rows": text}))
}

/// Rewrite every chart's codes through `renamed` (old code to new), after a renumber.
pub(crate) fn rename_codes(s: &mut Session, renamed: &BTreeMap<String, String>) {
    let charts: Vec<String> = s.doc.model.elements.iter().filter(|e| fl::is_a(cat(), &e.type_id, fl::STEP_CHART)).map(|e| e.id.clone()).collect();
    for id in charts {
        let mut chart = st::parse(&chart_text(s, &id));
        let mut changed = false;
        for row in &mut chart.rows {
            if let Some(to) = renamed.get(&row.code) {
                row.code = to.clone();
                changed = true;
            }
        }
        if changed {
            let _ = store(s, &id, &chart);
        }
    }
}

fn step_chart(s: &mut Session, v: &Value) -> CmdResult {
    let index = page_param(&s.doc, v)?;
    let steps = match num(v, "steps")? {
        None => 4,
        Some(n) if (1.0..=(st::MAX_STATES - 1) as f64).contains(&n) => n as usize,
        Some(_) => return Err(EngineError::Params(format!("`steps` is 1 to {}", st::MAX_STATES - 1))),
    };
    let c = Circuit::build(&s.doc, cat(), Some(index));
    let actuators: Vec<String> = match super::super::ids_param(&json!({"ids": v.get("actuators").cloned().unwrap_or(Value::Null)}))? {
        Some(ids) => {
            let mut out = Vec::new();
            for id in ids.iter().take(st::MAX_ROWS + 1) {
                let e = element_of(&s.doc, check_len("actuators", id)?)
                    .ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(id))))?;
                let t = s.doc.model.element(&e).map(|x| x.type_id.as_str()).unwrap_or_default();
                if !fl::is_a(cat(), t, fl::ACTUATOR) {
                    return Err(EngineError::Params(format!("`{}` is not an actuator", short(id))));
                }
                out.push(e);
            }
            out
        }
        None => c.parts.iter().filter(|p| fl::is_a(cat(), &p.type_id, fl::ACTUATOR)).map(|p| p.element.clone()).collect(),
    };
    let mut codes: Vec<String> = Vec::new();
    for e in &actuators {
        let tag = s.doc.model.element(e).map(|x| x.tag.trim().to_string()).unwrap_or_default();
        if tag.is_empty() {
            if v.get("actuators").is_some_and(|x| !x.is_null()) {
                return Err(EngineError::Params(format!("actuator `{}` has no code: run fluid.renumber first", short(e))));
            }
            continue;
        }
        if !codes.contains(&tag) {
            codes.push(tag);
        }
    }
    if codes.len() > st::MAX_ROWS {
        return Err(EngineError::Params(format!("a chart holds at most {} rows", st::MAX_ROWS)));
    }
    codes.sort();
    let rows = if codes.is_empty() { st::parse("1A1 0 1 0").text() } else { st::default_rows(&codes, steps).text() };
    let page = s.doc.pages.get(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let centre = page.frame().center();
    let x = super::super::shape::coord(num(v, "x")?.unwrap_or(centre.x));
    let y = super::super::shape::coord(num(v, "y")?.unwrap_or(centre.y));
    let mut properties = BTreeMap::new();
    properties.insert("rows".to_string(), PropValue::Text(rows.clone()));
    let (id, _) = create_element(&mut s.doc, NewElement { type_id: fl::STEP_CHART.into(), properties, ..NewElement::default() })?;
    let shape = place_shape(&mut s.doc, &id, index, x, y, None, None)?;
    if index == s.doc.active_index() {
        select(s, vec![shape.clone()]);
    }
    with_sel(s, json!({"element": id, "shape": shape, "rows": rows}))
}

fn read(s: &mut Session, v: &Value) -> CmdResult {
    let e = chart_element(s, v)?;
    let chart = st::parse(&chart_text(s, &e));
    let c = Circuit::build(&s.doc, cat(), None);
    let findings: Vec<Value> = st::findings(&c, cat(), &s.doc.model, &chart)
        .into_iter()
        .map(|f| {
            let mut o = json!({"message": f.message, "idle": f.idle});
            if let Some(r) = f.row {
                o["row"] = json!(r);
            }
            if let Some(st) = f.step {
                o["step"] = json!(st);
            }
            o
        })
        .collect();
    let rows: Vec<Value> = chart
        .rows
        .iter()
        .map(|r| {
            let actuator = c
                .parts
                .iter()
                .find(|p| fl::is_a(cat(), &p.type_id, fl::ACTUATOR) && s.doc.model.element(&p.element).is_some_and(|x| x.tag.trim() == r.code))
                .map(|p| p.element.clone());
            json!({"code": r.code, "actuator": actuator, "states": r.states.iter().map(|b| u8::from(*b)).collect::<Vec<_>>()})
        })
        .collect();
    Ok(json!({"chart": e, "rows": rows, "steps": chart.states().saturating_sub(1), "findings": findings}))
}

/// A whole-number param in `0..len`.
fn index_param(v: &Value, k: &str, len: usize) -> Result<usize, EngineError> {
    let n = v.get(k).and_then(Value::as_f64).filter(|n| n.is_finite() && *n >= 0.0 && n.fract() == 0.0);
    match n {
        Some(n) if (n as usize) < len => Ok(n as usize),
        _ => Err(EngineError::Params(format!("`{k}` must be a whole number from 0 to {}", len.saturating_sub(1)))),
    }
}

fn set_step(s: &mut Session, v: &Value) -> CmdResult {
    let e = chart_element(s, v)?;
    let mut chart = st::parse(&chart_text(s, &e));
    let n = chart.states();
    let row = index_param(v, "row", chart.rows.len())?;
    let step = index_param(v, "step", n)?;
    let state = match v.get("state") {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(x)) if x.as_f64() == Some(0.0) || x.as_f64() == Some(1.0) => x.as_f64() == Some(1.0),
        _ => return Err(EngineError::Params("`state` is 0, 1, true or false".into())),
    };
    if let Some(r) = chart.rows.get_mut(row) {
        let last = r.states.last().copied().unwrap_or(false);
        r.states.resize(n, last);
        if let Some(cell) = r.states.get_mut(step) {
            *cell = state;
        }
    }
    store(s, &e, &chart)
}

fn insert_step(s: &mut Session, v: &Value) -> CmdResult {
    let e = chart_element(s, v)?;
    let mut chart = st::parse(&chart_text(s, &e));
    let n = chart.states();
    if n >= st::MAX_STATES {
        return Err(EngineError::Params(format!("a chart holds at most {} steps", st::MAX_STATES - 1)));
    }
    let at = match v.get("at") {
        None | Some(Value::Null) => n,
        Some(_) => index_param(v, "at", n + 1)?,
    };
    for r in &mut chart.rows {
        let last = r.states.last().copied().unwrap_or(false);
        r.states.resize(n, last);
        let copy = if at == 0 { r.states.first().copied().unwrap_or(false) } else { r.states.get(at - 1).copied().unwrap_or(last) };
        r.states.insert(at.min(r.states.len()), copy);
    }
    store(s, &e, &chart)
}

fn delete_step(s: &mut Session, v: &Value) -> CmdResult {
    let e = chart_element(s, v)?;
    let mut chart = st::parse(&chart_text(s, &e));
    let n = chart.states();
    if n <= st::MIN_STATES {
        return Err(EngineError::Params("a chart keeps at least one step".into()));
    }
    let at = index_param(v, "at", n)?;
    for r in &mut chart.rows {
        let last = r.states.last().copied().unwrap_or(false);
        r.states.resize(n, last);
        if at < r.states.len() {
            r.states.remove(at);
        }
    }
    store(s, &e, &chart)
}
