//! Shape data commands (`data.*`): typed key and value rows on shapes, the page's data key
//! definitions, and the merged view of a shape's data with its model element's typed properties.
//!
//! The rules for keys, types and values live in `vizcraft_diagram::data`. A key the shape's model
//! element owns (a property its pack defines, or one it already holds) is read-only here: it shows
//! in `data.get` and `data.list` marked as model-owned and is written with `element.setProperty`.
//! A bad value is a params error; a required key left empty is a warning in the result.

use serde_json::{Map, Value, json};
use vizcraft_diagram::data::{self, DataKey, DataRow, DataType, MAX_LABEL, MAX_PAGE_KEYS, MAX_ROWS, MAX_UNIT, MAX_VALUE, MAX_VALUES};
use vizcraft_diagram::vizcraft_model::PropValue;
use vizcraft_diagram::{Document, MAX_TEXT, Shape};
use vizcraft_notation::schema::PropKind;

use super::model::cat;
use super::{MAX_IDS, ids_param, opt_bool, opt_str, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the commands live in the UI.
const LOCATION: &str = "Data > Shape Data";

/// The shape data commands, in menu order.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("data.set", "Set Shape Data", LOCATION, set).params(
            r#"{"ids"?: [shape id] (default: the selected shapes), "key": string, "value": string|number|bool|null (null or "" clears), "type"?: "string|number|bool|date|enum|url", "label"?: string, "unit"?: string, "values"?: [string], "required"?: bool, "hidden"?: bool} -> {changed, warnings}; a new row takes the page's definition of the key, else `type` (default: from the value); a key the shape's model element owns is refused (use element.setProperty)"#,
        ),
        CommandSpec::new("data.remove", "Remove Shape Data", LOCATION, remove)
            .params(r#"{"ids"?: [shape id] (default: the selected shapes), "key": string} -> {changed}; refused when no shape has the key"#),
        CommandSpec::new("data.defineKey", "Define Shape Data", LOCATION, define_key).params(
            r#"{"key": string, "type": "string|number|bool|date|enum|url", "label"?: string, "unit"?: string, "values"?: [string] (enum), "required"?: bool, "default"?: value, "page"?: id|name|index, "ids"?: [shape id] (opt these in)} -> {page, key, changed, warnings}; adds or updates the key on every shape of the page that uses page keys"#,
        ),
        CommandSpec::new("data.undefineKey", "Delete Shape Data Key", LOCATION, undefine_key).params(
            r#"{"key": string, "page"?: id|name|index, "keepRows"?: bool} -> {page, key, changed}; also removes the key's rows from shapes that use page keys unless keepRows"#,
        ),
        CommandSpec::new("data.usePageKeys", "Use Page Data Keys", LOCATION, use_page_keys).params(
            r#"{"ids"?: [shape id] (default: the selected shapes), "value"?: bool (default true)} -> {changed}; on adds every key the page defines and keeps adding new ones; off stops (rows stay)"#,
        ),
        CommandSpec::new("data.get", "Shape Data", LOCATION, get).params(
            r#"{"id"?: shape id (default: the primary selection)} -> {id, element?, rows: [{key, label, type, unit, values, required, default, value, text, hidden, source: "shape"|"model", readOnly, pageKey}], keys: [page key], warnings}"#,
        ),
        CommandSpec::new("data.list", "Shape Data Table", LOCATION, list).params(
            r#"{"page"?: id|name|index} -> {page, keys: [string], shapes: [{id, values: {key: value}}]}; shapes with any data, model properties included"#,
        ),
        CommandSpec::new("data.defineDialog", "Define Shape Data", "Data > Show/Hide", |s, _| {
            s.ui_requests.push(json!({"open": "defineShapeData"}));
            Ok(json!({"dialog": "defineShapeData"}))
        })
        .params("{} -> opens the Define Shape Data dialog in the app; agents call data.defineKey"),
        CommandSpec::new("view.shapeDataPane", "Inspector", "Data > Show/Hide", |s, v| {
            let value = opt_bool(v, "value")?;
            let req = json!({"pane": "shapeData", "value": value});
            s.ui_requests.push(req.clone());
            Ok(req)
        })
        .params(r#"{"value"?: bool (default: toggle)} -> shows or hides the Shape Data pane in the app"#),
    ]
}

/// The required `key` param: a usable data key.
fn key_param(v: &Value) -> Result<String, EngineError> {
    let k = opt_str(v, "key")?.ok_or_else(|| EngineError::Params("`key` (string) is required".into()))?;
    if !data::is_key(k) {
        return Err(EngineError::Params(format!(
            "`key` must be 1 to {} letters, digits, `_`, `-`, `.` or inner spaces, not `{}`",
            data::MAX_KEY,
            short(k)
        )));
    }
    Ok(k.to_string())
}

/// An optional text param of at most `max` bytes.
fn text_param(v: &Value, k: &str, max: usize) -> Result<Option<String>, EngineError> {
    match opt_str(v, k)? {
        Some(t) if t.len() > max => Err(EngineError::Params(format!("`{k}` is at most {max} bytes"))),
        t => Ok(t.map(|t| t.trim().to_string())),
    }
}

/// The optional `type` param.
fn type_param(v: &Value) -> Result<Option<DataType>, EngineError> {
    match opt_str(v, "type")? {
        None => Ok(None),
        Some(t) => DataType::from_name(t.trim())
            .map(Some)
            .ok_or_else(|| EngineError::Params(format!("`type` must be string, number, bool, date, enum or url, not `{}`", short(t)))),
    }
}

/// The optional `values` param: enum values, trimmed, unique, not empty.
fn values_param(v: &Value) -> Result<Option<Vec<String>>, EngineError> {
    let bad = || EngineError::Params(format!("`values` must be a list of at most {MAX_VALUES} strings of at most {MAX_VALUE} bytes"));
    match v.get("values") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) if a.len() <= MAX_VALUES => {
            let mut out: Vec<String> = Vec::new();
            for x in a {
                let t = x.as_str().filter(|t| t.len() <= MAX_VALUE).ok_or_else(bad)?.trim();
                if !t.is_empty() && !out.iter().any(|o| o == t) {
                    out.push(t.to_string());
                }
            }
            Ok(Some(out))
        }
        Some(_) => Err(bad()),
    }
}

/// The page the `page` param names (id, name or index), else the active page.
fn page_index(s: &Session, v: &Value) -> Result<usize, EngineError> {
    let r = match v.get("page") {
        None | Some(Value::Null) => {
            return if s.doc.pages.is_empty() { Err(EngineError::Failed("the document has no page".into())) } else { Ok(s.doc.active_index()) };
        }
        Some(Value::String(t)) => t.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(_) => return Err(EngineError::Params("`page` must be a page id, name or index".into())),
    };
    s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r))))
}

/// Where a shape is: page index and shape index.
fn locate(doc: &Document, id: &str) -> Option<(usize, usize)> {
    doc.pages.iter().enumerate().find_map(|(pi, p)| p.shapes.iter().position(|sh| sh.id == id).map(|si| (pi, si)))
}

/// The shapes a command acts on: `ids` (each a shape on any page), else the selected shapes.
fn targets(s: &Session, v: &Value) -> Result<Vec<(String, usize, usize)>, EngineError> {
    let ids = match ids_param(v)? {
        Some(ids) => ids,
        None => {
            let page = s.doc.page();
            s.sel.items.iter().filter(|id| page.is_some_and(|p| p.shape(id).is_some())).cloned().collect()
        }
    };
    let mut out: Vec<(String, usize, usize)> = Vec::new();
    for id in ids {
        let (pi, si) = locate(&s.doc, &id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape", short(&id))))?;
        if !out.iter().any(|(o, ..)| *o == id) {
            out.push((id, pi, si));
        }
    }
    if out.is_empty() {
        return Err(EngineError::Params("select a shape first (or pass `ids`)".into()));
    }
    out.truncate(MAX_IDS);
    Ok(out)
}

/// Refuse when any target is on a locked layer of its own page.
fn ensure_targets_unlocked(s: &Session, targets: &[(String, usize, usize)]) -> Result<(), EngineError> {
    for (id, pi, _) in targets {
        ensure_unlocked_on(s, *pi, std::slice::from_ref(id))?;
    }
    Ok(())
}

fn shape_at(doc: &mut Document, pi: usize, si: usize) -> Result<&mut Shape, EngineError> {
    doc.pages.get_mut(pi).and_then(|p| p.shapes.get_mut(si)).ok_or_else(|| EngineError::Failed("the shape went away".into()))
}

/// One row as the Shape Data pane and `data.get` show it: a shape data row, or a typed property
/// of the element the shape draws (`model`, read-only here).
#[derive(Clone, Debug, PartialEq)]
pub struct RowView {
    /// The key.
    pub key: String,
    /// The label, or the key when there is none.
    pub label: String,
    /// Value type.
    pub kind: DataType,
    /// Unit shown beside numbers.
    pub unit: String,
    /// Allowed values of an enum.
    pub values: Vec<String>,
    /// The value must be set.
    pub required: bool,
    /// The starting value.
    pub default: Option<String>,
    /// The value as canonical text; empty is not set.
    pub value: String,
    /// Hidden from the pane unless hidden rows are shown.
    pub hidden: bool,
    /// Owned by the model element: written with `element.setProperty`, not `data.set`.
    pub model: bool,
    /// The page defines this key.
    pub page_key: bool,
}

impl RowView {
    fn shape(r: &DataRow, page_key: bool) -> Self {
        RowView {
            key: r.key.clone(),
            label: r.title().to_string(),
            kind: r.kind,
            unit: r.unit.clone(),
            values: r.values.clone(),
            required: r.required,
            default: r.default.clone(),
            value: r.value.clone(),
            hidden: r.hidden,
            model: false,
            page_key,
        }
    }
    fn json(&self) -> Value {
        json!({
            "key": self.key, "label": self.label, "type": self.kind.name(), "unit": self.unit, "values": self.values,
            "required": self.required, "default": self.default, "value": data::typed_value(self.kind, &self.value), "text": self.value,
            "hidden": self.hidden, "source": if self.model { "model" } else { "shape" }, "readOnly": self.model, "pageKey": self.page_key,
        })
    }
}

/// Every row of shape `id` (on any page): its element's typed properties first, then its own
/// rows. `None` when there is no such shape.
pub fn rows_of(doc: &Document, id: &str) -> Option<Vec<RowView>> {
    let (pi, si) = locate(doc, id)?;
    let page = doc.pages.get(pi)?;
    let sh = page.shapes.get(si)?;
    let defs = data::page_keys(page);
    let mut rows = model_rows(doc, sh);
    rows.extend(sh.data.rows.iter().map(|r| RowView::shape(r, defs.iter().any(|d| d.key == r.key))));
    Some(rows)
}

fn prop_text(v: &PropValue) -> String {
    match v {
        PropValue::Text(t) => t.clone(),
        PropValue::Number(n) => data::fmt_number(*n),
        PropValue::Bool(b) => b.to_string(),
    }
}

/// The typed properties of the element `shape` draws: the pack's schema first, in its order,
/// then any other property the element holds. Empty for a free-drawing shape.
fn model_rows(doc: &Document, shape: &Shape) -> Vec<RowView> {
    let Some(el) = shape.element.as_deref().and_then(|e| doc.model.element(e)) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some((_, ty)) = cat().element_type(&el.type_id) {
        for p in &ty.properties {
            let kind = match p.kind {
                PropKind::Text => DataType::String,
                PropKind::Number | PropKind::Integer => DataType::Number,
                PropKind::Bool => DataType::Bool,
                PropKind::Enum => DataType::Enum,
            };
            out.push(RowView {
                key: p.key.clone(),
                label: if p.label.is_empty() { p.key.clone() } else { p.label.clone() },
                kind,
                unit: p.unit.clone(),
                values: p.values.clone(),
                required: p.required,
                default: p.default.as_ref().map(prop_text),
                value: el.properties.get(&p.key).map(prop_text).unwrap_or_default(),
                hidden: false,
                model: true,
                page_key: false,
            });
        }
    }
    for (k, v) in &el.properties {
        if !out.iter().any(|r| r.key == *k) {
            let kind = match v {
                PropValue::Number(_) => DataType::Number,
                PropValue::Bool(_) => DataType::Bool,
                PropValue::Text(_) => DataType::String,
            };
            out.push(RowView {
                key: k.clone(),
                label: k.clone(),
                kind,
                unit: String::new(),
                values: Vec::new(),
                required: false,
                default: None,
                value: prop_text(v),
                hidden: false,
                model: true,
                page_key: false,
            });
        }
    }
    out
}

/// Refuse a key the shape's model element owns.
fn ensure_not_model(doc: &Document, shape: &Shape, key: &str) -> Result<(), EngineError> {
    if model_rows(doc, shape).iter().any(|r| r.key == key) {
        let el = shape.element.as_deref().unwrap_or_default();
        return Err(EngineError::Params(format!(
            "`{}` on `{}` is a property of element `{}`; change it with element.setProperty",
            short(key),
            short(&shape.id),
            short(el)
        )));
    }
    Ok(())
}

/// Warnings for required rows left empty on `ids`.
fn warnings(doc: &Document, ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        if let Some(sh) = doc.shape(id) {
            for r in sh.data.rows.iter().filter(|r| r.missing()) {
                out.push(format!("`{}` on `{}` is required but empty", r.key, short(id)));
            }
        }
    }
    out
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let key = key_param(v)?;
    let value = v.get("value").ok_or_else(|| EngineError::Params("`value` is required (null clears it)".into()))?;
    if matches!(value, Value::Array(_) | Value::Object(_)) {
        return Err(EngineError::Params("`value` must be a string, number, true or false, or null".into()));
    }
    let kind = type_param(v)?;
    let label = text_param(v, "label", MAX_LABEL)?;
    let unit = text_param(v, "unit", MAX_UNIT)?;
    let values = values_param(v)?;
    let required = opt_bool(v, "required")?;
    let hidden = opt_bool(v, "hidden")?;
    let targets = targets(s, v)?;
    ensure_targets_unlocked(s, &targets)?;
    let mut changed = Vec::new();
    for (id, pi, si) in &targets {
        let sh = s.doc.pages.get(*pi).and_then(|p| p.shapes.get(*si)).ok_or_else(|| EngineError::Failed("the shape went away".into()))?;
        ensure_not_model(&s.doc, sh, &key)?;
        let defs = s.doc.pages.get(*pi).map(data::page_keys).unwrap_or_default();
        let mut row = match sh.data.row(&key) {
            Some(r) => r.clone(),
            None => {
                if sh.data.rows.len() >= MAX_ROWS {
                    return Err(EngineError::Params(format!("`{}` already has {MAX_ROWS} data rows", short(id))));
                }
                match defs.iter().find(|d| d.key == key) {
                    Some(d) => d.row(),
                    None => DataRow { key: key.clone(), kind: kind.unwrap_or_else(|| data::infer_type(value)), ..DataRow::default() },
                }
            }
        };
        if let Some(k) = kind {
            row.kind = k;
        }
        if let Some(l) = &label {
            row.label = l.clone();
        }
        if let Some(u) = &unit {
            row.unit = u.clone();
        }
        if let Some(vs) = &values {
            row.values = vs.clone();
        }
        if let Some(r) = required {
            row.required = r;
        }
        if let Some(h) = hidden {
            row.hidden = h;
        }
        if row.kind == DataType::Enum && row.values.is_empty() {
            return Err(EngineError::Params(format!("`{}` is an enum; give its `values`", short(&key))));
        }
        row.value = data::normalize(row.kind, &row.values, &row.unit, value)
            .map_err(|e| EngineError::Params(format!("`{}` on `{}`: {e}", short(&key), short(id))))?;
        let sh = shape_at(&mut s.doc, *pi, *si)?;
        let before = sh.data.clone();
        match sh.data.row_mut(&key) {
            Some(r) => *r = row,
            None => sh.data.rows.push(row),
        }
        sh.data.sanitize();
        if sh.data != before {
            changed.push(id.clone());
        }
    }
    let ids: Vec<String> = targets.into_iter().map(|(id, ..)| id).collect();
    let warnings = warnings(&s.doc, &ids);
    with_sel(s, json!({"key": key, "changed": changed, "warnings": warnings}))
}

fn remove(s: &mut Session, v: &Value) -> CmdResult {
    let key = key_param(v)?;
    let targets = targets(s, v)?;
    ensure_targets_unlocked(s, &targets)?;
    let mut changed = Vec::new();
    for (id, pi, si) in &targets {
        let sh = shape_at(&mut s.doc, *pi, *si)?;
        let n = sh.data.rows.len();
        sh.data.rows.retain(|r| r.key != key);
        if sh.data.rows.len() != n {
            changed.push(id.clone());
        }
    }
    if changed.is_empty() {
        return Err(EngineError::Params(format!("no shape here has shape data `{}`", short(&key))));
    }
    with_sel(s, json!({"key": key, "changed": changed}))
}

fn define_key(s: &mut Session, v: &Value) -> CmdResult {
    let key = key_param(v)?;
    let kind = type_param(v)?.ok_or_else(|| EngineError::Params("`type` (string, number, bool, date, enum or url) is required".into()))?;
    let mut def = DataKey {
        key: key.clone(),
        label: text_param(v, "label", MAX_LABEL)?.unwrap_or_default(),
        kind,
        unit: text_param(v, "unit", MAX_UNIT)?.unwrap_or_default(),
        values: values_param(v)?.unwrap_or_default(),
        required: opt_bool(v, "required")?.unwrap_or(false),
        default: None,
    };
    if kind == DataType::Enum && def.values.is_empty() {
        return Err(EngineError::Params("an enum key needs `values`".into()));
    }
    if let Some(d) = v.get("default") {
        let t = data::normalize(kind, &def.values, &def.unit, d).map_err(|e| EngineError::Params(format!("`default`: {e}")))?;
        def.default = Some(t).filter(|t| !t.is_empty());
    }
    let pi = page_index(s, v)?;
    let opt_in = match ids_param(v)? {
        Some(ids) => {
            let page = s.doc.pages.get(pi).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
            for id in &ids {
                if page.shape(id).is_none() {
                    return Err(EngineError::Params(format!("`{}` is not a shape on page `{}`", short(id), short(&page.name))));
                }
            }
            ids
        }
        None => Vec::new(),
    };
    let doc = &mut s.doc;
    let page = doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let mut defs = data::page_keys(page);
    match defs.iter().position(|d| d.key == key) {
        Some(i) => {
            if let Some(d) = defs.get_mut(i) {
                *d = def.clone();
            }
        }
        None if defs.len() >= MAX_PAGE_KEYS => return Err(EngineError::Params(format!("a page defines at most {MAX_PAGE_KEYS} data keys"))),
        None => defs.push(def.clone()),
    }
    if serde_json::to_string(&defs).map_or(true, |t| t.len() > MAX_TEXT) {
        return Err(EngineError::Params("the page's data key definitions would be too large".into()));
    }
    data::set_page_keys(page, &defs);
    let pid = page.id.clone();
    for sh in &mut page.shapes {
        if opt_in.contains(&sh.id) {
            sh.data.page_keys = true;
        }
    }
    let page_ids: Vec<String> = page.shapes.iter().filter(|sh| sh.data.page_keys).map(|sh| sh.id.clone()).collect();
    ensure_unlocked_on(s, pi, &page_ids)?;
    let mut changed = Vec::new();
    for id in &page_ids {
        let Some(sh) = s.doc.pages.get(pi).and_then(|p| p.shape(id)) else { continue };
        // A key the element owns stays the element's; the shape gets no shadowing row.
        let owned: Vec<String> = model_rows(&s.doc, sh).into_iter().map(|r| r.key).collect();
        let Some(sh) = s.doc.pages.get_mut(pi).and_then(|p| p.shape_mut(id)) else { continue };
        let before = sh.data.clone();
        if opt_in.contains(id) {
            let mine: Vec<DataKey> = defs.iter().filter(|d| !owned.contains(&d.key)).cloned().collect();
            data::add_page_rows(sh, &mine);
        }
        if !owned.contains(&key) {
            let room = sh.data.rows.len() < MAX_ROWS;
            match sh.data.row_mut(&key) {
                Some(r) => r.apply(&def).map_err(|e| {
                    EngineError::Params(format!(
                        "`{}` on `{}` does not fit the new definition ({e}); change or remove it first",
                        short(&key),
                        short(id)
                    ))
                })?,
                None if room => sh.data.rows.push(def.row()),
                None => {}
            }
        }
        if sh.data != before {
            changed.push(id.clone());
        }
    }
    let warnings = warnings(&s.doc, &page_ids);
    with_sel(s, json!({"page": pid, "key": serde_json::to_value(&def).unwrap_or(Value::Null), "changed": changed, "warnings": warnings}))
}

/// [`ensure_unlocked`] for shapes on page `pi`: the layer check only knows the active page, so
/// shapes of other pages are checked against their own page here.
fn ensure_unlocked_on(s: &Session, pi: usize, ids: &[String]) -> Result<(), EngineError> {
    let Some(page) = s.doc.pages.get(pi) else { return Ok(()) };
    for id in ids {
        if let Some(sh) = page.shape(id)
            && vizcraft_diagram::layer::locked(page, sh)
        {
            return Err(EngineError::Params(format!("`{}` is on a locked layer", short(id))));
        }
    }
    Ok(())
}

fn undefine_key(s: &mut Session, v: &Value) -> CmdResult {
    let key = key_param(v)?;
    let keep = opt_bool(v, "keepRows")?.unwrap_or(false);
    let pi = page_index(s, v)?;
    let page = s.doc.pages.get(pi).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let mut defs = data::page_keys(page);
    let n = defs.len();
    defs.retain(|d| d.key != key);
    if defs.len() == n {
        return Err(EngineError::Params(format!("page `{}` does not define `{}`", short(&page.name), short(&key))));
    }
    let ids: Vec<String> = if keep { Vec::new() } else { page.shapes.iter().filter(|sh| sh.data.page_keys).map(|sh| sh.id.clone()).collect() };
    ensure_unlocked_on(s, pi, &ids)?;
    let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    data::set_page_keys(page, &defs);
    let mut changed = Vec::new();
    for sh in page.shapes.iter_mut().filter(|sh| ids.contains(&sh.id)) {
        let n = sh.data.rows.len();
        sh.data.rows.retain(|r| r.key != key);
        if sh.data.rows.len() != n {
            changed.push(sh.id.clone());
        }
    }
    let pid = page.id.clone();
    with_sel(s, json!({"page": pid, "key": key, "changed": changed}))
}

fn use_page_keys(s: &mut Session, v: &Value) -> CmdResult {
    let on = opt_bool(v, "value")?.unwrap_or(true);
    let targets = targets(s, v)?;
    ensure_targets_unlocked(s, &targets)?;
    let mut changed = Vec::new();
    for (id, pi, si) in &targets {
        let defs = s.doc.pages.get(*pi).map(data::page_keys).unwrap_or_default();
        let sh = s.doc.pages.get(*pi).and_then(|p| p.shapes.get(*si)).ok_or_else(|| EngineError::Failed("the shape went away".into()))?;
        let owned: Vec<String> = model_rows(&s.doc, sh).into_iter().map(|r| r.key).collect();
        let defs: Vec<DataKey> = defs.into_iter().filter(|d| !owned.contains(&d.key)).collect();
        let sh = shape_at(&mut s.doc, *pi, *si)?;
        let before = sh.data.clone();
        sh.data.page_keys = on;
        if on {
            data::add_page_rows(sh, &defs);
        }
        if sh.data != before {
            changed.push(id.clone());
        }
    }
    let ids: Vec<String> = targets.into_iter().map(|(id, ..)| id).collect();
    let warnings = warnings(&s.doc, &ids);
    with_sel(s, json!({"changed": changed, "warnings": warnings}))
}

fn get(s: &mut Session, v: &Value) -> CmdResult {
    let id = match opt_str(v, "id")? {
        Some(id) => id.to_string(),
        None => s.sel.primary.clone().ok_or_else(|| EngineError::Params("select a shape first (or pass `id`)".into()))?,
    };
    let (pi, si) = locate(&s.doc, &id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape", short(&id))))?;
    let page = s.doc.pages.get(pi).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let sh = page.shapes.get(si).ok_or_else(|| EngineError::Failed("the shape went away".into()))?;
    let defs = data::page_keys(page);
    let rows = rows_of(&s.doc, &id).unwrap_or_default();
    let mut warnings = Vec::new();
    for r in rows.iter().filter(|r| r.model && r.required && r.value.is_empty()) {
        warnings.push(format!("`{}` on `{}` is required but empty (model)", r.key, short(&id)));
    }
    warnings.extend(self::warnings(&s.doc, std::slice::from_ref(&id)));
    let rows: Vec<Value> = rows.iter().map(RowView::json).collect();
    let keys: Vec<Value> = defs.iter().map(|d| serde_json::to_value(d).unwrap_or(Value::Null)).collect();
    Ok(json!({
        "id": id, "page": page.id, "element": sh.element, "pageKeys": sh.data.page_keys,
        "rows": rows, "keys": keys, "warnings": warnings,
    }))
}

fn list(s: &mut Session, v: &Value) -> CmdResult {
    let pi = page_index(s, v)?;
    let page = s.doc.pages.get(pi).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let mut keys: Vec<String> = data::page_keys(page).into_iter().map(|d| d.key).collect();
    let mut shapes = Vec::new();
    for sh in &page.shapes {
        let mut values = Map::new();
        for r in model_rows(&s.doc, sh) {
            values.insert(r.key.clone(), data::typed_value(r.kind, &r.value));
            if !keys.contains(&r.key) {
                keys.push(r.key);
            }
        }
        for r in &sh.data.rows {
            values.entry(r.key.clone()).or_insert_with(|| data::typed_value(r.kind, &r.value));
            if !keys.contains(&r.key) {
                keys.push(r.key.clone());
            }
        }
        if !values.is_empty() {
            shapes.push(json!({"id": sh.id, "values": values}));
        }
    }
    Ok(json!({"page": page.id, "keys": keys, "shapes": shapes}))
}

#[cfg(test)]
mod tests;
