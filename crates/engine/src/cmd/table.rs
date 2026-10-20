//! Tables over the model: lists of elements or relationships with their properties and matrices of
//! the relationships between two sets of elements (a RACI reads as a matrix of one text property),
//! edited in place and exported as CSV, XLSX or Markdown.
//!
//! The reads are `vizcraft_model::query`; this module resolves types (a type covers its subtypes)
//! and declared properties through the notation catalog, and turns the edits into model changes:
//! a cell edit is `table.set`, a matrix click is `table.toggle`.

use serde_json::{Value, json};
use vizcraft_diagram::{Connector, Document, End, Route};
use vizcraft_model::query::{self, Matrix, Selector, Subject};
use vizcraft_model::{MAX_TAG, MAX_TEXT, PropValue, Relationship};
use vizcraft_notation::infer::Facts;
use vizcraft_notation::schema::{PropKind, PropertySchema};

use super::model::{cat, check_key, cut, element_of, fill_defaults, id_param, model_err, prop_value, rename_element, rename_relationship, set_prop};
use super::{MAX_IDS, delete_on_page, opt_str, prune_selection, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Most columns a list names.
const MAX_COLUMNS: usize = 256;

/// Return the table commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("table.list", "Element List", "Model > Tables", list)
            .params(
                r#"{"type"?: element or relationship type, "pack"?: pack id, "columns"?: [string], "filter"?: string, "subject"?: "nets"|"connections", "kind"?: net kind} -> {"subject": "elements"|"relationships"|"nets"|"connections", "columns": [string], "ids": [id], "rows": [[string]], "count", "truncated"}; one row per element (id, type when several, tag, name, then the declared properties) or per relationship (id, type, name, source, target, then the properties); subject nets lists one row per net (number, name, kind, pins, segments, colour, crossSection, cable, core) and connections one per wire segment (wire, from, to, colour, crossSection, page; a junction end names a pin of the same net), the net or connector ids in ids, both filtered by net kind; a type covers its subtypes; columns names fixed columns or property keys; filter keeps rows with a cell containing the text; at most 10000 rows"#,
            )
            .when(super::model::has_items),
        CommandSpec::new("table.matrix", "Relationship Matrix", "Model > Tables", matrix)
            .params(
                r#"{"rows": element type, "cols": element type, "relationship"?: relationship type, "property"?: string} -> {"rows": [{"id", "label"}], "cols": [{"id", "label"}], "cells": [{"row": index, "col": index, "relationships": [id], "text"}], "count", "truncated"}; a cell lists the relationships from the row's element to the column's (`x`, or the values of `property` across them, which is how a RACI reads); at most 10000 rows and 256 columns"#,
            )
            .when(super::model::has_elements),
        CommandSpec::new("table.set", "Set Table Cell", "Model > Tables", set)
            .params(
                r#"{"id": element, relationship or net id, "key": "name"|"tag"|"number" (a net)|property key, "value": string|number|bool|null} -> {"id", "key", "value"}; one edit of the model as a table cell shows it: name renames (and updates every view), tag retags, any other key sets that property (null or empty text removes it; text is read as a number or flag when the property is one)"#,
            )
            .when(super::model::has_items),
        CommandSpec::new("table.toggle", "Toggle Matrix Cell", "Model > Tables", toggle)
            .params(
                r#"{"row": element id, "col": element id, "relationship": relationship type} -> {"linked": bool, "relationships": [id], "connector"?: id}; creates a relationship of that type from the row's element to the column's (drawn where both are on one page), or deletes every such relationship when there is one"#,
            )
            .when(super::model::has_elements),
        CommandSpec::new("table.export", "Export Table", "Model > Tables", export)
            .params(
                r#"{"view": "list"|"matrix"|"pack", ..the view's params (pack: "list": "process.<name>", "page"?), "format": "csv"|"xlsx"|"markdown", "path"?: string} -> {"format", "columns", "rows", "path"?, "bytes"?, "content"?}; writes the file at path, or returns csv and markdown as content when there is no path (xlsx needs a path)"#,
            )
            .when(super::model::has_items),
    ]
}

/// A table-shaped view of the model, ready to show or write.
struct View {
    /// The sheet name an export uses.
    name: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    truncated: bool,
}

/// Every distinct type id the model uses, in model order.
fn types_in_use(doc: &Document, subject: Subject) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let all: Vec<&str> = match subject {
        Subject::Elements => doc.model.elements.iter().map(|e| e.type_id.as_str()).collect(),
        Subject::Relationships => doc.model.relationships.iter().map(|r| r.type_id.as_str()).collect(),
    };
    for t in all {
        if !out.iter().any(|x| x == t) {
            out.push(t.to_string());
        }
    }
    out
}

/// The selector for an element `type` (with its subtypes in the model) or `pack`.
fn element_selector(doc: &Document, type_id: Option<&str>, pack: Option<&str>) -> Result<Selector, EngineError> {
    match (type_id, pack) {
        (Some(_), Some(_)) => Err(EngineError::Params("give `type` or `pack`, not both".into())),
        (Some(t), None) => {
            if cat().element_type(t).is_none() && !doc.model.elements.iter().any(|e| e.type_id == t) {
                return Err(EngineError::Params(format!("unknown element type `{}` (see notation.describe)", short(t))));
            }
            let facts = Facts::new(&doc.model, cat());
            let mut types: Vec<String> = types_in_use(doc, Subject::Elements).into_iter().filter(|u| facts.is_a(u, t)).collect();
            if types.is_empty() {
                types.push(t.to_string());
            }
            Ok(Selector::Types(types))
        }
        (None, Some(pk)) => {
            if cat().pack(pk).is_none() && !doc.model.uses_pack(pk) {
                return Err(EngineError::Params(format!("unknown pack `{}`", short(pk))));
            }
            Ok(Selector::Pack(pk.to_string()))
        }
        (None, None) => Ok(Selector::All),
    }
}

/// The declared property keys for a list: those of `type_id` (and its supertypes), or the union
/// over the types in use that the selector covers.
fn declared(doc: &Document, subject: Subject, sel: &Selector, type_id: Option<&str>) -> Vec<String> {
    let facts = Facts::new(&doc.model, cat());
    let mut out: Vec<String> = Vec::new();
    let mut add = |ps: Vec<&PropertySchema>| {
        for p in ps {
            if !out.contains(&p.key) {
                out.push(p.key.clone());
            }
        }
    };
    match subject {
        Subject::Elements => match (sel, type_id) {
            (_, Some(t)) => add(facts.properties(t)),
            (Selector::Pack(_), _) => {
                for t in types_in_use(doc, subject).iter().filter(|t| sel.matches(t)) {
                    add(facts.properties(t));
                }
            }
            _ => {}
        },
        Subject::Relationships => {
            if let Some((_, rt)) = type_id.and_then(|t| cat().relationship_type(t)) {
                add(rt.properties.iter().collect());
            }
        }
    }
    out
}

fn columns_param(v: &Value) -> Result<Vec<String>, EngineError> {
    match v.get("columns") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => {
            if a.len() > MAX_COLUMNS {
                return Err(EngineError::Params(format!("`columns` has more than {MAX_COLUMNS} entries")));
            }
            a.iter()
                .map(|x| match x.as_str() {
                    Some(k) => check_key(k).map(|()| k.to_string()),
                    None => Err(EngineError::Params("`columns` must be strings".into())),
                })
                .collect()
        }
        Some(_) => Err(EngineError::Params("`columns` must be an array of strings".into())),
    }
}

/// The list the params ask for, and the subject it is about.
fn build_list(s: &Session, v: &Value) -> Result<(Subject, query::Table, Option<String>), EngineError> {
    let (type_id, pack) = (opt_str(v, "type")?, opt_str(v, "pack")?);
    let doc = &s.doc;
    let relationships = type_id.is_some_and(|t| {
        cat().element_type(t).is_none() && (cat().relationship_type(t).is_some() || doc.model.relationships.iter().any(|r| r.type_id == t))
    });
    let (subject, sel) = if relationships {
        let t = type_id.unwrap_or_default();
        if pack.is_some() {
            return Err(EngineError::Params("give `type` or `pack`, not both".into()));
        }
        (Subject::Relationships, Selector::Types(vec![t.to_string()]))
    } else {
        (Subject::Elements, element_selector(doc, type_id, pack)?)
    };
    let mut props = declared(doc, subject, &sel, type_id);
    for k in query::keys_in_use(&doc.model, subject, &sel) {
        if !props.contains(&k) {
            props.push(k);
        }
    }
    let filter = opt_str(v, "filter")?.unwrap_or_default();
    let t = query::list(&doc.model, subject, &sel, &columns_param(v)?, &props, filter);
    Ok((subject, t, type_id.map(str::to_string)))
}

fn list(s: &mut Session, v: &Value) -> CmdResult {
    if let Some(subject) = net_subject(v)? {
        return super::table_nets::list(s, subject, v);
    }
    let (subject, t, _) = build_list(s, v)?;
    Ok(json!({
        "subject": if subject == Subject::Elements { "elements" } else { "relationships" },
        "columns": t.columns, "ids": t.ids, "count": t.rows.len(), "rows": t.rows, "truncated": t.truncated,
    }))
}

/// The matrix the params ask for.
fn build_matrix(s: &Session, v: &Value) -> Result<Matrix, EngineError> {
    let rows = p::req_str(v, "rows")?;
    let cols = p::req_str(v, "cols")?;
    let rs = element_selector(&s.doc, Some(rows), None)?;
    let cs = element_selector(&s.doc, Some(cols), None)?;
    let relationship = opt_str(v, "relationship")?;
    if let Some(r) = relationship
        && cat().relationship_type(r).is_none()
        && !s.doc.model.relationships.iter().any(|x| x.type_id == r)
    {
        return Err(EngineError::Params(format!("unknown relationship type `{}` (see notation.describe)", short(r))));
    }
    let property = opt_str(v, "property")?;
    if let Some(k) = property {
        check_key(k)?;
    }
    Ok(query::matrix(&s.doc.model, &rs, &cs, relationship, property))
}

fn matrix(s: &mut Session, v: &Value) -> CmdResult {
    let m = build_matrix(s, v)?;
    let axis = |a: &[query::Axis]| a.iter().map(|x| json!({"id": x.id, "label": x.label})).collect::<Vec<_>>();
    let cells: Vec<Value> =
        m.cells.iter().map(|((r, c), cell)| json!({"row": r, "col": c, "relationships": cell.relationships, "text": cell.text})).collect();
    Ok(json!({"rows": axis(&m.rows), "cols": axis(&m.cols), "count": cells.len(), "cells": cells, "truncated": m.truncated}))
}

/// The id of the element or relationship `id` names (or a shape or connector that draws one).
fn target_of(doc: &Document, id: &str) -> Result<(String, bool), EngineError> {
    if let Some(e) = element_of(doc, id) {
        return Ok((e, true));
    }
    let r = doc.model.relationship(id).map(|r| r.id.clone()).or_else(|| doc.connector(id).and_then(|c| c.relationship.clone()));
    r.filter(|r| doc.model.relationship(r).is_some())
        .map(|r| (r, false))
        .ok_or_else(|| EngineError::Params(format!("`id`: `{}` is not an element or a relationship", short(id))))
}

/// A cell's value as a property value: the JSON value as is, or text read as the property's kind.
fn property_value(x: &Value, key: &str, schema: Option<&PropertySchema>) -> Result<Option<PropValue>, EngineError> {
    match x {
        Value::Null => Ok(None),
        Value::String(t) if t.trim().is_empty() => Ok(None),
        Value::String(t) => {
            let t = cut(t, MAX_TEXT);
            let bad = |what: &str| EngineError::Params(format!("`{}` takes {what}, not `{}`", short(key), short(&t)));
            Ok(Some(match schema.map(|p| p.kind) {
                Some(PropKind::Number) => PropValue::Number(t.trim().parse::<f64>().ok().filter(|f| f.is_finite()).ok_or_else(|| bad("a number"))?),
                Some(PropKind::Integer) => {
                    let f = t.trim().parse::<f64>().ok().filter(|f| f.is_finite() && f.fract() == 0.0).ok_or_else(|| bad("a whole number"))?;
                    PropValue::Number(f)
                }
                Some(PropKind::Bool) => match t.trim().to_lowercase().as_str() {
                    "true" | "yes" => PropValue::Bool(true),
                    "false" | "no" => PropValue::Bool(false),
                    _ => return Err(bad("true or false")),
                },
                _ => PropValue::Text(t),
            }))
        }
        other => prop_value(other, key).map(Some),
    }
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let raw = id_param(v, "id")?;
    // A wire list row is a wire segment: its cells edit the segment's net.
    let wire_net = s.doc.pages.iter().find_map(|pg| pg.connector(raw)).filter(|c| c.relationship.is_none()).and_then(|c| c.net.clone());
    if let Some(net) = wire_net.or_else(|| s.doc.model.net(raw).map(|n| n.id.clone())) {
        let key = match p::req_str(v, "key")? {
            "wire" => "number",
            k => k,
        }
        .to_string();
        let value = v.get("value").ok_or_else(|| EngineError::Params("`value` is required (null clears the cell)".into()))?.clone();
        return super::table_nets::set(s, &net, &key, &value);
    }
    let (id, is_element) = target_of(&s.doc, raw)?;
    let key = p::req_str(v, "key")?.to_string();
    let value = v.get("value").ok_or_else(|| EngineError::Params("`value` is required (null clears the cell)".into()))?;
    let text = |value: &Value| match value {
        Value::Null => Ok(String::new()),
        Value::String(t) => Ok(t.clone()),
        _ => Err(EngineError::Params(format!("`value` must be text for `{}`", short(&key)))),
    };
    let shown = match key.as_str() {
        "name" => {
            let name = text(value)?;
            if is_element {
                rename_element(&mut s.doc, &id, &name)?
            } else {
                rename_relationship(&mut s.doc, &id, &name)?
            }
            cut(&name, MAX_TEXT)
        }
        "tag" if is_element => {
            let tag = text(value)?;
            if tag.len() > MAX_TAG {
                return Err(EngineError::Params(format!("`tag` is longer than {MAX_TAG} bytes")));
            }
            let e = s.doc.model.element_mut(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(&id))))?;
            e.tag = tag.clone();
            tag
        }
        "id" | "type" | "source" | "target" | "tag" => {
            return Err(EngineError::Params(format!("`{key}` is not edited in a table; use the model commands")));
        }
        _ => {
            check_key(&key)?;
            let type_id = if is_element {
                s.doc.model.element(&id).map(|e| e.type_id.clone())
            } else {
                s.doc.model.relationship(&id).map(|r| r.type_id.clone())
            };
            let schema: Option<PropertySchema> = type_id.as_deref().and_then(|t| {
                if is_element {
                    Facts::new(&s.doc.model, cat()).properties(t).into_iter().find(|p| p.key == key).cloned()
                } else {
                    cat().relationship_type(t).and_then(|(_, rt)| rt.properties.iter().find(|p| p.key == key).cloned())
                }
            });
            let pv = property_value(value, &key, schema.as_ref())?;
            let shown = pv.as_ref().map(PropValue::display).unwrap_or_default();
            if is_element {
                let e = s.doc.model.element_mut(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(&id))))?;
                set_prop(&mut e.properties, key.clone(), pv)?;
            } else {
                let r = s.doc.model.relationship_mut(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not a relationship", short(&id))))?;
                set_prop(&mut r.properties, key.clone(), pv)?;
            }
            shown
        }
    };
    Ok(json!({"id": id, "key": key, "value": shown}))
}

/// The shape on page `page` that draws `element`.
fn shape_on(doc: &Document, page: usize, element: &str) -> Option<String> {
    doc.pages.get(page)?.shapes.iter().find(|sh| sh.element.as_deref() == Some(element)).map(|sh| sh.id.clone())
}

fn toggle(s: &mut Session, v: &Value) -> CmdResult {
    let row = element_of(&s.doc, id_param(v, "row")?).ok_or_else(|| EngineError::Params("`row` is not an element".into()))?;
    let col = element_of(&s.doc, id_param(v, "col")?).ok_or_else(|| EngineError::Params("`col` is not an element".into()))?;
    let type_id = p::req_str(v, "relationship")?.to_string();
    let rt = cat()
        .relationship_type(&type_id)
        .map(|(_, rt)| rt)
        .ok_or_else(|| EngineError::Params(format!("unknown relationship type `{}`", short(&type_id))))?;
    let existing: Vec<String> = s
        .doc
        .model
        .relationships
        .iter()
        .filter(|r| r.type_id == type_id && r.source == row && r.target == col)
        .map(|r| r.id.clone())
        .take(MAX_IDS)
        .collect();
    if !existing.is_empty() {
        let removed = s.doc.model.remove(&existing);
        let gone: Vec<String> = removed.elements.iter().chain(removed.relationships.iter()).cloned().collect();
        let doomed = vizcraft_diagram::views::items_referencing(&s.doc, &gone);
        if !doomed.is_empty() {
            for page in &mut s.doc.pages {
                delete_on_page(page, &doomed);
            }
        }
        prune_selection(s);
        return Ok(json!({"linked": false, "relationships": removed.relationships}));
    }
    let mut r = Relationship::new(s.doc.next_id("r"), type_id.clone(), row.clone(), col.clone());
    fill_defaults(&mut r.properties, &rt.properties);
    let end_arrow = vizcraft_diagram::Arrow::from_name(&rt.end_arrow);
    let id = r.id.clone();
    let pack = cat().pack_ref(r.pack());
    s.doc.model.add_relationship(r).map_err(model_err)?;
    if let Some(p) = pack {
        s.doc.model.use_pack(p);
    }
    let found = (0..s.doc.pages.len()).find_map(|i| Some((i, shape_on(&s.doc, i, &row)?, shape_on(&s.doc, i, &col)?)));
    let mut out = json!({"linked": true, "relationships": [id]});
    if let Some((page, a, b)) = found {
        let cid = s.doc.next_id("c");
        let mut c = Connector::new(cid.clone(), End::dynamic(a), End::dynamic(b));
        c.route = Route::Orthogonal;
        if let Some(arrow) = end_arrow {
            c.end_arrow = arrow;
        }
        c.relationship = Some(id);
        c.sanitize();
        if let Some(pg) = s.doc.pages.get_mut(page) {
            pg.connectors.push(c);
            if page == 0 {
                select(s, vec![cid.clone()]);
            }
        }
        if let Some(o) = out.as_object_mut() {
            o.insert("connector".into(), json!(cid));
        }
    }
    with_sel(s, out)
}

/// The `subject` param when it names a net table (`nets`, `connections`); `elements` and
/// `relationships` (or none) are the model lists.
fn net_subject(v: &Value) -> Result<Option<&str>, EngineError> {
    match opt_str(v, "subject")? {
        None | Some("elements" | "relationships") => Ok(None),
        Some(sub) if super::table_nets::SUBJECTS.contains(&sub) => Ok(Some(sub)),
        Some(other) => Err(EngineError::Params(format!("`subject` must be elements, relationships, nets or connections, not `{}`", short(other)))),
    }
}

/// The view an export or a screen shows, as columns and text rows.
fn view_of(s: &Session, v: &Value) -> Result<View, EngineError> {
    match opt_str(v, "view")?.unwrap_or("list") {
        "list" if net_subject(v)?.is_some() => {
            let subject = net_subject(v)?.unwrap_or("nets");
            let t = super::table_nets::build(&s.doc, subject, v)?;
            let name = if subject == "nets" { "Nets" } else { "Connections" };
            Ok(View { name: name.into(), columns: t.columns, rows: t.rows, truncated: t.truncated })
        }
        "list" => {
            let (subject, t, type_id) = build_list(s, v)?;
            let name = type_id
                .as_deref()
                .map(|t| t.rsplit('.').next().unwrap_or(t).to_string())
                .unwrap_or_else(|| if subject == Subject::Elements { "Elements".to_string() } else { "Relationships".to_string() });
            Ok(View { name, columns: t.columns, rows: t.rows, truncated: t.truncated })
        }
        "matrix" => {
            let m = build_matrix(s, v)?;
            let mut columns = vec!["Element".to_string()];
            columns.extend(m.cols.iter().map(|c| c.label.clone()));
            let rows = m
                .rows
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let mut row = vec![r.label.clone()];
                    row.extend((0..m.cols.len()).map(|j| m.cell(i, j).map(|c| c.text.clone()).unwrap_or_default()));
                    row
                })
                .collect();
            Ok(View { name: "Matrix".into(), columns, rows, truncated: m.truncated })
        }
        "pack" => {
            let (name, columns, rows) = super::plant::list_view(s, v)?;
            Ok(View { name, columns, rows, truncated: false })
        }
        other => Err(EngineError::Params(format!("`view` must be list, matrix or pack, not `{}`", short(other)))),
    }
}

fn export(s: &mut Session, v: &Value) -> CmdResult {
    let format = p::req_str(v, "format")?;
    if !matches!(format, "csv" | "xlsx" | "markdown") {
        return Err(EngineError::Params(format!("`format` must be csv, xlsx or markdown, not `{}`", short(format))));
    }
    let path = opt_str(v, "path")?;
    if format == "xlsx" && path.is_none() {
        return Err(EngineError::Params("`path` is required for xlsx: a workbook is a binary file".into()));
    }
    let view = view_of(s, v)?;
    let table = vizcraft_formats::tables::from_text(&view.columns, view.rows.clone());
    let (text, bytes) = match format {
        "csv" => {
            let t = vizcraft_formats::tables::write_csv(&table);
            (Some(t.clone()), t.into_bytes())
        }
        "markdown" => {
            let t = vizcraft_formats::tables::write_markdown(&table);
            (Some(t.clone()), t.into_bytes())
        }
        _ => (None, vizcraft_formats::tables::write_xlsx(&table, &view.name).map_err(|e| EngineError::Failed(e.to_string()))?),
    };
    let mut out = json!({"format": format, "columns": view.columns, "rows": view.rows, "truncated": view.truncated});
    if let Some(o) = out.as_object_mut() {
        match (path, text) {
            (Some(path), _) => {
                std::fs::write(path, &bytes).map_err(|e| EngineError::Failed(format!("{}: {e}", short(path))))?;
                o.insert("path".into(), json!(path));
                o.insert("bytes".into(), json!(bytes.len()));
            }
            (None, Some(t)) => {
                o.insert("content".into(), json!(t));
            }
            (None, None) => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The sample flowchart as model elements and flows, drawn on one page.
    fn sample() -> Session {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        let nodes = [
            ("start", "flowchart.Terminator", "Start"),
            ("gather", "flowchart.Process", "Gather input"),
            ("valid", "flowchart.Decision", "Valid?"),
            ("save", "flowchart.Process", "Save diagram"),
            ("end", "flowchart.Terminator", "End"),
            ("fix", "flowchart.Process", "Fix input"),
        ];
        for (i, (id, ty, name)) in nodes.iter().enumerate() {
            run(&mut s, "element.create", json!({"id": id, "type": ty, "name": name, "x": (i % 3) as f64 * 200.0, "y": (i / 3) as f64 * 150.0}));
        }
        for (i, (a, b)) in
            [("start", "gather"), ("gather", "valid"), ("valid", "save"), ("valid", "fix"), ("fix", "gather"), ("save", "end")].iter().enumerate()
        {
            run(&mut s, "relationship.create", json!({"id": format!("f{i}"), "from": a, "to": b}));
        }
        s
    }

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    #[test]
    fn the_sample_list_has_a_row_per_flowchart_element_with_its_properties() {
        let mut s = sample();
        let want = s.doc.model.elements.iter().filter(|e| e.pack() == "flowchart").count();
        let r = run(&mut s, "table.list", json!({"pack": "flowchart"}));
        assert_eq!(r["rows"].as_array().unwrap().len(), want);
        assert_eq!(r["count"], want);
        let cols = r["columns"].as_array().unwrap();
        assert_eq!(cols[..4], [json!("id"), json!("type"), json!("tag"), json!("name")]);
        assert!(cols.contains(&json!("owner")) && cols.contains(&json!("automated")), "declared properties: {cols:?}");
        let r = run(&mut s, "table.list", json!({"type": "flowchart.Process", "columns": ["name", "owner"]}));
        let names: Vec<&str> = r["rows"].as_array().unwrap().iter().map(|x| x[0].as_str().unwrap()).collect();
        assert_eq!(names, ["Gather input", "Save diagram", "Fix input"]);
        let r = run(&mut s, "table.list", json!({"type": "flowchart.Process", "filter": "SAVE"}));
        assert_eq!(r["rows"].as_array().unwrap().len(), 1);
        let r = run(&mut s, "table.list", json!({"type": "flowchart.Flow"}));
        assert_eq!(r["subject"], "relationships");
        assert_eq!(r["rows"].as_array().unwrap().len(), s.doc.model.relationships.len());
    }

    #[test]
    fn the_process_by_process_matrix_marks_the_samples_flows() {
        let mut s = sample();
        let r = run(&mut s, "table.matrix", json!({"rows": "flowchart.Process", "cols": "flowchart.Process", "relationship": "flowchart.Flow"}));
        let labels: Vec<&str> = r["rows"].as_array().unwrap().iter().map(|x| x["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["Gather input", "Save diagram", "Fix input"]);
        // Gather and Save reach Fix only through the decision; Fix flows back to Gather.
        let cells: Vec<(u64, u64)> =
            r["cells"].as_array().unwrap().iter().map(|c| (c["row"].as_u64().unwrap(), c["col"].as_u64().unwrap())).collect();
        assert_eq!(cells, [(2, 0)]);
        assert_eq!(r["cells"][0]["text"], "x");
        assert!(s.run("table.matrix", &json!({"rows": "flowchart.Process", "cols": "nope.Thing"})).is_err());
    }

    #[test]
    fn set_renames_through_the_model_as_one_undo_step() {
        let mut s = sample();
        let id = s.doc.model.elements.iter().find(|e| e.name == "Gather input").unwrap().id.clone();
        let steps = s.transactions().len();
        run(&mut s, "table.set", json!({"id": id, "key": "name", "value": "Collect input"}));
        assert_eq!(s.doc.model.element(&id).unwrap().name, "Collect input");
        assert!(s.doc.pages[0].shapes.iter().any(|sh| sh.text == "Collect input"), "the view follows");
        assert_eq!(s.transactions().len(), steps + 1);
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model.element(&id).unwrap().name, "Gather input", "one undo takes the whole edit back");
        assert!(s.doc.pages[0].shapes.iter().any(|sh| sh.text == "Gather input"));
    }

    #[test]
    fn set_edits_tags_and_typed_properties_and_refuses_the_rest() {
        let mut s = sample();
        let id = s.doc.model.elements.iter().find(|e| e.type_id == "flowchart.Process").unwrap().id.clone();
        run(&mut s, "table.set", json!({"id": id, "key": "tag", "value": "P1"}));
        run(&mut s, "table.set", json!({"id": id, "key": "owner", "value": "Ana"}));
        let r = run(&mut s, "table.set", json!({"id": id, "key": "automated", "value": "yes"}));
        assert_eq!(r["value"], "true");
        let e = s.doc.model.element(&id).unwrap();
        assert_eq!(
            (e.tag.as_str(), e.prop("owner"), e.prop("automated")),
            ("P1", Some(&PropValue::Text("Ana".into())), Some(&PropValue::Bool(true)))
        );
        run(&mut s, "table.set", json!({"id": id, "key": "owner", "value": ""}));
        assert!(s.doc.model.element(&id).unwrap().prop("owner").is_none());
        for bad in [
            json!({"id": id, "key": "automated", "value": "maybe"}),
            json!({"id": id, "key": "id", "value": "x"}),
            json!({"id": id, "key": "type", "value": "x"}),
            json!({"id": "nope", "key": "name", "value": "x"}),
            json!({"id": id, "key": "name"}),
            json!({"id": id, "key": "name", "value": 4}),
        ] {
            assert!(s.run("table.set", &bad).is_err(), "{bad}");
        }
        let rel = s.doc.model.relationships[0].id.clone();
        run(&mut s, "table.set", json!({"id": rel, "key": "condition", "value": "yes"}));
        run(&mut s, "table.set", json!({"id": rel, "key": "name", "value": "go"}));
        let r = s.doc.model.relationship(&rel).unwrap();
        assert_eq!((r.name.as_str(), r.prop("condition")), ("go", Some(&PropValue::Text("yes".into()))));
    }

    #[test]
    fn toggle_creates_and_removes_a_relationship_and_its_connector() {
        let mut s = sample();
        let find = |s: &Session, n: &str| s.doc.model.elements.iter().find(|e| e.name == n).unwrap().id.clone();
        let (a, b) = (find(&s, "Gather input"), find(&s, "Save diagram"));
        let (rels, conns) = (s.doc.model.relationships.len(), s.doc.pages[0].connectors.len());
        let steps = s.transactions().len();
        let r = run(&mut s, "table.toggle", json!({"row": a, "col": b, "relationship": "flowchart.Flow"}));
        assert_eq!(r["linked"], true);
        assert_eq!((s.doc.model.relationships.len(), s.doc.pages[0].connectors.len()), (rels + 1, conns + 1));
        assert_eq!(s.transactions().len(), steps + 1);
        let r = run(&mut s, "table.toggle", json!({"row": a, "col": b, "relationship": "flowchart.Flow"}));
        assert_eq!(r["linked"], false);
        assert_eq!((s.doc.model.relationships.len(), s.doc.pages[0].connectors.len()), (rels, conns));
        assert!(s.run("table.toggle", &json!({"row": a, "col": b, "relationship": "nope.Rel"})).is_err());
        assert!(s.run("table.toggle", &json!({"row": "zzz", "col": b, "relationship": "flowchart.Flow"})).is_err());
    }

    #[test]
    fn export_writes_csv_markdown_and_xlsx_and_a_matrix_too() {
        let mut s = sample();
        let r = run(&mut s, "table.export", json!({"view": "list", "type": "flowchart.Process", "columns": ["id", "name"], "format": "csv"}));
        let csv = r["content"].as_str().unwrap();
        assert!(csv.starts_with("id,name\n") && csv.contains(",Gather input\n"), "{csv}");
        let r = run(&mut s, "table.export", json!({"view": "list", "type": "flowchart.Process", "columns": ["name"], "format": "markdown"}));
        assert_eq!(r["content"], "| name |\n| --- |\n| Gather input |\n| Save diagram |\n| Fix input |\n");
        let dir = std::env::temp_dir().join(format!("vizcraft-table-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.xlsx");
        let r = run(
            &mut s,
            "table.export",
            json!({"view": "matrix", "rows": "flowchart.Process", "cols": "flowchart.Process", "format": "xlsx", "path": path.to_string_lossy()}),
        );
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(r["bytes"], bytes.len());
        #[cfg(not(target_arch = "wasm32"))]
        {
            let t = vizcraft_formats::table::read(&bytes, vizcraft_diagram::data_graphic::TableFormat::Xlsx, "").unwrap();
            assert_eq!(t.columns.len(), 4);
            assert_eq!(t.cell(2, 1), "x");
        }
        std::fs::remove_dir_all(&dir).unwrap();
        for bad in [json!({"format": "xlsx"}), json!({"format": "pdf"}), json!({"format": "csv", "view": "cube"}), json!({})] {
            assert!(s.run("table.export", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn reads_change_nothing() {
        let mut s = sample();
        let before = (s.doc.clone(), s.transactions().len());
        run(&mut s, "table.list", json!({}));
        run(&mut s, "table.matrix", json!({"rows": "flowchart.Process", "cols": "flowchart.Decision"}));
        run(&mut s, "table.export", json!({"format": "csv"}));
        assert_eq!((s.doc.clone(), s.transactions().len()), before);
    }
}
