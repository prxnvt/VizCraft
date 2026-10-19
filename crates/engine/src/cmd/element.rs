//! Element commands: create elements (optionally with a view), rename, set properties, retag through the identity registry, show in another view, delete from view.
//!
//! An element param (`id`) is an element id or the id of a shape that draws the element, so a
//! caller holding a shape from the canvas can act on its element directly.

use serde_json::{Value, json};
use vizcraft_model::{MAX_TAG, MAX_TEXT};

use super::model::{
    NewElement, auto_tag, cat, check_container, check_len, create_element, element_of, has_elements, id_param, key_value, page_param, place_shape,
    props_param, rename_element, req_element, set_prop, views_json,
};
use super::shape::{coord, size};
use super::{delete_items, ids_param, num, opt_str, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all element commands: `element.create`, `element.get`, `element.rename`, `element.setProperty`, `element.retag`, `element.show`, `element.deleteFromView`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("element.create", "New Element", "Model > Elements", create).params(
            r#"{"type": element type, "name"?: string, "tag"?: string, "owner"?: element id, "properties"?: {key: bool|number|string}, "id"?: string, "x"?: n, "y"?: n, "w"?: n, "h"?: n, "page"?: page} -> {"id", "tag", "shape"?}; x and y also draw it on page (default: the active one); no tag: the next auto number when the type's scope numbers"#,
        ),
        CommandSpec::new("element.get", "Element", "Model > Elements", get)
            .params(r#"{"id": element or shape id} -> {"element", "views": [{"page", "pageName", "items"}], "outgoing": [relationship], "incoming": [relationship]}"#)
            .when(has_elements),
        CommandSpec::new("element.rename", "Rename Element", "Model > Elements", rename)
            .params(r#"{"id": element or shape id, "name": string} -> {"id", "name"}; every shape drawing it shows the name"#)
            .when(has_elements),
        CommandSpec::new("element.setProperty", "Set Element Property", "Model > Elements", set_property)
            .params(r#"{"id": element or shape id, "key": string, "value": bool|number|string|null} -> {"id", "key", "value"}; null removes the property"#)
            .when(has_elements),
        CommandSpec::new("element.retag", "Retag Element", "Model > Elements", retag)
            .params(r#"{"id": element or shape id, "tag"?: string} -> {"id", "tag"}; no tag: the next auto number of the type's scope"#)
            .when(has_elements),
        CommandSpec::new("element.show", "Show Element", "Model > Elements", show)
            .params(r#"{"id": element or shape id, "page"?: page, "x": n, "y": n, "w"?: n, "h"?: n} -> {"shape"}; another view of the element; refused when that page already draws it"#)
            .when(has_elements),
        CommandSpec::new("element.deleteFromView", "Delete from View", "Model > Elements", delete_from_view)
            .params(r#"{"ids"?: [shape ids]} (default: the selected shapes that draw elements) -> {"deleted", "detached"}; the elements stay in the model"#)
            .when(has_bound_shapes),
    ]
}

/// Enabled when the active page has a shape that draws an element.
fn has_bound_shapes(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(|p| p.shapes.iter().any(|sh| sh.element.is_some())) { None } else { Some("no shape on this page draws an element") }
}

/// The optional `tag` param: at most `MAX_TAG` bytes.
fn tag_param(v: &Value) -> Result<Option<String>, EngineError> {
    match opt_str(v, "tag")? {
        None => Ok(None),
        Some(t) if t.len() > MAX_TAG => Err(EngineError::Params(format!("`tag` is longer than {MAX_TAG} bytes"))),
        Some(t) => Ok(Some(t.to_string())),
    }
}

/// `x`, `y`, `w`, `h` for a new view: `None` when neither x nor y is given; one without the other
/// is a params error.
fn frame_param(v: &Value) -> Result<Option<(f64, f64, Option<f64>, Option<f64>)>, EngineError> {
    let (x, y) = (num(v, "x")?, num(v, "y")?);
    let (w, h) = (num(v, "w")?.map(size), num(v, "h")?.map(size));
    match (x, y) {
        (Some(x), Some(y)) => Ok(Some((coord(x), coord(y), w, h))),
        (None, None) => Ok(None),
        _ => Err(EngineError::Params("give both `x` and `y` to draw the element, or neither".into())),
    }
}

/// Select `shape` when it is on the active page (the selection only holds active-page items).
fn select_if_active(s: &mut Session, page: usize, shape: &str) {
    if page == s.doc.active_index() {
        select(s, vec![shape.to_string()]);
    }
}

fn create(s: &mut Session, v: &Value) -> CmdResult {
    let type_id = id_param(v, "type")?;
    if cat().element_type(type_id).is_none() {
        return Err(EngineError::Params(format!("unknown element type `{}` (see notation.list)", short(type_id))));
    }
    let name = opt_str(v, "name")?.map(|n| super::model::cut(n, MAX_TEXT)).unwrap_or_default();
    let tag = tag_param(v)?;
    let owner = match opt_str(v, "owner")? {
        None => None,
        Some(o) => {
            let o = check_len("owner", o)?;
            let o = element_of(&s.doc, o).ok_or_else(|| EngineError::Params(format!("`owner`: `{}` is not an element", short(o))))?;
            check_container(&s.doc, &o)?;
            Some(o)
        }
    };
    let properties = props_param(v)?;
    let frame = frame_param(v)?;
    let page = page_param(&s.doc, v)?;
    if frame.is_none() && v.get("page").is_some_and(|x| !x.is_null()) {
        return Err(EngineError::Params("`page` needs `x` and `y`".into()));
    }
    let id = super::new_id(s, v, "e")?;
    let ne = NewElement { type_id: type_id.to_string(), name, tag, owner, properties, id: Some(id) };
    let (id, tag) = create_element(&mut s.doc, ne)?;
    let Some((x, y, w, h)) = frame else {
        return Ok(json!({"id": id, "tag": tag}));
    };
    let shape = place_shape(&mut s.doc, &id, page, x, y, w, h)?;
    select_if_active(s, page, &shape);
    with_sel(s, json!({"id": id, "tag": tag, "shape": shape}))
}

fn get(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_element(s, v, "id")?;
    let m = &s.doc.model;
    let e = m.element(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(&id))))?;
    let to = |x: Vec<&vizcraft_model::Relationship>| serde_json::to_value(x).map_err(|e| EngineError::Failed(e.to_string()));
    Ok(json!({
        "element": serde_json::to_value(e).map_err(|e| EngineError::Failed(e.to_string()))?,
        "views": views_json(&s.doc, &id),
        "outgoing": to(m.outgoing(&id).collect())?,
        "incoming": to(m.incoming(&id).collect())?,
    }))
}

fn rename(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_element(s, v, "id")?;
    let name = p::req_str(v, "name")?;
    rename_element(&mut s.doc, &id, name)?;
    let name = s.doc.model.element(&id).map(|e| e.name.clone()).unwrap_or_default();
    Ok(json!({"id": id, "name": name}))
}

fn set_property(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_element(s, v, "id")?;
    let (key, value) = key_value(v)?;
    let e = s.doc.model.element_mut(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(&id))))?;
    set_prop(&mut e.properties, key.clone(), value.clone())?;
    Ok(json!({"id": id, "key": key, "value": value}))
}

fn retag(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_element(s, v, "id")?;
    let tag = match tag_param(v)? {
        Some(t) => t,
        None => {
            let e = s.doc.model.element(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(&id))))?;
            auto_tag(&s.doc, e)?
                .ok_or_else(|| EngineError::Params(format!("type `{}` is not numbered automatically; give `tag`", short(&e.type_id))))?
        }
    };
    let e = s.doc.model.element_mut(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(&id))))?;
    e.tag = tag.clone();
    Ok(json!({"id": id, "tag": tag}))
}

fn show(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_element(s, v, "id")?;
    let page = page_param(&s.doc, v)?;
    let (x, y, w, h) = frame_param(v)?.ok_or_else(|| EngineError::Params("`x` and `y` (numbers) are required".into()))?;
    let drawn = s.doc.pages.get(page).is_some_and(|p| p.shapes.iter().any(|sh| sh.element.as_deref() == Some(id.as_str())));
    if drawn {
        return Err(EngineError::Params(format!("`{}` is already drawn on that page", short(&id))));
    }
    let shape = place_shape(&mut s.doc, &id, page, x, y, w, h)?;
    select_if_active(s, page, &shape);
    with_sel(s, json!({"id": id, "shape": shape}))
}

fn delete_from_view(s: &mut Session, v: &Value) -> CmdResult {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let bound = |id: &str| page.shape(id).is_some_and(|sh| sh.element.is_some());
    let mut ids: Vec<String> = Vec::new();
    match ids_param(v)? {
        Some(given) => {
            for id in given {
                if !bound(&id) {
                    return Err(EngineError::Params(format!("`{}` is not a shape of an element on this page", short(&id))));
                }
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        None => {
            for id in &s.sel.items {
                if bound(id) && !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
        }
    }
    if ids.is_empty() {
        return Err(EngineError::Params("select a shape of an element first (or pass `ids`)".into()));
    }
    delete_items(s, &ids)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::model::fixture::{blank, flow, refused, run, shapes_of};

    #[test]
    fn create_stores_type_name_properties_tag_and_pack() {
        let mut s = blank();
        let r = run(&mut s, "element.create", json!({"type": "flowchart.Process", "name": "Mix", "properties": {"owner": "ops", "weight": 2.5}}));
        assert_eq!((r["tag"].as_str(), r.get("shape")), (Some("S1"), None));
        let id = r["id"].as_str().unwrap().to_string();
        let e = s.doc.model.element(&id).unwrap().clone();
        assert_eq!((e.type_id.as_str(), e.name.as_str(), e.tag.as_str()), ("flowchart.Process", "Mix", "S1"));
        assert_eq!(e.properties.get("owner").and_then(|v| v.as_str()), Some("ops"));
        assert_eq!(e.properties.get("weight").and_then(|v| v.as_f64()), Some(2.5));
        assert_eq!(e.properties.get("automated").and_then(|v| v.as_bool()), Some(false), "default filled");
        assert!(s.doc.model.uses_pack("flowchart"));
        assert!(s.doc.pages[0].shapes.is_empty(), "no x and y: no view");
        assert_eq!(s.undo_label(), Some("New Element"));
        // Terminators are not numbered; a given tag is stored as is, even when taken.
        let r = run(&mut s, "element.create", json!({"type": "flowchart.Terminator"}));
        assert_eq!(r["tag"], "");
        let r = run(&mut s, "element.create", json!({"type": "flowchart.Decision", "tag": "S1"}));
        assert_eq!(r["tag"], "S1");
        let r = run(&mut s, "element.create", json!({"type": "flowchart.Decision"}));
        assert_eq!(r["tag"], "S2");
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model.elements.len(), 3);
    }

    #[test]
    fn create_with_a_frame_draws_and_selects_a_view() {
        let mut s = blank();
        let r = run(&mut s, "element.create", json!({"type": "flowchart.Decision", "name": "Ok?", "id": "q", "x": 10, "y": 20, "w": 100}));
        assert_eq!(r["id"], "q");
        let sid = r["shape"].as_str().unwrap();
        let sh = s.doc.shape(sid).unwrap();
        assert_eq!((sh.element.as_deref(), sh.text.as_str(), sh.stencil.as_str()), (Some("q"), "Ok?", "flowchart.decision"));
        assert_eq!((sh.x, sh.y, sh.w), (10.0, 20.0, 100.0));
        assert_eq!(s.sel.items, vec![sid.to_string()]);
        // On another page: drawn there, selection untouched.
        s.doc.add_page("Two");
        let r = run(&mut s, "element.create", json!({"type": "flowchart.Data", "x": 0, "y": 0, "page": "Two"}));
        assert!(s.doc.pages[1].shape(r["shape"].as_str().unwrap()).is_some());
        assert_eq!(s.sel.items, vec![sid.to_string()]);
        run(&mut s, "edit.undo", json!({}));
        run(&mut s, "edit.undo", json!({}));
        assert!(s.doc.model.elements.is_empty() && s.doc.pages[0].shapes.is_empty());
    }

    #[test]
    fn create_refuses_bad_params() {
        let mut s = flow();
        for bad in [
            json!({}),
            json!({"type": "flowchart.Nope"}),
            json!({"type": "basic.rectangle"}),
            json!({"type": "flowchart.Process", "properties": {"k": [1]}}),
            json!({"type": "flowchart.Process", "properties": {"k": null}}),
            json!({"type": "flowchart.Process", "properties": {"": 1}}),
            json!({"type": "flowchart.Process", "properties": [1]}),
            json!({"type": "flowchart.Process", "x": 5}),
            json!({"type": "flowchart.Process", "page": "p1"}),
            json!({"type": "flowchart.Process", "x": 0, "y": 0, "page": "nowhere"}),
            json!({"type": "flowchart.Process", "owner": "do"}),
            json!({"type": "flowchart.Process", "owner": "nope"}),
            json!({"type": "flowchart.Process", "id": "do"}),
            json!({"type": "flowchart.Process", "id": "p1"}),
            json!({"type": "flowchart.Process", "id": "a b"}),
            json!({"type": "flowchart.Process", "tag": "t".repeat(300)}),
            json!({"type": "flowchart.Process", "name": 4}),
        ] {
            refused(&mut s, "element.create", bad);
        }
    }

    #[test]
    fn get_rename_and_set_property() {
        let mut s = flow();
        let shape = shapes_of(&s, "do")[0].clone();
        let before = s.doc.clone();
        let r = run(&mut s, "element.get", json!({"id": shape}));
        assert_eq!(s.doc, before, "read only");
        assert_eq!(r["element"]["id"], "do");
        assert_eq!(r["views"][0]["items"], json!([shape]));
        assert_eq!((r["incoming"][0]["id"].as_str(), r["outgoing"][0]["id"].as_str()), (Some("f1"), Some("f2")));
        refused(&mut s, "element.get", json!({"id": "f1"}));

        run(&mut s, "element.rename", json!({"id": "do", "name": "Blend"}));
        assert_eq!(s.doc.model.element("do").unwrap().name, "Blend");
        assert_eq!(s.doc.shape(&shape).unwrap().text, "Blend");
        refused(&mut s, "element.rename", json!({"id": "do"}));
        refused(&mut s, "element.rename", json!({"id": "nope", "name": "x"}));

        run(&mut s, "element.setProperty", json!({"id": "do", "key": "owner", "value": "me"}));
        assert_eq!(s.doc.model.element("do").unwrap().prop("owner").and_then(|v| v.as_str()), Some("me"));
        run(&mut s, "element.setProperty", json!({"id": "do", "key": "owner", "value": null}));
        assert!(s.doc.model.element("do").unwrap().prop("owner").is_none());
        refused(&mut s, "element.setProperty", json!({"id": "do", "key": "owner"}));
        refused(&mut s, "element.setProperty", json!({"id": "do", "key": "owner", "value": {}}));
        refused(&mut s, "element.setProperty", json!({"id": "do", "key": "", "value": 1}));
        run(&mut s, "edit.undo", json!({}));
        run(&mut s, "edit.undo", json!({}));
        assert!(s.doc.model.element("do").unwrap().prop("owner").is_none());
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.shape(&shape).unwrap().text, "Do");
    }

    #[test]
    fn retag_numbers_or_stores_the_given_tag() {
        let mut s = flow();
        // A duplicate tag is stored; reporting it is the rules engine's job.
        assert_eq!(run(&mut s, "element.retag", json!({"id": "ok", "tag": "S1"}))["tag"], "S1");
        assert_eq!(s.doc.model.element("ok").unwrap().tag, "S1");
        assert_eq!(run(&mut s, "element.retag", json!({"id": "ok"}))["tag"], "S2");
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model.element("ok").unwrap().tag, "S1");
        let e = refused(&mut s, "element.retag", json!({"id": "start"}));
        assert!(e.contains("not numbered"), "{e}");
        refused(&mut s, "element.retag", json!({"id": "ok", "tag": 3}));
        refused(&mut s, "element.retag", json!({"id": "nope"}));
    }

    #[test]
    fn show_adds_one_view_per_page() {
        let mut s = flow();
        let e = refused(&mut s, "element.show", json!({"id": "do", "x": 0, "y": 0}));
        assert!(e.contains("already drawn"), "{e}");
        refused(&mut s, "element.show", json!({"id": "do", "page": "nowhere", "x": 0, "y": 0}));
        refused(&mut s, "element.show", json!({"id": "do"}));
        let id = run(&mut s, "element.create", json!({"type": "flowchart.Data", "name": "Log"}))["id"].as_str().unwrap().to_string();
        let r = run(&mut s, "element.show", json!({"id": id, "x": 400, "y": 0}));
        let sh = s.doc.shape(r["shape"].as_str().unwrap()).unwrap();
        assert_eq!((sh.element.as_deref(), sh.text.as_str()), (Some(id.as_str()), "Log"));
        assert_eq!(s.sel.items, vec![r["shape"].as_str().unwrap().to_string()]);
        s.doc.add_page("Two");
        run(&mut s, "element.show", json!({"id": "do", "page": "Two", "x": 0, "y": 0}));
        assert_eq!(shapes_of(&s, "do").len(), 2);
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(shapes_of(&s, "do").len(), 1);
    }

    #[test]
    fn delete_from_view_keeps_the_element() {
        let mut s = flow();
        assert!(blank().run("element.deleteFromView", &json!({})).is_err(), "disabled without bound shapes");
        let free = run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}))["id"].as_str().unwrap().to_string();
        refused(&mut s, "element.deleteFromView", json!({"ids": [free]}));
        refused(&mut s, "element.deleteFromView", json!({}));
        let shape = shapes_of(&s, "ok")[0].clone();
        run(&mut s, "edit.select", json!({"ids": [shape, free]}));
        let r = run(&mut s, "element.deleteFromView", json!({}));
        assert_eq!(r["deleted"], json!([shape]));
        assert!(s.doc.shape(&free).is_some());
        assert!(s.doc.model.element("ok").is_some());
        assert!(shapes_of(&s, "ok").is_empty());
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(shapes_of(&s, "ok"), vec![shape]);
    }
}
