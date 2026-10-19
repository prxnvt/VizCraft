//! Entity-relationship commands (`er.*`): add a column to a table, set a relationship's
//! cardinalities, and write the tables drawn on a page as DBML or a Mermaid ER diagram.
//!
//! Columns are `er.Attribute` elements owned by their table (`er.Entity` or `er.WeakEntity`);
//! the sync pass shows them as the table's key and attribute rows, or, on a Chen page, as
//! ellipses around it. A relationship's ends are drawn from its `sourceCardinality` (the parent's
//! end) and `targetCardinality` (the child's) by the end pass.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use vizcraft_model::{MAX_TEXT, PropValue};

use super::model::{NewElement, cat, check_len, create_element, cut, page_param, req_element};
use super::{new_id, opt_bool, opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The column element type.
const ATTRIBUTE: &str = "er.Attribute";

/// The cardinality values the ER pack's relationship types take.
const CARDINALITIES: [&str; 4] = ["one", "zeroOrOne", "oneOrMore", "zeroOrMore"];

/// Most relationships one `er.setCardinality` changes.
const MAX_IDS: usize = 10_000;

/// The ER commands: `er.addColumn`, `er.setCardinality`, `er.write`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("er.addColumn", "Add Column", "Model > Entity Relationship", add_column).params(
            r#"{"entity": table element or shape id, "name": string, "dataType"?: string, "pk"?: bool, "fk"?: bool, "nullable"?: bool, "unique"?: bool, "references"?: "table.column", "id"?: string} -> {"id"}; adds an er.Attribute the table owns, in one undo step; refused for anything that is not an ER table"#,
        ),
        CommandSpec::new("er.setCardinality", "Cardinality", "Model > Entity Relationship", set_cardinality).params(
            r#"{"ids"?: [relationship or connector id] (default: the selected connectors), "source"?: "one"|"zeroOrOne"|"oneOrMore"|"zeroOrMore", "target"?: same} -> {"ids": [relationship id]}; source is the parent's end, target the child's; the ends redraw in the page's notation"#,
        ),
        CommandSpec::new("er.write", "Write Schema", "Model > Entity Relationship", write).params(
            r#"{"format": "dbml"|"mermaid", "page"?: page id, name or index, "path"?: string} -> {"text", "warnings", "path"?}; the tables drawn on the page with the columns they own, their relationships and enums; with path, also saved there"#,
        ),
    ]
}

/// Whether element type `t` is an ER table (or a type that extends one).
fn is_table(t: &str) -> bool {
    let mut at = Some(t.to_string());
    // Supertype chains are short; the bound stops a cycle in a hand-made pack.
    for _ in 0..16 {
        let Some(id) = at else { return false };
        if id == "er.Entity" {
            return true;
        }
        at = cat().element_type(&id).and_then(|(_, x)| x.extends.clone());
    }
    false
}

fn add_column(s: &mut Session, v: &Value) -> CmdResult {
    let entity = req_element(s, v, "entity")?;
    let ty = s.doc.model.element(&entity).map(|e| e.type_id.clone()).unwrap_or_default();
    if !is_table(&ty) {
        return Err(EngineError::Params(format!("`entity`: `{}` is a {}, not an ER table", short(&entity), short(&ty))));
    }
    let name = opt_str(v, "name")?
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| EngineError::Params("`name` (the column name) is required".into()))?;
    let name = check_len("name", name)?.to_string();
    let mut props: BTreeMap<String, PropValue> = BTreeMap::new();
    for key in ["dataType", "references"] {
        if let Some(t) = opt_str(v, key)? {
            props.insert(key.to_string(), PropValue::Text(cut(t.trim(), MAX_TEXT)));
        }
    }
    for key in ["pk", "fk", "nullable", "unique"] {
        if let Some(b) = opt_bool(v, key)? {
            props.insert(key.to_string(), PropValue::Bool(b));
        }
    }
    // A primary key column is never null unless the caller says so.
    if props.get("pk") == Some(&PropValue::Bool(true)) && !props.contains_key("nullable") {
        props.insert("nullable".into(), PropValue::Bool(false));
    }
    if props.contains_key("references") && !props.contains_key("fk") {
        props.insert("fk".into(), PropValue::Bool(true));
    }
    let id = new_id(s, v, "e")?;
    let ne = NewElement { type_id: ATTRIBUTE.into(), name, tag: None, owner: Some(entity), properties: props, id: Some(id) };
    let (id, _) = create_element(&mut s.doc, ne)?;
    s.status = "Column added".into();
    Ok(json!({"id": id}))
}

/// A cardinality param, checked.
fn cardinality(v: &Value, key: &str) -> Result<Option<String>, EngineError> {
    match opt_str(v, key)? {
        None => Ok(None),
        Some(c) if CARDINALITIES.contains(&c) => Ok(Some(c.to_string())),
        Some(c) => Err(EngineError::Params(format!("`{key}`: `{}` is not one of {}", short(c), CARDINALITIES.join(", ")))),
    }
}

fn set_cardinality(s: &mut Session, v: &Value) -> CmdResult {
    let source = cardinality(v, "source")?;
    let target = cardinality(v, "target")?;
    if source.is_none() && target.is_none() {
        return Err(EngineError::Params("give `source`, `target` or both".into()));
    }
    let asked: Vec<String> = match v.get("ids") {
        None | Some(Value::Null) => s.sel.items.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .take(MAX_IDS.saturating_add(1))
            .map(|x| x.as_str().map(str::to_string).ok_or_else(|| EngineError::Params("`ids` must be strings".into())))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(EngineError::Params("`ids` must be a list of relationship or connector ids".into())),
    };
    if asked.len() > MAX_IDS {
        return Err(EngineError::Params(format!("at most {MAX_IDS} ids")));
    }
    let mut ids: Vec<String> = Vec::new();
    for id in &asked {
        let rel = if s.doc.model.relationship(id).is_some() { Some(id.clone()) } else { s.doc.connector(id).and_then(|c| c.relationship.clone()) };
        let Some(rel) =
            rel.filter(|r| s.doc.model.relationship(r).is_some_and(|x| matches!(x.type_id.as_str(), "er.Relationship" | "er.Identifying")))
        else {
            if v.get("ids").is_some_and(|x| !x.is_null()) {
                return Err(EngineError::Params(format!("`{}` is not an ER relationship or a connector of one", short(id))));
            }
            continue;
        };
        if !ids.contains(&rel) {
            ids.push(rel);
        }
    }
    if ids.is_empty() {
        return Err(EngineError::Params("select an ER relationship first (or pass `ids`)".into()));
    }
    for id in &ids {
        let Some(r) = s.doc.model.relationship_mut(id) else { continue };
        for (key, value) in [("sourceCardinality", &source), ("targetCardinality", &target)] {
            if let Some(c) = value {
                r.properties.insert(key.into(), PropValue::Text(c.clone()));
            }
        }
    }
    s.status = format!("Cardinality set on {} relationships", ids.len());
    Ok(json!({"ids": ids}))
}

fn write(s: &mut Session, v: &Value) -> CmdResult {
    let format = opt_str(v, "format")?.ok_or_else(|| EngineError::Params("`format` (\"dbml\" or \"mermaid\") is required".into()))?;
    let page = page_param(&s.doc, v)?;
    let (schema, mut warnings) = vizcraft_formats::er::from_doc(&s.doc, page).map_err(|e| EngineError::Failed(e.to_string()))?;
    let (text, more) = match format {
        "dbml" => vizcraft_formats::dbml::write(&schema),
        "mermaid" => vizcraft_formats::mermaid::er::write(&schema),
        other => return Err(EngineError::Params(format!("`format`: `{}` is not dbml or mermaid", short(other)))),
    };
    warnings.extend(more);
    if schema.tables.is_empty() {
        warnings.push("the page draws no ER table".into());
    }
    match opt_str(v, "path")? {
        Some(path) => {
            let path = check_len("path", path)?;
            std::fs::write(path, &text).map_err(|e| EngineError::Failed(format!("{}: {e}", short(path))))?;
            Ok(json!({"text": text, "warnings": warnings, "path": path}))
        }
        None => Ok(json!({"text": text, "warnings": warnings})),
    }
}

#[cfg(test)]
mod tests;
