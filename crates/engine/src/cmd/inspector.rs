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
            r#"{"id": element or shape id} -> {"element"?: {id, type, name, tag, owner?}, "type"?: {id, label, pack}, "identity": {tag, format, free, scope?, conflict?: {id, name}}, "properties": [{key, label, kind, unit, values, required, value, source}], "relationships": [{id, type, label, direction, other, otherName, pages}], "views": [{page, pageName, items}], "shape"?: {id, kind, stencil, text, x, y, w, h, angle}}; read only. For a relationship or a connector id: {"relationship": {id, type, name, source, target, sourceName, targetName, connectors: [connector ids]} | null, "type": {id, label, pack}, "properties": [{..., "end": "start"|"end"|null}], "connector"?: {id, kind, text, startLabel, endLabel}}; the end of a property comes from the type's endLabels, else from a `source` or `target` key prefix; a connector with no relationship gives `relationship: null` and empty lists"#,
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

/// The end of a relationship property: `start` or `end` when the type's `endLabels` list the key,
/// else by a `source` or `target` prefix followed by a capital letter (`sourceRole`), else `None`.
fn end_of(rt: Option<&vizcraft_notation::schema::RelationshipType>, key: &str) -> Option<&'static str> {
    if let Some(rt) = rt {
        if rt.end_labels.start.iter().any(|k| k == key) {
            return Some("start");
        }
        if rt.end_labels.end.iter().any(|k| k == key) {
            return Some("end");
        }
    }
    let after = |prefix: &str| key.strip_prefix(prefix).and_then(|r| r.chars().next()).is_some_and(|c| c.is_ascii_uppercase());
    if after("source") {
        Some("start")
    } else if after("target") {
        Some("end")
    } else {
        None
    }
}

/// `inspector.get` for a relationship id or a connector id: the relationship, its type and its
/// properties tagged with the end they belong to. `None` when `id` is neither.
fn describe_relationship(s: &Session, id: &str) -> Option<Value> {
    let m = &s.doc.model;
    let connector = s.doc.connector(id);
    let rel = m.relationship(id).or_else(|| connector.and_then(|c| c.relationship.as_deref()).and_then(|r| m.relationship(r)));
    if rel.is_none() && connector.is_none() {
        return None;
    }
    let mut out =
        json!({"properties": [], "relationships": [], "views": [], "identity": {"tag": "", "format": "", "free": true}, "relationship": null});
    if let Some(c) = connector {
        out["connector"] = json!({"id": c.id, "kind": c.kind, "text": c.text, "startLabel": c.start_label, "endLabel": c.end_label});
    }
    let Some(r) = rel else { return Some(out) };
    let ty = cat().relationship_type(&r.type_id);
    let name_of = |e: &str| m.element(e).map(|x| x.name.clone()).unwrap_or_default();
    let connectors: Vec<&str> = s
        .doc
        .pages
        .iter()
        .flat_map(|p| p.connectors.iter())
        .filter(|c| c.relationship.as_deref() == Some(r.id.as_str()))
        .map(|c| c.id.as_str())
        .collect();
    out["relationship"] = json!({
        "id": r.id, "type": r.type_id, "name": r.name, "source": r.source, "target": r.target,
        "sourceName": name_of(&r.source), "targetName": name_of(&r.target), "connectors": connectors,
    });
    out["type"] = json!({"id": r.type_id, "label": ty.map(|(_, t)| t.label.clone()).unwrap_or_default(), "pack": r.pack()});
    let rt = ty.map(|(_, t)| t);
    let schema = rt.map(|t| t.properties.as_slice()).unwrap_or_default();
    let mut props = Vec::new();
    for sp in schema {
        props.push(json!({
            "key": sp.key, "label": if sp.label.is_empty() { &sp.key } else { &sp.label }, "kind": kind_name(sp.kind), "unit": sp.unit,
            "values": sp.values, "required": sp.required, "value": r.properties.get(&sp.key).map(prop_json), "source": "schema",
            "end": end_of(rt, &sp.key),
        }));
    }
    for (k, v) in &r.properties {
        if !schema.iter().any(|sp| sp.key == *k) {
            let kind = match v {
                vizcraft_model::PropValue::Bool(_) => "bool",
                vizcraft_model::PropValue::Number(_) => "number",
                _ => "text",
            };
            props.push(json!({"key": k, "label": k, "kind": kind, "unit": "", "values": [], "required": false, "value": prop_json(v), "source": "extra", "end": end_of(rt, k)}));
        }
    }
    out["properties"] = Value::Array(props);
    Some(out)
}

/// What `inspector.get` reports for `id`, without needing a mutable session (the pane reads it every frame).
pub fn describe(s: &Session, id: &str) -> CmdResult {
    if s.doc.model.element(id).is_none()
        && s.doc.pages.iter().all(|p| p.shape(id).is_none())
        && let Some(out) = describe_relationship(s, id)
    {
        return Ok(out);
    }
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

    fn classes() -> crate::Session {
        let mut s = crate::Session::new(vizcraft_diagram::Document::new());
        run(&mut s, "template.new", json!({"template": "uml.class"}));
        s
    }

    #[test]
    fn a_uml_association_reports_its_four_end_rows_and_navigability() {
        let mut s = classes();
        let c = s
            .doc
            .page()
            .unwrap()
            .connectors
            .iter()
            .find(|c| c.relationship.as_deref().and_then(|r| s.doc.model.relationship(r)).is_some_and(|r| r.type_id == "uml.Association"))
            .unwrap()
            .id
            .clone();
        let before = s.doc.clone();
        let r = run(&mut s, "inspector.get", json!({"id": c}));
        assert_eq!(s.doc, before, "read only");
        assert_eq!(r["relationship"]["type"], "uml.Association");
        assert_eq!(r["type"]["pack"], "uml");
        assert!(r["relationship"]["sourceName"].as_str().is_some_and(|n| !n.is_empty()));
        assert_eq!(r["relationship"]["connectors"], json!([c]));
        let props = r["properties"].as_array().unwrap();
        let end = |k: &str| props.iter().find(|p| p["key"] == k).unwrap_or_else(|| panic!("{k} in {props:?}"))["end"].clone();
        for (k, e) in [("sourceRole", "start"), ("sourceMultiplicity", "start"), ("targetRole", "end"), ("targetMultiplicity", "end")] {
            assert_eq!(end(k), e, "{k}");
        }
        assert!(props.iter().any(|p| p["key"] == "navigability" || p["key"].as_str().is_some_and(|k| k.contains("avigab"))), "{props:?}");
        // The relationship id gives the same relationship.
        let rid = r["relationship"]["id"].as_str().unwrap().to_string();
        assert_eq!(run(&mut s, "inspector.get", json!({"id": rid}))["relationship"]["id"], json!(rid));
    }

    #[test]
    fn an_er_relationship_tags_cardinalities_on_their_ends() {
        let mut s = crate::Session::new(vizcraft_diagram::Document::new());
        run(&mut s, "template.new", json!({"template": "er.schema"}));
        let c = s.doc.page().unwrap().connectors.iter().find(|c| c.relationship.is_some()).unwrap().id.clone();
        let r = run(&mut s, "inspector.get", json!({"id": c}));
        let props = r["properties"].as_array().unwrap();
        let end = |k: &str| props.iter().find(|p| p["key"] == k).unwrap_or_else(|| panic!("{k} in {props:?}"))["end"].clone();
        assert_eq!(end("sourceCardinality"), "start");
        assert_eq!(end("targetCardinality"), "end");
    }

    #[test]
    fn a_free_connector_gives_empty_lists_not_an_error() {
        let mut s = crate::Session::new(vizcraft_diagram::Document::new());
        let free = s.doc.page().unwrap().connectors.iter().find(|c| c.relationship.is_none()).map(|c| c.id.clone());
        let id = match free {
            Some(f) => f,
            None => s.run("connector.add", &json!({"from": {"x": 0, "y": 0}, "to": {"x": 90, "y": 0}})).unwrap()["id"].as_str().unwrap().to_string(),
        };
        let r = run(&mut s, "inspector.get", json!({"id": id}));
        assert!(r["relationship"].is_null());
        assert!(r["properties"].as_array().unwrap().is_empty() && r["relationships"].as_array().unwrap().is_empty());
        assert_eq!(r["connector"]["id"], json!(id));
    }
}
