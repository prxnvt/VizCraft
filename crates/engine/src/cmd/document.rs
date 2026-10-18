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
        CommandSpec::new("doc.inspectHidden", "Inspect Document", "File > Info", inspect)
            .params(r#"{} -> {"counts": {"comments", "hiddenShapes", "hiddenLayers", "screenTips", "dataRows", "images", "properties"}, "total": n}; read-only"#),
        CommandSpec::new("doc.removeHidden", "Remove Hidden Data", "File > Info", remove).params(
            r#"{"what": ["comments"|"hiddenShapes"|"hiddenLayers"|"screenTips"|"dataRows"|"images"|"properties"] or "all"} -> {"removed": {kind: n}}; one undo step; a hidden layer is removed with the shapes that are only on hidden layers"#,
        ),
    ]
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
    let p = &mut s.doc.properties;
    for (k, t) in given {
        match k {
            "title" => p.title = t,
            "subject" => p.subject = t,
            "author" => p.author = t,
            "keywords" => p.keywords = t,
            _ => p.description = t,
        }
    }
    p.sanitize();
    Ok(json!({"properties": properties_json(&s.doc.properties)}))
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
        [&p.title, &p.author, &p.subject, &p.keywords, &p.description].iter().filter(|t| !t.is_empty()).count(),
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
