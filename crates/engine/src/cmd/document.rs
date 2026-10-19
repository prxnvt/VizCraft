//! Document commands for the Info page: file properties, and finding and removing hidden data.
//!
//! "Hidden data" is what a document carries that its drawing does not show: review comments,
//! hidden shapes and layers, screen tips, shape data rows, pictures and file properties. Inspect
//! counts each kind; remove deletes the kinds asked for in one undo step.

use serde_json::{Map, Value, json};
use vizcraft_diagram::DocProperties;

use super::{delete_on_page, drop_undrawn_relationships, opt_str, prune_selection, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The kinds of hidden data, in the order they are listed.
pub const HIDDEN_KINDS: [&str; 7] = ["comments", "hiddenShapes", "hiddenLayers", "screenTips", "dataRows", "images", "properties"];

/// Return the document commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("doc.setProperties", "Document Properties", "File > Info", set_properties).params(
            r#"{"title"?: string, "subject"?: string, "author"?: string, "keywords"?: string, "description"?: string} -> {"properties": {...}}; sets the given properties, "" clears one"#,
        ),
        CommandSpec::new("doc.setProperty", "Document Property", "File > Info", set_property).params(
            r#"{"key": a document property a loaded pack declares (see doc.properties), "value": string|number|bool|null} -> {"key", "value", "issue"?}; null or "" clears it; a value over the field's length is kept and reported"#,
        ),
        CommandSpec::new("doc.properties", "Document Properties List", "File > Info", list_properties).params(
            r#"{} -> {"properties": [{"key", "label", "group", "required", "maxChars", "kind", "values", "value", "issue"?, "pack"}]}; the title block fields every loaded pack declares; read-only"#,
        ),
        CommandSpec::new("doc.inspectHidden", "Inspect Document", "File > Info", inspect)
            .params(r#"{} -> {"counts": {"comments", "hiddenShapes", "hiddenLayers", "screenTips", "dataRows", "images", "properties"}, "total": n}; read-only"#),
        CommandSpec::new("doc.removeHidden", "Remove Hidden Data", "File > Info", remove).params(
            r#"{"what": ["comments"|"hiddenShapes"|"hiddenLayers"|"screenTips"|"dataRows"|"images"|"properties"] or "all"} -> {"removed": {kind: n}}; one undo step; a hidden layer is removed with the shapes that are only on hidden layers"#,
        ),
    ]
}

/// Set the document's title. The title property (`properties.title`, the ISO 7200 title field
/// the title block, the exports and `doc.properties` read) is the one title; the plain
/// `Document.title` is kept equal to it so the window title and older readers agree.
/// `edit.setTitle`, `doc.setProperties` and `doc.setProperty {key: "title"}` all end here.
pub fn write_title(doc: &mut vizcraft_diagram::Document, title: &str) {
    doc.properties.title = title.to_string();
    doc.properties.sanitize();
    doc.title = doc.properties.title.clone();
}

fn properties_json(p: &DocProperties) -> Value {
    json!({"title": p.title, "subject": p.subject, "author": p.author, "keywords": p.keywords, "description": p.description})
}

fn set_properties(s: &mut Session, v: &Value) -> CmdResult {
    let keys = ["title", "subject", "author", "keywords", "description"];
    let mut given = Vec::new();
    for k in keys {
        if let Some(t) = opt_str(v, k)? {
            given.push((k, t.to_string()));
        }
    }
    if given.is_empty() {
        return Err(EngineError::Params("give at least one of title, subject, author, keywords, description".into()));
    }
    if let Some((_, t)) = given.iter().find(|(k, _)| *k == "title") {
        let t = t.clone();
        write_title(&mut s.doc, &t);
    }
    let p = &mut s.doc.properties;
    for (k, t) in given {
        match k {
            "title" => {}
            "subject" => p.subject = t,
            "author" => p.author = t,
            "keywords" => p.keywords = t,
            _ => p.description = t,
        }
    }
    p.sanitize();
    Ok(json!({"properties": properties_json(&s.doc.properties)}))
}

/// The declaration of document property `key` in a loaded pack, with the pack's id.
fn declared(key: &str) -> Option<(&'static str, &'static vizcraft_notation::PropertySchema)> {
    vizcraft_notation::catalog().packs().iter().find_map(|p| p.document_properties.iter().find(|d| d.key == key).map(|d| (p.id.as_str(), d)))
}

/// What is wrong with a stored value, if anything: too long, or not one of an enum's values.
fn value_issue(d: &vizcraft_notation::PropertySchema, value: &str) -> Option<String> {
    let n = value.chars().count();
    if let Some(max) = d.max_chars.filter(|m| n > *m as usize) {
        return Some(format!("{n} characters; at most {max} fit"));
    }
    if d.kind == vizcraft_notation::PropKind::Enum && !value.is_empty() && !d.values.iter().any(|x| x == value) {
        return Some(format!("not one of {}", d.values.join(", ")));
    }
    None
}

fn set_property(s: &mut Session, v: &Value) -> CmdResult {
    use vizcraft_notation::PropKind;
    let key = crate::p::req_str(v, "key")?;
    let (_, d) = declared(key)
        .ok_or_else(|| EngineError::Params(format!("no loaded pack declares the document property `{}`; see doc.properties", short(key))))?;
    let value = match v.get("value") {
        None => return Err(EngineError::Params("`value` is required (null clears it)".into())),
        Some(Value::Null) => String::new(),
        Some(Value::String(t)) => match d.kind {
            PropKind::Text | PropKind::Enum => t.clone(),
            _ => return Err(EngineError::Params(format!("`{key}` takes a {}", kind_name(d.kind)))),
        },
        Some(Value::Number(n)) => match (d.kind, n.as_f64()) {
            (PropKind::Number, Some(f)) if f.is_finite() => n.to_string(),
            (PropKind::Integer, Some(f)) if f.is_finite() && f.fract() == 0.0 => n.to_string(),
            _ => return Err(EngineError::Params(format!("`{key}` takes a {}", kind_name(d.kind)))),
        },
        Some(Value::Bool(b)) if d.kind == PropKind::Bool => b.to_string(),
        Some(_) => return Err(EngineError::Params(format!("`{key}` takes a {}", kind_name(d.kind)))),
    };
    if d.kind == PropKind::Enum && !value.is_empty() && !d.values.contains(&value) {
        return Err(EngineError::Params(format!("`{key}` must be one of {}", d.values.join(", "))));
    }
    if key == "title" {
        write_title(&mut s.doc, &value);
    } else {
        let props = &mut s.doc.properties;
        if value.is_empty() {
            props.fields.remove(key);
        } else {
            if !props.fields.contains_key(key) && props.fields.len() >= DocProperties::MAX_FIELDS {
                return Err(EngineError::Params(format!("a document keeps at most {} named properties", DocProperties::MAX_FIELDS)));
            }
            props.fields.insert(key.to_string(), value.clone());
        }
        props.sanitize();
    }
    let props = &s.doc.properties;
    let stored = if key == "title" { props.title.clone() } else { props.fields.get(key).cloned().unwrap_or_default() };
    let mut out = json!({"key": key, "value": stored});
    if let (Some(i), Some(o)) = (value_issue(d, &stored), out.as_object_mut()) {
        o.insert("issue".into(), json!(i));
    }
    Ok(out)
}

fn kind_name(k: vizcraft_notation::PropKind) -> &'static str {
    use vizcraft_notation::PropKind;
    match k {
        PropKind::Text => "string",
        PropKind::Number => "number",
        PropKind::Integer => "whole number",
        PropKind::Bool => "boolean",
        PropKind::Enum => "string from its values",
    }
}

fn list_properties(s: &mut Session, _: &Value) -> CmdResult {
    let page = s.doc.pages.iter().position(|p| !p.is_background && p.sheet.is_some()).unwrap_or_else(|| s.doc.active_index());
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for pack in vizcraft_notation::catalog().packs() {
        for d in &pack.document_properties {
            if !seen.insert(d.key.clone()) {
                continue;
            }
            let value = vizcraft_diagram::text_fields::doc_value(&s.doc, page, &d.key);
            let mut row = json!({
                "key": d.key, "label": d.label, "group": d.group, "required": d.required, "maxChars": d.max_chars,
                "kind": kind_name(d.kind), "values": d.values, "value": value, "pack": pack.id,
            });
            let issue = if d.required && value.trim().is_empty() { Some("required".to_string()) } else { value_issue(d, &value) };
            if let (Some(i), Some(o)) = (issue, row.as_object_mut()) {
                o.insert("issue".into(), json!(i));
            }
            out.push(row);
        }
    }
    Ok(json!({"properties": out}))
}

/// How many items of each kind of hidden data the document holds.
fn counts(s: &Session) -> [usize; 7] {
    let doc = &s.doc;
    let shapes = || doc.pages.iter().flat_map(|p| &p.shapes);
    let p = &doc.properties;
    [
        doc.comments.len(),
        shapes().filter(|sh| sh.behavior.hidden).count(),
        doc.pages.iter().flat_map(|p| &p.layers).filter(|l| !l.visible).count(),
        shapes().filter(|sh| !sh.screen_tip.is_empty()).count(),
        shapes().map(|sh| sh.data.rows.len()).sum(),
        doc.images.len(),
        [&p.title, &p.author, &p.subject, &p.keywords, &p.description].iter().filter(|t| !t.is_empty()).count() + p.fields.len(),
    ]
}

fn inspect(s: &mut Session, _: &Value) -> CmdResult {
    let n = counts(s);
    let map: Map<String, Value> = HIDDEN_KINDS.iter().zip(n).map(|(k, n)| (k.to_string(), json!(n))).collect();
    Ok(json!({"counts": map, "total": n.iter().sum::<usize>()}))
}

fn remove(s: &mut Session, v: &Value) -> CmdResult {
    let what: Vec<String> = match v.get("what") {
        Some(Value::String(t)) if t == "all" => HIDDEN_KINDS.iter().map(|k| k.to_string()).collect(),
        Some(Value::Array(a)) if !a.is_empty() && a.len() <= 64 => a
            .iter()
            .map(|x| x.as_str().map(str::to_string).ok_or_else(|| EngineError::Params("`what` must hold strings".into())))
            .collect::<Result<_, _>>()?,
        _ => return Err(EngineError::Params("`what` must be \"all\" or a non-empty list of kinds".into())),
    };
    if let Some(bad) = what.iter().find(|k| !HIDDEN_KINDS.contains(&k.as_str())) {
        return Err(EngineError::Params(format!("unknown kind `{}`; use one of {}", short(bad), HIDDEN_KINDS.join(", "))));
    }
    let wants = |k: &str| what.iter().any(|w| w == k);
    let mut removed = Map::new();
    let before = counts(s);
    if wants("hiddenShapes") {
        let mut relationships = Vec::new();
        for page in &mut s.doc.pages {
            let ids: Vec<String> = page.shapes.iter().filter(|sh| sh.behavior.hidden).map(|sh| sh.id.clone()).collect();
            relationships.extend(delete_on_page(page, &ids).relationships);
        }
        drop_undrawn_relationships(&mut s.doc, &relationships);
    }
    if wants("hiddenLayers") {
        let mut relationships = Vec::new();
        for page in &mut s.doc.pages {
            let hidden: Vec<String> = page.layers.iter().filter(|l| !l.visible).map(|l| l.id.clone()).collect();
            if hidden.is_empty() {
                continue;
            }
            let only_hidden: Vec<String> = page
                .shapes
                .iter()
                .filter(|sh| !sh.layers.is_empty() && sh.layers.iter().all(|l| hidden.contains(l)))
                .map(|sh| sh.id.clone())
                .collect();
            relationships.extend(delete_on_page(page, &only_hidden).relationships);
            page.layers.retain(|l| l.visible);
            for sh in &mut page.shapes {
                sh.layers.retain(|l| !hidden.contains(l));
            }
        }
        drop_undrawn_relationships(&mut s.doc, &relationships);
    }
    if wants("comments") {
        s.doc.comments.clear();
    }
    if wants("screenTips") {
        s.doc.pages.iter_mut().flat_map(|p| &mut p.shapes).for_each(|sh| sh.screen_tip.clear());
    }
    if wants("dataRows") {
        s.doc.pages.iter_mut().flat_map(|p| &mut p.shapes).for_each(|sh| sh.data.rows.clear());
    }
    if wants("images") {
        s.doc.images.clear();
        s.doc.pages.iter_mut().flat_map(|p| &mut p.shapes).for_each(|sh| sh.image = None);
    }
    if wants("properties") {
        s.doc.properties = DocProperties::default();
    }
    for (i, k) in HIDDEN_KINDS.iter().enumerate() {
        if wants(k) {
            removed.insert(k.to_string(), json!(before.get(i).copied().unwrap_or(0)));
        }
    }
    prune_selection(s);
    with_sel(s, json!({"removed": removed}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vizcraft_diagram::{Comment, Connector, End, ImageAsset, ImageRef, Layer, Page, Shape, ShapeKind};

    fn run(s: &mut Session, id: &str, params: Value) -> Value {
        s.run(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    /// A document with one of everything: a comment, a hidden shape with a connector, a hidden
    /// layer with a shape only on it, a screen tip, a data row, a picture and file properties.
    fn loaded() -> Session {
        let mut doc = vizcraft_diagram::Document::new();
        let page = doc.page_mut().unwrap();
        let mut a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
        a.screen_tip = "tip".into();
        a.data.rows.push(vizcraft_diagram::data::DataRow { key: "k".into(), value: "v".into(), ..Default::default() });
        a.image = Some(ImageRef { asset: "i1".into(), ..Default::default() });
        let mut h = Shape::new("h", ShapeKind::Rectangle, 200.0, 0.0, 100.0, 60.0);
        h.behavior.hidden = true;
        let mut l = Shape::new("l", ShapeKind::Rectangle, 400.0, 0.0, 100.0, 60.0);
        l.layers = vec!["ly".into()];
        let mut both = Shape::new("both", ShapeKind::Rectangle, 600.0, 0.0, 100.0, 60.0);
        both.layers = vec!["ly".into(), "vis".into()];
        page.shapes.extend([a, h, l, both]);
        page.connectors.push(Connector::new("c", End::dynamic("a"), End::dynamic("h")));
        page.layers.push(Layer { id: "ly".into(), name: "Notes".into(), visible: false, ..Layer::default() });
        page.layers.push(Layer { id: "vis".into(), name: "Main".into(), ..Layer::default() });
        doc.comments.push(Comment { id: "m1".into(), text: "hello".into(), ..Default::default() });
        doc.images.push(ImageAsset { id: "i1".into(), media_type: "image/png".into(), bytes: vec![1, 2, 3] });
        doc.properties.title = "T".into();
        doc.properties.author = "A".into();
        doc.pages.push(Page::new("p2", "Second"));
        Session::new(doc)
    }

    #[test]
    fn set_properties_sets_only_the_given_ones() {
        let mut s = loaded();
        let r = run(&mut s, "doc.setProperties", json!({"subject": "S", "keywords": "k1; k2", "description": "D", "author": ""}));
        assert_eq!(r["properties"]["subject"], "S");
        assert_eq!((s.doc.properties.title.as_str(), s.doc.properties.author.as_str()), ("T", ""));
        assert!(s.can_undo());
        assert!(s.run("doc.setProperties", &json!({})).is_err());
        assert!(s.run("doc.setProperties", &json!({"title": 4})).is_err());
        run(&mut s, "doc.setProperties", json!({"title": "x".repeat(5000)}));
        assert_eq!(s.doc.properties.title.len(), DocProperties::MAX_SHORT);
    }

    /// Regression: the Inspector's title (`edit.setTitle`) and the ISO 7200 title field
    /// (`doc.setProperty {key: "title"}`) were two keys that disagreed. Every writer now sets the
    /// one title, and an old document whose two fields differ reads the property.
    #[test]
    fn every_title_writer_sets_the_one_title() {
        let mut s = loaded();
        run(&mut s, "doc.setProperty", json!({"key": "title", "value": "Pump skid"}));
        assert_eq!((s.doc.title.as_str(), s.doc.properties.title.as_str()), ("Pump skid", "Pump skid"));
        run(&mut s, "edit.setTitle", json!({"title": "Wiring"}));
        assert_eq!((s.doc.title.as_str(), s.doc.properties.title.as_str()), ("Wiring", "Wiring"));
        let listed = run(&mut s, "doc.properties", json!({}));
        let row = listed["properties"].as_array().unwrap().iter().find(|p| p["key"] == "title").cloned().unwrap();
        assert_eq!(row["value"], "Wiring", "doc.properties reads what edit.setTitle wrote");
        run(&mut s, "doc.setProperties", json!({"title": "Plant"}));
        assert_eq!((s.doc.title.as_str(), s.doc.properties.title.as_str()), ("Plant", "Plant"));
        // A document saved before the fix with two different titles: the property wins.
        let mut doc = vizcraft_diagram::Document::new();
        doc.title = "Old window title".into();
        doc.properties.title = "Title block title".into();
        assert_eq!(vizcraft_diagram::text_fields::doc_value(&doc, 0, "title"), "Title block title");
    }

    #[test]
    fn inspect_counts_each_kind_and_changes_nothing() {
        let mut s = loaded();
        let before = s.doc.clone();
        let r = run(&mut s, "doc.inspectHidden", json!({}));
        let c = &r["counts"];
        assert_eq!(
            (c["comments"].as_u64(), c["hiddenShapes"].as_u64(), c["hiddenLayers"].as_u64(), c["screenTips"].as_u64()),
            (Some(1), Some(1), Some(1), Some(1))
        );
        assert_eq!((c["dataRows"].as_u64(), c["images"].as_u64(), c["properties"].as_u64()), (Some(1), Some(1), Some(2)));
        assert_eq!(r["total"], 8);
        assert_eq!(s.doc, before);
        assert!(!s.can_undo());
    }

    #[test]
    fn remove_takes_the_chosen_kinds_in_one_undo_step() {
        let mut s = loaded();
        let before = s.doc.clone();
        let r = run(
            &mut s,
            "doc.removeHidden",
            json!({"what": ["comments", "hiddenShapes", "hiddenLayers", "screenTips", "dataRows", "images", "properties"]}),
        );
        assert_eq!(r["removed"]["hiddenShapes"], 1);
        let ids: Vec<&str> = s.doc.pages[0].shapes.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["a", "both"], "the hidden shape and the shape only on a hidden layer are gone");
        assert!(matches!(s.doc.pages[0].connectors[0].end, End::Free { .. }), "the connector to the hidden shape keeps its place, unglued");
        assert_eq!(s.doc.pages[0].layers.len(), 1);
        assert!(s.doc.pages[0].shapes[1].layers == ["vis"], "a shape on both keeps the visible layer");
        let total = run(&mut s, "doc.inspectHidden", json!({}))["total"].as_u64();
        assert_eq!(total, Some(0));
        assert!(s.doc.images.is_empty() && s.doc.pages[0].shapes[0].image.is_none());
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc, before, "one undo step restores everything");
    }

    #[test]
    fn remove_only_what_was_asked_and_refuses_junk() {
        let mut s = loaded();
        run(&mut s, "doc.removeHidden", json!({"what": ["comments"]}));
        assert!(s.doc.comments.is_empty());
        assert_eq!(s.doc.pages[0].shapes.len(), 4);
        assert_eq!(s.doc.images.len(), 1);
        run(&mut s, "doc.removeHidden", json!({"what": "all"}));
        assert_eq!(run(&mut s, "doc.inspectHidden", json!({}))["total"], 0);
        for bad in [json!({}), json!({"what": []}), json!({"what": ["nope"]}), json!({"what": [3]}), json!({"what": "some"})] {
            assert!(s.run("doc.removeHidden", &bad).is_err(), "{bad}");
        }
    }
}
