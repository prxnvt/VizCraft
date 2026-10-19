//! Relationship commands: create typed links between elements (with a connector when both ends are drawn on one page), retype, reconnect, set properties.
//!
//! Ends (`from`, `to`) are element ids or ids of shapes that draw elements. A relationship that
//! breaks the pack's connection table is still created unless `strict` asks otherwise, so the
//! rules engine can report it; the result says whether it is legal.

use serde_json::{Value, json};
use vizcraft_diagram::{Arrow, Connector, Document, End, Route};
use vizcraft_model::{MAX_TEXT, Relationship};
use vizcraft_notation::legal;

use super::model::{
    cat, cut, fill_defaults, has_elements, has_relationships, id_param, key_value, model_err, props_param, req_element, req_relationship, set_prop,
};
use super::{opt_bool, opt_str, prune_selection, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return all relationship commands: `relationship.create`, `relationship.retype`, `relationship.reconnect`, `relationship.setProperty`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("relationship.create", "New Relationship", "Model > Relationships", create)
            .params(
                r#"{"from": element or shape id, "to": element or shape id, "type"?: relationship type, "name"?: string, "properties"?: {key: bool|number|string}, "id"?: string, "strict"?: bool} -> {"id", "type", "legal", "connector"?}; no type: the pack's type for the pair; strict refuses an illegal one; a connector is drawn when both ends are on one page (the active one first)"#,
            )
            .when(has_elements),
        CommandSpec::new("relationship.retype", "Change Relationship Type", "Model > Relationships", retype)
            .params(r#"{"id": relationship or connector id, "type": relationship type} -> {"id", "type", "legal"}"#)
            .when(has_relationships),
        CommandSpec::new("relationship.reconnect", "Reconnect Relationship", "Model > Relationships", reconnect)
            .params(
                r#"{"id": relationship or connector id, "from"?: element or shape id, "to"?: element or shape id} -> {"id", "source", "target", "legal", "removed": [connector ids]}; each connector view is reglued to a shape of the new end on its page, or removed when that page has none"#,
            )
            .when(has_relationships),
        CommandSpec::new("relationship.setProperty", "Set Relationship Property", "Model > Relationships", set_property)
            .params(r#"{"id": relationship or connector id, "key": string, "value": bool|number|string|null} -> {"id", "key", "value"}; null removes the property"#)
            .when(has_relationships),
    ]
}

fn type_of(doc: &Document, element: &str) -> String {
    doc.model.element(element).map(|e| e.type_id.clone()).unwrap_or_default()
}

/// The `type` param when given: a relationship type some loaded pack defines.
fn rel_type_param(v: &Value) -> Result<Option<String>, EngineError> {
    match opt_str(v, "type")? {
        None => Ok(None),
        Some(t) => {
            let t = super::model::check_len("type", t)?;
            if cat().relationship_type(t).is_none() {
                return Err(EngineError::Params(format!("unknown relationship type `{}` (see notation.describe)", short(t))));
            }
            Ok(Some(t.to_string()))
        }
    }
}

/// The shape on page `page` to draw element `element` with: `hint` when it is a shape of that
/// element on the page, else the first shape drawing it there.
fn shape_on(doc: &Document, page: usize, element: &str, hint: Option<&str>) -> Option<String> {
    let p = doc.pages.get(page)?;
    if let Some(h) = hint
        && p.shape(h).is_some_and(|sh| sh.element.as_deref() == Some(element))
    {
        return Some(h.to_string());
    }
    p.shapes.iter().find(|sh| sh.element.as_deref() == Some(element)).map(|sh| sh.id.clone())
}

/// The end arrow a relationship type's connectors are drawn with.
fn end_arrow(type_id: &str) -> Option<Arrow> {
    cat().relationship_type(type_id).and_then(|(_, r)| Arrow::from_name(&r.end_arrow))
}

fn create(s: &mut Session, v: &Value) -> CmdResult {
    let from_param = id_param(v, "from")?.to_string();
    let to_param = id_param(v, "to")?.to_string();
    let source = req_element(s, v, "from")?;
    let target = req_element(s, v, "to")?;
    let (st, tt) = (type_of(&s.doc, &source), type_of(&s.doc, &target));
    let (type_id, is_legal) = match rel_type_param(v)? {
        Some(t) => {
            let ok = legal::is_legal(cat(), &st, &t, &tt);
            (t, ok)
        }
        None => legal::relationship_for(cat(), &st, &tt)
            .ok_or_else(|| EngineError::Params(format!("no relationship type joins a {} to a {}; give `type`", short(&st), short(&tt))))?,
    };
    if opt_bool(v, "strict")?.unwrap_or(false) && !is_legal {
        return Err(EngineError::Params(format!("a {type_id} from a {} to a {} is not allowed by its pack", short(&st), short(&tt))));
    }
    let name = opt_str(v, "name")?.map(|n| cut(n, MAX_TEXT)).unwrap_or_default();
    let mut properties = props_param(v)?;
    if let Some((_, rt)) = cat().relationship_type(&type_id) {
        fill_defaults(&mut properties, &rt.properties);
    }
    let id = super::new_id(s, v, "r")?;
    let mut r = Relationship::new(id.clone(), type_id.clone(), source.clone(), target.clone());
    r.name = name.clone();
    r.properties = properties;
    let pack = cat().pack_ref(r.pack());
    s.doc.model.add_relationship(r).map_err(model_err)?;
    if let Some(p) = pack {
        s.doc.model.use_pack(p);
    }

    // A connector on the first page that draws both ends (the active page is the first).
    let found = (0..s.doc.pages.len()).find_map(|i| {
        let a = shape_on(&s.doc, i, &source, Some(&from_param))?;
        let b = shape_on(&s.doc, i, &target, Some(&to_param))?;
        Some((i, a, b))
    });
    let Some((page, a, b)) = found else {
        return Ok(json!({"id": id, "type": type_id, "legal": is_legal}));
    };
    let cid = s.doc.next_id("c");
    let mut c = Connector::new(cid.clone(), End::dynamic(a), End::dynamic(b));
    c.route = Route::Orthogonal;
    if let Some(arrow) = end_arrow(&type_id) {
        c.end_arrow = arrow;
    }
    // The connector type that draws the relationship (UML ends, dashes) under the page's
    // variant, when the pack names one.
    let variant = s.doc.pages.get(page).and_then(|p| p.view.as_ref()).map(|v| v.variant.clone()).unwrap_or_default();
    if let Some(k) = vizcraft_stencils::connector_kind_for_relationship(&type_id, &variant) {
        k.apply(&mut c);
    }
    c.text = name;
    c.relationship = Some(id.clone());
    c.sanitize();
    let pg = s.doc.pages.get_mut(page).ok_or_else(|| EngineError::Failed("the page went missing".into()))?;
    pg.connectors.push(c);
    if page == 0 {
        select(s, vec![cid.clone()]);
    }
    with_sel(s, json!({"id": id, "type": type_id, "legal": is_legal, "connector": cid}))
}

fn retype(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_relationship(s, v, "id")?;
    let type_id = rel_type_param(v)?.ok_or_else(|| EngineError::Params("`type` (relationship type) is required".into()))?;
    let r = s.doc.model.relationship_mut(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not a relationship", short(&id))))?;
    r.type_id = type_id.clone();
    let (source, target) = (r.source.clone(), r.target.clone());
    if let Some(p) = cat().pack_ref(vizcraft_model::pack_of(&type_id)) {
        s.doc.model.use_pack(p);
    }
    if let Some(arrow) = end_arrow(&type_id) {
        for c in s.doc.pages.iter_mut().flat_map(|p| p.connectors.iter_mut()).filter(|c| c.relationship.as_deref() == Some(id.as_str())) {
            c.end_arrow = arrow;
        }
    }
    let ok = legal::is_legal(cat(), &type_of(&s.doc, &source), &type_id, &type_of(&s.doc, &target));
    Ok(json!({"id": id, "type": type_id, "legal": ok}))
}

fn reconnect(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_relationship(s, v, "id")?;
    let from = match opt_str(v, "from")? {
        Some(_) => Some((req_element(s, v, "from")?, id_param(v, "from")?.to_string())),
        None => None,
    };
    let to = match opt_str(v, "to")? {
        Some(_) => Some((req_element(s, v, "to")?, id_param(v, "to")?.to_string())),
        None => None,
    };
    if from.is_none() && to.is_none() {
        return Err(EngineError::Params("give `from`, `to` or both".into()));
    }
    let r = s.doc.model.relationship_mut(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not a relationship", short(&id))))?;
    if let Some((e, _)) = &from {
        r.source = e.clone();
    }
    if let Some((e, _)) = &to {
        r.target = e.clone();
    }
    // Ports belong to the old end's type; they no longer mean anything.
    if from.is_some() {
        r.source_port = None;
    }
    if to.is_some() {
        r.target_port = None;
    }
    let (source, target, type_id) = (r.source.clone(), r.target.clone(), r.type_id.clone());

    // Reglue each connector view on its own page, or drop it when that page cannot draw the end.
    let mut removed = Vec::new();
    for page in 0..s.doc.pages.len() {
        let views: Vec<String> = s
            .doc
            .pages
            .get(page)
            .map(|p| p.connectors.iter().filter(|c| c.relationship.as_deref() == Some(id.as_str())).map(|c| c.id.clone()).collect())
            .unwrap_or_default();
        if views.is_empty() {
            continue;
        }
        let new_start = from.as_ref().map(|(e, hint)| shape_on(&s.doc, page, e, Some(hint)));
        let new_end = to.as_ref().map(|(e, hint)| shape_on(&s.doc, page, e, Some(hint)));
        let Some(pg) = s.doc.pages.get_mut(page) else { continue };
        for cid in views {
            if matches!(new_start, Some(None)) || matches!(new_end, Some(None)) {
                pg.remove_connector(&cid);
                removed.push(cid);
                continue;
            }
            if let Some(c) = pg.connector_mut(&cid) {
                if let Some(Some(sh)) = &new_start {
                    c.start = End::dynamic(sh.clone());
                }
                if let Some(Some(sh)) = &new_end {
                    c.end = End::dynamic(sh.clone());
                }
                c.sanitize();
            }
        }
    }
    prune_selection(s);
    let ok = legal::is_legal(cat(), &type_of(&s.doc, &source), &type_id, &type_of(&s.doc, &target));
    with_sel(s, json!({"id": id, "source": source, "target": target, "legal": ok, "removed": removed}))
}

fn set_property(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_relationship(s, v, "id")?;
    let (key, value) = key_value(v)?;
    let r = s.doc.model.relationship_mut(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not a relationship", short(&id))))?;
    set_prop(&mut r.properties, key.clone(), value.clone())?;
    Ok(json!({"id": id, "key": key, "value": value}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::{Arrow, End, Route};

    use super::super::model::fixture::{blank, flow, refused, run, shapes_of};

    #[test]
    fn a_flowchart_built_from_elements_has_glued_bound_connectors() {
        let s = flow();
        let m = &s.doc.model;
        assert_eq!(m.relationships.len(), 2);
        let f2 = m.relationship("f2").unwrap();
        assert_eq!((f2.type_id.as_str(), f2.source.as_str(), f2.target.as_str(), f2.name.as_str()), ("flowchart.Flow", "do", "ok", "next"));
        let page = &s.doc.pages[0];
        assert_eq!(page.connectors.len(), 2);
        for c in &page.connectors {
            let r = m.relationship(c.relationship.as_deref().unwrap()).unwrap();
            assert_eq!(c.start, End::dynamic(shapes_of(&s, &r.source)[0].clone()));
            assert_eq!(c.end, End::dynamic(shapes_of(&s, &r.target)[0].clone()));
            assert_eq!((c.route, c.end_arrow), (Route::Orthogonal, Arrow::Triangle));
            assert_eq!(c.text, r.name);
        }
    }

    #[test]
    fn create_reports_legality_and_strict_refuses() {
        let mut s = blank();
        run(&mut s, "element.create", json!({"type": "flowchart.Database", "id": "d1", "x": 0, "y": 0}));
        run(&mut s, "element.create", json!({"type": "flowchart.Database", "id": "d2", "x": 200, "y": 0}));
        let e = refused(&mut s, "relationship.create", json!({"from": "d1", "to": "d2", "strict": true}));
        assert!(e.contains("not allowed"), "{e}");
        let shape = shapes_of(&s, "d2")[0].clone();
        let r = run(&mut s, "relationship.create", json!({"from": "d1", "to": shape, "properties": {"condition": "sync"}}));
        assert_eq!((r["type"].as_str(), r["legal"].as_bool()), (Some("flowchart.Flow"), Some(false)));
        let id = r["id"].as_str().unwrap();
        assert_eq!(s.doc.model.relationship(id).unwrap().prop("condition").and_then(|v| v.as_str()), Some("sync"));
        let c = s.doc.connector(r["connector"].as_str().unwrap()).unwrap();
        assert_eq!(c.relationship.as_deref(), Some(id));
        assert_eq!(s.sel.items, vec![c.id.clone()]);
        run(&mut s, "edit.undo", json!({}));
        assert!(s.doc.model.relationships.is_empty() && s.doc.pages[0].connectors.is_empty());
    }

    #[test]
    fn create_without_a_shared_page_records_the_relationship_only() {
        let mut s = blank();
        run(&mut s, "element.create", json!({"type": "flowchart.Process", "id": "a", "x": 0, "y": 0}));
        s.doc.add_page("Two");
        run(&mut s, "element.create", json!({"type": "flowchart.Process", "id": "b", "x": 0, "y": 0, "page": "Two"}));
        let r = run(&mut s, "relationship.create", json!({"from": "a", "to": "b", "type": "flowchart.Flow", "name": "go"}));
        assert_eq!((r["legal"].as_bool(), r.get("connector")), (Some(true), None));
        assert!(s.doc.pages.iter().all(|p| p.connectors.is_empty()));
        // Drawn on the second page too: the connector goes there.
        run(&mut s, "element.show", json!({"id": "a", "page": "Two", "x": 300, "y": 0}));
        let r = run(&mut s, "relationship.create", json!({"from": "a", "to": "b"}));
        assert!(s.doc.pages[1].connector(r["connector"].as_str().unwrap()).is_some());
        for bad in [
            json!({"from": "a"}),
            json!({"from": "a", "to": "nope"}),
            json!({"from": "a", "to": "b", "type": "flowchart.Nope"}),
            json!({"from": "a", "to": "b", "id": "a"}),
            json!({"from": "a", "to": "b", "strict": "yes"}),
            json!({"from": "a", "to": "b", "properties": {"x": []}}),
        ] {
            refused(&mut s, "relationship.create", bad);
        }
    }

    #[test]
    fn retype_and_set_property() {
        let mut s = flow();
        let r = run(&mut s, "relationship.retype", json!({"id": "f1", "type": "flowchart.Flow"}));
        assert_eq!(r["legal"], true);
        refused(&mut s, "relationship.retype", json!({"id": "f1", "type": "flowchart.Nope"}));
        refused(&mut s, "relationship.retype", json!({"id": "f1"}));
        refused(&mut s, "relationship.retype", json!({"id": "do", "type": "flowchart.Flow"}));
        let c = s.doc.pages[0].connectors[0].id.clone();
        run(&mut s, "relationship.setProperty", json!({"id": c, "key": "condition", "value": "yes"}));
        assert_eq!(s.doc.model.relationship("f1").unwrap().prop("condition").and_then(|v| v.as_str()), Some("yes"));
        refused(&mut s, "relationship.setProperty", json!({"id": "f1", "key": "condition", "value": [1]}));
        run(&mut s, "edit.undo", json!({}));
        assert!(s.doc.model.relationship("f1").unwrap().prop("condition").is_none());
    }

    #[test]
    fn reconnect_reglues_views_or_removes_them() {
        let mut s = flow();
        run(&mut s, "element.create", json!({"type": "flowchart.Process", "id": "other", "x": 300, "y": 150}));
        let r = run(&mut s, "relationship.reconnect", json!({"id": "f2", "to": "other"}));
        assert_eq!((r["source"].as_str(), r["target"].as_str(), r["removed"].clone()), (Some("do"), Some("other"), json!([])));
        let c = s.doc.pages[0].connectors.iter().find(|c| c.relationship.as_deref() == Some("f2")).unwrap();
        assert_eq!(c.end, End::dynamic(shapes_of(&s, "other")[0].clone()));
        // An end with no shape on the page takes the connector view off it; the relationship stays.
        run(&mut s, "element.create", json!({"type": "flowchart.Process", "id": "hidden"}));
        let r = run(&mut s, "relationship.reconnect", json!({"id": "f2", "from": "hidden"}));
        assert_eq!(r["removed"].as_array().unwrap().len(), 1);
        assert_eq!(s.doc.model.relationship("f2").unwrap().source, "hidden");
        assert!(shapes_of(&s, "f2").is_empty());
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(shapes_of(&s, "f2").len(), 1);
        refused(&mut s, "relationship.reconnect", json!({"id": "f2"}));
        refused(&mut s, "relationship.reconnect", json!({"id": "f2", "to": "nope"}));
        refused(&mut s, "relationship.reconnect", json!({"id": "nope", "to": "do"}));
    }
}
