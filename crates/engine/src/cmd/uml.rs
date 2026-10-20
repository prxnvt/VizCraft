//! OMG UML member commands (`uml.*`): add, set, remove and move the rows of an element's member
//! lists (a class's attributes and operations, an enumeration's literals).
//!
//! A member list is a text property of the element (`attributes`, `operations`, `literals`: one
//! row per line) that the element type's `rowsFrom` compartments draw. These commands edit it a
//! row at a time so the Inspector and agents need not rewrite the whole text. A row is stored in
//! its canonical form, `format_member(parse_member(text), Shown::Symbols)`, which keeps the
//! visibility mark whatever the page variant shows. A multiplicity that does not read is reported
//! in `problems` and never refused: the rules lint, they do not block.

use serde_json::{Value, json};
use vizcraft_diagram::compartment::MAX_ROWS;
use vizcraft_model::PropValue;
use vizcraft_notation::uml_text::{self, MAX_ROW, Shown};

use super::model::{cat, has_elements, req_element, set_prop};
use super::{num, opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The member commands: `uml.addMember`, `uml.setMember`, `uml.removeMember`, `uml.moveMember`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("uml.addMember", "Add Member", "Model > UML", add)
            .params(r#"{"element": element or shape id, "list"?: rowsFrom key (default operations when the text has "(", else attributes), "text": string, "index"?: n (default the end)} -> {"element", "list", "rows": [string], "problems": [{"row", "text", "problem"}]}; the row is stored as format_member(parse_member(text)); a multiplicity that does not read is reported, never refused; one undo step"#)
            .when(has_elements),
        CommandSpec::new("uml.setMember", "Set Member", "Model > UML", set)
            .params(r#"{"element": element or shape id, "list": rowsFrom key, "index": n, "text": string} -> {"element", "list", "rows", "problems"}; replaces row `index` of the list"#)
            .when(has_elements),
        CommandSpec::new("uml.removeMember", "Remove Member", "Model > UML", remove)
            .params(r#"{"element": element or shape id, "list": rowsFrom key, "index": n} -> {"element", "list", "rows", "problems"}"#)
            .when(has_elements),
        CommandSpec::new("uml.moveMember", "Move Member", "Model > UML", move_row)
            .params(r#"{"element": element or shape id, "list": rowsFrom key, "from": n, "to": n} -> {"element", "list", "rows", "problems"}; moves a row to position `to` (clamped to the end)"#)
            .when(has_elements),
    ]
}

/// The element's declared member list keys: the `rowsFrom` of its type's own compartments.
fn lists_of(type_id: &str) -> Vec<String> {
    cat().compartments_in(type_id, "").iter().filter(|d| !d.rows_from.is_empty()).map(|d| d.rows_from.clone()).collect()
}

/// The element named by `element` and its member list keys, or why it has none.
fn element_and_lists(s: &Session, v: &Value) -> Result<(String, Vec<String>), EngineError> {
    let id = req_element(s, v, "element")?;
    let type_id = s.doc.model.element(&id).map(|e| e.type_id.clone()).unwrap_or_default();
    let lists = lists_of(&type_id);
    if lists.is_empty() {
        return Err(EngineError::Params(format!("a {} has no member lists", short(&type_id))));
    }
    Ok((id, lists))
}

/// The `list` param, checked against the element's member lists.
fn list_param(v: &Value, lists: &[String]) -> Result<String, EngineError> {
    let name = opt_str(v, "list")?.ok_or_else(|| EngineError::Params("`list` (a member list such as attributes) is required".into()))?;
    if lists.iter().any(|l| l == name) {
        Ok(name.to_string())
    } else {
        Err(EngineError::Params(format!("`list` must be one of {}, not `{}`", lists.join(", "), short(name))))
    }
}

/// The required `text` param: not empty once trimmed, cut at [`MAX_ROW`] bytes.
fn text_param(v: &Value) -> Result<String, EngineError> {
    let t = opt_str(v, "text")?.ok_or_else(|| EngineError::Params("`text` (the member row) is required".into()))?.trim();
    if t.is_empty() {
        return Err(EngineError::Params("`text` is empty".into()));
    }
    let mut end = t.len().min(MAX_ROW);
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    let row = uml_text::parse_member(t.get(..end).unwrap_or(t));
    Ok(uml_text::format_member(&row, Shown::Symbols).replace(['\n', '\r'], " "))
}

/// A whole-number index param within `0..=max`.
fn index_param(v: &Value, k: &str, max: usize) -> Result<Option<usize>, EngineError> {
    let Some(n) = num(v, k)? else { return Ok(None) };
    if n.fract() != 0.0 || n < 0.0 || n > max as f64 {
        return Err(EngineError::Params(format!("`{k}` must be a whole number from 0 to {max}")));
    }
    Ok(Some(n as usize))
}

/// The rows of list `key` on element `id`.
fn rows_of(s: &Session, id: &str, key: &str) -> Vec<String> {
    let text = s.doc.model.element(id).and_then(|e| e.prop(key)).map(PropValue::display).unwrap_or_default();
    uml_text::rows(&text).map(str::to_string).collect()
}

/// Store `rows` as list `key` of element `id` and report them with what does not read.
fn store(s: &mut Session, id: &str, key: &str, rows: Vec<String>) -> CmdResult {
    let value = if rows.is_empty() { None } else { Some(PropValue::Text(rows.join("\n"))) };
    let e = s.doc.model.element_mut(id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(id))))?;
    set_prop(&mut e.properties, key.to_string(), value)?;
    let problems: Vec<Value> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let m = uml_text::parse_member(r);
            let bad = (!m.multiplicity.is_empty()).then(|| uml_text::parse_multiplicity(&m.multiplicity).err()).flatten()?;
            Some(json!({"row": i, "text": r, "problem": format!("multiplicity `{}`: {bad}", m.multiplicity)}))
        })
        .collect();
    Ok(json!({"element": id, "list": key, "rows": rows, "problems": problems}))
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let (id, lists) = element_and_lists(s, v)?;
    let text = text_param(v)?;
    let key = match opt_str(v, "list")? {
        Some(_) => list_param(v, &lists)?,
        None => {
            let want = if text.contains('(') { "operations" } else { "attributes" };
            lists.iter().find(|l| *l == want).or(lists.first()).cloned().unwrap_or_default()
        }
    };
    let mut rows = rows_of(s, &id, &key);
    if rows.len() >= MAX_ROWS {
        return Err(EngineError::Params(format!("a list holds at most {MAX_ROWS} rows")));
    }
    let at = index_param(v, "index", rows.len())?.unwrap_or(rows.len());
    rows.insert(at, text);
    store(s, &id, &key, rows)
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let (id, lists) = element_and_lists(s, v)?;
    let key = list_param(v, &lists)?;
    let text = text_param(v)?;
    let mut rows = rows_of(s, &id, &key);
    let at = index_param(v, "index", rows.len().saturating_sub(1))?.ok_or_else(|| EngineError::Params("`index` (number) is required".into()))?;
    let slot = rows.get_mut(at).ok_or_else(|| EngineError::Params(format!("the list has no row {at}")))?;
    *slot = text;
    store(s, &id, &key, rows)
}

fn remove(s: &mut Session, v: &Value) -> CmdResult {
    let (id, lists) = element_and_lists(s, v)?;
    let key = list_param(v, &lists)?;
    let mut rows = rows_of(s, &id, &key);
    let at = index_param(v, "index", rows.len().saturating_sub(1))?.ok_or_else(|| EngineError::Params("`index` (number) is required".into()))?;
    if at >= rows.len() {
        return Err(EngineError::Params(format!("the list has no row {at}")));
    }
    rows.remove(at);
    store(s, &id, &key, rows)
}

fn move_row(s: &mut Session, v: &Value) -> CmdResult {
    let (id, lists) = element_and_lists(s, v)?;
    let key = list_param(v, &lists)?;
    let mut rows = rows_of(s, &id, &key);
    let last = rows.len().saturating_sub(1);
    let from = index_param(v, "from", last)?.ok_or_else(|| EngineError::Params("`from` (number) is required".into()))?;
    let to = index_param(v, "to", 1_000_000)?.ok_or_else(|| EngineError::Params("`to` (number) is required".into()))?.min(last);
    if from >= rows.len() {
        return Err(EngineError::Params(format!("the list has no row {from}")));
    }
    let row = rows.remove(from);
    rows.insert(to.min(rows.len()), row);
    store(s, &id, &key, rows)
}

#[cfg(test)]
mod tests;
