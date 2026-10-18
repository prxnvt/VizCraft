//! Inspector commands: `inspector.get` assembles everything the Inspector pane shows for one
//! element or shape in a single read, and `element.checkTag` answers whether a tag is free in the
//! element's identity scope. Both only read; the pane and agents see the same data.
//!
//! The `id` param is an element id, or the id of a shape (which may draw an element or be free
//! drawing). Relationship and view lists are capped so a hub element in a huge model stays cheap.

use serde_json::{Value, json};
use vizcraft_model::{Element, Registry};
use vizcraft_notation::schema::PropKind;

use super::data::rows_of;
use super::model::{cat, id_param, views_json};
use super::short;
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Most relationships `inspector.get` lists for one element.
pub const MAX_RELATIONSHIPS: usize = 500;

/// Return the inspector commands: `inspector.get` and `element.checkTag`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("inspector.get", "Inspector", "Window > Inspector", get).params(
            r#"{"id": element or shape id} -> {"element"?: {id, type, name, tag, owner?}, "type"?: {id, label, pack}, "identity": {tag, format, free, scope?, conflict?: {id, name}}, "properties": [{key, label, kind, unit, values, required, value, source}], "relationships": [{id, type, label, direction, other, otherName, pages}], "views": [{page, pageName, items}], "shape"?: {id, kind, stencil, text, x, y, w, h, angle}}; read only"#,
        ),
        CommandSpec::new("element.checkTag", "Check Tag", "Model > Elements", check_tag)
            .params(r#"{"id": element or shape id, "tag": string} -> {"free", "scope"?: scope id, "conflict"?: {id, name}}; whether the element could take the tag in its identity scope; read only"#),
    ]
}

/// The element and the shape `id` stands for: an element id, or a shape (with its element when it draws one).
fn resolve<'a>(s: &'a Session, id: &str) -> Result<(Option<&'a Element>, Option<&'a vizcraft_diagram::Shape>), EngineError> {
    if let Some(e) = s.doc.model.element(id) {
        return Ok((Some(e), None));
    }
    let shape =
        s.doc.pages.iter().find_map(|p| p.shape(id)).ok_or_else(|| EngineError::Params(format!("`{}` is not an element or a shape", short(id))))?;
    Ok((shape.element.as_deref().and_then(|e| s.doc.model.element(e)), Some(shape)))
}

/// Whether `tag` is free for `e`, the scope id, and the first other member holding it.
fn tag_status(s: &Session, e: &Element, tag: &str) -> (bool, Option<String>, Option<Value>) {
    let scopes = cat().scopes();
    let reg = Registry::new(&s.doc.model, &scopes);
    let free = reg.is_free(&e.type_id, tag, Some(&e.id));
    let Some(scope) = reg.scope_for(&e.type_id) else { return (free, None, None) };
    let conflict = if free || tag.is_empty() {
        None
    } else {
        reg.members(scope).into_iter().find(|m| m.tag == tag && m.id != e.id).map(|m| json!({"id": m.id, "name": m.name}))
    };
    (free, Some(scope.id.clone()), conflict)
}

fn kind_name(k: PropKind) -> &'static str {
    match k {
        PropKind::Text => "text",
        PropKind::Number => "number",
        PropKind::Integer => "integer",
        PropKind::Bool => "bool",
        PropKind::Enum => "enum",
    }
}

fn prop_json(v: &vizcraft_model::PropValue) -> Value {
    match v {
        vizcraft_model::PropValue::Bool(b) => json!(b),
        vizcraft_model::PropValue::Number(n) => json!(n),
        vizcraft_model::PropValue::Text(t) => json!(t),
    }
}

/// The element's properties: the schema's in its order (`source: "schema"`), then any other the
/// element holds (`"extra"`).
fn element_properties(e: &Element) -> Vec<Value> {
    let mut out = Vec::new();
    let schema = cat().element_type(&e.type_id).map(|(_, t)| t.properties.as_slice()).unwrap_or_default();
    for sp in schema {
        out.push(json!({
            "key": sp.key, "label": if sp.label.is_empty() { &sp.key } else { &sp.label }, "kind": kind_name(sp.kind), "unit": sp.unit,
            "values": sp.values, "required": sp.required, "value": e.properties.get(&sp.key).map(prop_json), "source": "schema",
        }));
    }
    for (k, v) in &e.properties {
        if !schema.iter().any(|sp| sp.key == *k) {
            out.push(json!({"key": k, "label": k, "kind": match v { vizcraft_model::PropValue::Bool(_) => "bool", vizcraft_model::PropValue::Number(_) => "number", _ => "text" },
                "unit": "", "values": [], "required": false, "value": prop_json(v), "source": "extra"}));
        }
    }
    out
}

fn relationships(s: &Session, e: &Element) -> Vec<Value> {
    let m = &s.doc.model;
    let mut out = Vec::new();
    for r in m.relationships.iter().filter(|r| r.source == e.id || r.target == e.id).take(MAX_RELATIONSHIPS) {
        let outgoing = r.source == e.id;
        let other = if outgoing { &r.target } else { &r.source };
        let label = cat().relationship_type(&r.type_id).map(|(_, t)| t.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| r.type_id.clone());
        let pages: Vec<Value> = vizcraft_diagram::views::views_of(&s.doc, &r.id)
            .into_iter()
            .map(|v| json!({"page": v.page_id, "pageName": s.doc.pages.get(v.page).map(|p| p.name.as_str()).unwrap_or_default(), "items": v.items}))
            .collect();
        out.push(json!({
            "id": r.id, "type": r.type_id, "label": label, "direction": if outgoing { "out" } else { "in" }, "other": other,
            "otherName": m.element(other).map(|o| o.name.clone()).unwrap_or_default(), "pages": pages,
        }));
    }
    out
}

fn get(s: &mut Session, v: &Value) -> CmdResult {
    describe(s, id_param(v, "id")?)
}

/// What `inspector.get` reports for `id`, without needing a mutable session (the pane reads it every frame).
pub fn describe(s: &Session, id: &str) -> CmdResult {
    let (element, shape) = resolve(s, id)?;
    let mut out = json!({"properties": [], "relationships": [], "views": [], "identity": {"tag": "", "format": "", "free": true}});
    if let Some(e) = element {
        let (free, scope, conflict) = tag_status(s, e, &e.tag);
        let scopes = cat().scopes();
        let format = Registry::new(&s.doc.model, &scopes).scope_for(&e.type_id).map(|sc| sc.format.clone()).unwrap_or_default();
        let ty = cat().element_type(&e.type_id);
        out["element"] = json!({"id": e.id, "type": e.type_id, "name": e.name, "tag": e.tag, "owner": e.owner});
        out["type"] = json!({"id": e.type_id, "label": ty.map(|(_, t)| t.label.clone()).unwrap_or_default(), "pack": e.pack()});
        out["identity"] = json!({"tag": e.tag, "format": format, "free": free, "scope": scope, "conflict": conflict});
        out["properties"] = Value::Array(element_properties(e));
        out["relationships"] = Value::Array(relationships(s, e));
        out["views"] = views_json(&s.doc, &e.id);
    }
    if let Some(sh) = shape {
        if let Some(props) = out["properties"].as_array_mut() {
            for r in rows_of(&s.doc, &sh.id).unwrap_or_default().into_iter().filter(|r| !r.model) {
                props.push(json!({
                    "key": r.key, "label": r.label, "kind": r.kind.name(), "unit": r.unit, "values": r.values, "required": r.required,
                    "value": r.value, "source": if r.page_key { "page" } else { "shape" },
                }));
            }
        }
        out["shape"] = json!({"id": sh.id, "kind": sh.kind.name(), "stencil": sh.stencil, "text": sh.text, "x": sh.x, "y": sh.y, "w": sh.w, "h": sh.h, "angle": sh.angle_deg});
    }
    Ok(out)
}

fn check_tag(s: &mut Session, v: &Value) -> CmdResult {
    check(s, id_param(v, "id")?, p::req_str(v, "tag")?)
}

/// What `element.checkTag` reports for `id` and `tag`; the pane calls it on every keystroke of the tag field.
pub fn check(s: &Session, id: &str, tag: &str) -> CmdResult {
    let (element, _) = resolve(s, id)?;
    let e = element.ok_or_else(|| EngineError::Params(format!("`{}` draws no element", short(id))))?;
    let (free, scope, conflict) = tag_status(s, e, tag);
    Ok(json!({"free": free, "scope": scope, "conflict": conflict}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::model::fixture::{flow, refused, run, shapes_of};

    #[test]
    fn check_tag_reports_the_conflict() {
        let mut s = flow();
        run(&mut s, "element.retag", json!({"id": "do", "tag": "S1"}));
        let r = run(&mut s, "element.retag", json!({"id": "ok", "tag": "S2"}));
        assert_eq!(r["tag"], "S2");
        let taken = run(&mut s, "element.checkTag", json!({"id": "ok", "tag": "S1"}));
        assert_eq!(taken["free"], false);
        assert_eq!(taken["conflict"]["id"], "do");
        assert!(taken["scope"].is_string());
        let free = run(&mut s, "element.checkTag", json!({"id": "ok", "tag": "S9"}));
        assert_eq!(free["free"], true);
        assert!(free["conflict"].is_null());
        // Its own tag is free for it, and a shape id works as well as the element id.
        let sid = shapes_of(&s, "do")[0].clone();
        let own = run(&mut s, "element.checkTag", json!({"id": sid, "tag": "S1"}));
        assert_eq!(own["free"], true);
        refused(&mut s, "element.checkTag", json!({"id": "f1", "tag": "x"}));
        refused(&mut s, "element.checkTag", json!({"id": "do"}));
    }

    #[test]
    fn get_carries_schema_properties_relationships_and_views() {
        let mut s = flow();
        run(&mut s, "element.setProperty", json!({"id": "do", "key": "owner", "value": "ops"}));
        let before = s.doc.clone();
        let r = run(&mut s, "inspector.get", json!({"id": "do"}));
        assert_eq!(s.doc, before, "read only");
        assert_eq!(r["element"]["id"], "do");
        assert_eq!(r["type"]["pack"], "flowchart");
        let props = r["properties"].as_array().unwrap();
        let owner = props.iter().find(|p| p["key"] == "owner").unwrap();
        assert_eq!((owner["value"].as_str(), owner["source"].as_str()), (Some("ops"), Some("schema")));
        assert!(props.iter().all(|p| p["kind"].is_string() && p["values"].is_array()));
        let rels = r["relationships"].as_array().unwrap();
        assert_eq!(rels.len(), 2);
        assert!(rels.iter().any(|x| x["direction"] == "in" && x["otherName"] == "Start"));
        assert!(rels.iter().any(|x| x["direction"] == "out" && x["otherName"] == "Ok?"));
        let views = run(&mut s, "model.views", json!({"id": "do"}));
        assert_eq!(r["views"].as_array().unwrap().len(), views["views"].as_array().unwrap().len());
        assert_eq!(r["identity"]["free"], true);
        // A shape id adds the shape block; an unknown id is refused.
        let shape = shapes_of(&s, "do")[0].clone();
        let r = run(&mut s, "inspector.get", json!({"id": shape}));
        assert_eq!(r["shape"]["id"], json!(shape));
        assert_eq!(r["element"]["id"], "do");
        refused(&mut s, "inspector.get", json!({"id": "nope"}));
    }
}
