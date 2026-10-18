//! Layer commands (`layer.*`): add, delete, change and assign the layers of a page, plus the
//! guards other commands call so locked shapes stay put and glue honours a layer's glue flag.
//!
//! A page is named by id, name or zero-based index and defaults to the active page; a layer by
//! id or name. The rules for what a layer allows live in `vizcraft_diagram::layer`.

use serde_json::{Value, json};
use vizcraft_diagram::page_fields::MAX_LAYERS;
use vizcraft_diagram::{Color, Layer, Page, Shape, layer};

use super::{MAX_IDS, delete_on_page, drop_undrawn_relationships, ids_param, opt_bool, opt_str, prune_selection, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The layer commands, in menu order.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("layer.add", "Add Layer", "Home > Editing > Layers", add)
            .params(r#"{"name": string, "page"?: id|name|index (default the active page)} -> {page, layer: {id, name, visible, locked, print, snap, glue, color?}}"#),
        CommandSpec::new("layer.delete", "Delete Layer", "Home > Editing > Layers", delete).params(
            r#"{"layer": id|name, "page"?: id|name|index, "deleteShapes"?: bool} -> {page, layer, deleted} (default: the layer's shapes stay and leave the layer; deleteShapes deletes them, refused while any is locked)"#,
        ),
        CommandSpec::new("layer.set", "Layer Properties", "Home > Editing > Layers", set).params(
            r##"{"layer": id|name, "page"?: id|name|index, "name"?: string, "visible"?: bool, "locked"?: bool, "print"?: bool, "snap"?: bool, "glue"?: bool, "color"?: "#rrggbb"|null (null or "" clears)} -> {page, layer}"##,
        ),
        CommandSpec::new("layer.assign", "Assign to Layer", "Home > Editing > Layers", assign)
            .params(r#"{"ids"?: [string] (default: the selected shapes), "layer": id|name, "page"?: id|name|index} -> {page, layer, changed}"#),
        CommandSpec::new("layer.unassign", "Remove from Layer", "Home > Editing > Layers", unassign)
            .params(r#"{"ids"?: [string] (default: the selected shapes), "layer": id|name, "page"?: id|name|index} -> {page, layer, changed}"#),
    ]
}

/// Refuse when any of `ids` names a shape on the active page that is on a locked layer. Commands
/// that move, resize, restyle or delete shapes call this first, so a locked shape is never
/// changed behind the user's back; ids that are not shapes (connectors, typos) pass, for the
/// command's own checks to report.
pub fn ensure_unlocked<I, S>(s: &Session, ids: I) -> Result<(), EngineError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some(page) = s.doc.page() else { return Ok(()) };
    for id in ids {
        if let Some(sh) = page.shape(id.as_ref())
            && layer::locked(page, sh)
        {
            return Err(EngineError::Params(format!("`{}` is on a locked layer", short(&sh.id))));
        }
    }
    Ok(())
}

/// Refuse when any of `ids` names a shape on the active page that cannot be selected (hidden, or
/// on a locked layer).
pub fn ensure_selectable<I, S>(s: &Session, ids: I) -> Result<(), EngineError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some(page) = s.doc.page() else { return Ok(()) };
    for id in ids {
        if let Some(sh) = page.shape(id.as_ref())
            && !layer::selectable(page, sh)
        {
            let why = if layer::visible(page, sh) { "on a locked layer" } else { "hidden" };
            return Err(EngineError::Params(format!("`{}` is {why} and cannot be selected", short(&sh.id))));
        }
    }
    Ok(())
}

/// `ids` without the shapes on the active page that cannot be selected (for select all).
pub fn selectable_only(s: &Session, ids: Vec<String>) -> Vec<String> {
    let Some(page) = s.doc.page() else { return ids };
    ids.into_iter().filter(|id| page.shape(id).is_none_or(|sh| layer::selectable(page, sh))).collect()
}

/// Refuse glue to `shape` when it is hidden or on a layer that turns glue off; `what` names the
/// end for the message.
pub fn ensure_gluable(page: &Page, shape: &Shape, what: &str) -> Result<(), EngineError> {
    if layer::gluable(page, shape) {
        Ok(())
    } else if !layer::visible(page, shape) {
        Err(EngineError::Params(format!("{what}: `{}` is hidden and takes no glue", short(&shape.id))))
    } else {
        Err(EngineError::Params(format!("{what}: `{}` is on a layer that does not allow glue", short(&shape.id))))
    }
}

/// The index of the page the `page` param names, else the active page (index 0).
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

fn page_at(s: &mut Session, i: usize) -> Result<&mut Page, EngineError> {
    s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Failed("the document has no page".into()))
}

/// The index of the layer `layer` names on `page`: an id first, then a name.
fn layer_index(page: &Page, v: &Value) -> Result<usize, EngineError> {
    let r = opt_str(v, "layer")?.ok_or_else(|| EngineError::Params("`layer` (id or name) is required".into()))?;
    page.layers
        .iter()
        .position(|l| l.id == r)
        .or_else(|| page.layers.iter().position(|l| l.name == r))
        .ok_or_else(|| EngineError::Params(format!("no layer `{}` on page `{}`", short(r), short(&page.name))))
}

/// A layer name: trimmed, not empty, at most [`Layer::MAX_NAME`] bytes, and not the name of
/// another layer on `page` (ignoring case), so a name always finds one layer.
fn checked_name(page: &Page, name: &str, except: Option<usize>) -> Result<String, EngineError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(EngineError::Params("a layer name cannot be empty".into()));
    }
    if name.len() > Layer::MAX_NAME {
        return Err(EngineError::Params(format!("a layer name is at most {} bytes", Layer::MAX_NAME)));
    }
    let taken = page.layers.iter().enumerate().any(|(i, l)| Some(i) != except && l.name.eq_ignore_ascii_case(name));
    if taken {
        return Err(EngineError::Params(format!("page `{}` already has a layer named `{}`", short(&page.name), short(name))));
    }
    Ok(name.to_string())
}

/// The layer as a command reports it.
fn layer_json(l: &Layer) -> Value {
    serde_json::to_value(l).unwrap_or(Value::Null)
}

/// The first `layerN` id not used on `page`.
fn fresh_id(page: &Page) -> String {
    (1..=MAX_LAYERS + 1).map(|n| format!("layer{n}")).find(|id| page.layer(id).is_none()).unwrap_or_else(|| format!("layer{}", MAX_LAYERS + 2))
}

/// Drop selected shapes that can no longer be selected (after hiding or locking a layer).
fn prune_unselectable(s: &mut Session) {
    let items = std::mem::take(&mut s.sel.items);
    s.sel.items = selectable_only(s, items);
    prune_selection(s);
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let name = opt_str(v, "name")?.ok_or_else(|| EngineError::Params("`name` (string) is required".into()))?;
    let i = page_index(s, v)?;
    let page = page_at(s, i)?;
    if page.layers.len() >= MAX_LAYERS {
        return Err(EngineError::Params(format!("a page holds at most {MAX_LAYERS} layers")));
    }
    let name = checked_name(page, name, None)?;
    let l = Layer { id: fresh_id(page), name, ..Layer::default() };
    let out = layer_json(&l);
    page.layers.push(l);
    let pid = page.id.clone();
    Ok(json!({"page": pid, "layer": out}))
}

fn delete(s: &mut Session, v: &Value) -> CmdResult {
    let with_shapes = opt_bool(v, "deleteShapes")?.unwrap_or(false);
    let i = page_index(s, v)?;
    let page = page_at(s, i)?;
    let li = layer_index(page, v)?;
    let lid = page.layers.get(li).map(|l| l.id.clone()).unwrap_or_default();
    let mut deleted = Vec::new();
    if with_shapes {
        let on: Vec<&Shape> = page.shapes.iter().filter(|sh| sh.layers.contains(&lid)).collect();
        if let Some(sh) = on.iter().find(|sh| layer::locked(page, sh)) {
            return Err(EngineError::Params(format!("`{}` is on a locked layer; unlock it before deleting its shapes", short(&sh.id))));
        }
        let ids: Vec<String> = on.iter().map(|sh| sh.id.clone()).collect();
        let d = delete_on_page(page, &ids);
        deleted = d.items;
        drop_undrawn_relationships(&mut s.doc, &d.relationships);
    }
    let page = page_at(s, i)?;
    page.layers.retain(|l| l.id != lid);
    for sh in &mut page.shapes {
        sh.layers.retain(|l| *l != lid);
    }
    let pid = page.id.clone();
    prune_selection(s);
    with_sel(s, json!({"page": pid, "layer": lid, "deleted": deleted}))
}

/// The `color` param: absent keeps the colour (`None`), null or `""` clears it, a colour sets it.
fn color_param(v: &Value) -> Result<Option<Option<Color>>, EngineError> {
    match v.get("color") {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(t)) if t.trim().is_empty() || t.trim().eq_ignore_ascii_case("none") => Ok(Some(None)),
        Some(Value::String(t)) => Color::parse(t.trim())
            .map(|c| Some(Some(c)))
            .ok_or_else(|| EngineError::Params(format!("`color`: `{}` is not a #rrggbb colour", short(t)))),
        Some(_) => Err(EngineError::Params("`color` must be a #rrggbb string or null".into())),
    }
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let name = opt_str(v, "name")?;
    let flags = [opt_bool(v, "visible")?, opt_bool(v, "locked")?, opt_bool(v, "print")?, opt_bool(v, "snap")?, opt_bool(v, "glue")?];
    let color = color_param(v)?;
    if name.is_none() && flags.iter().all(Option::is_none) && color.is_none() {
        return Err(EngineError::Params("give at least one of name, visible, locked, print, snap, glue, color".into()));
    }
    let i = page_index(s, v)?;
    let page = page_at(s, i)?;
    let li = layer_index(page, v)?;
    let name = name.map(|n| checked_name(page, n, Some(li))).transpose()?;
    let l = page.layers.get_mut(li).ok_or_else(|| EngineError::Failed("the layer went away".into()))?;
    if let Some(n) = name {
        l.name = n;
    }
    let [visible, locked, print, snap, glue] = flags;
    for (field, value) in [(&mut l.visible, visible), (&mut l.locked, locked), (&mut l.print, print), (&mut l.snap, snap), (&mut l.glue, glue)] {
        if let Some(b) = value {
            *field = b;
        }
    }
    if let Some(c) = color {
        l.color = c;
    }
    let out = layer_json(l);
    let pid = page.id.clone();
    prune_unselectable(s);
    with_sel(s, json!({"page": pid, "layer": out}))
}

/// The shapes `assign` and `unassign` act on: `ids` (each a shape on the page), else the
/// selected shapes when the page is the active one.
fn shape_ids(s: &Session, v: &Value, i: usize) -> Result<Vec<String>, EngineError> {
    let page = s.doc.pages.get(i).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let mut out: Vec<String> = Vec::new();
    match ids_param(v)? {
        Some(ids) => {
            for id in ids {
                if page.shape(&id).is_none() {
                    return Err(EngineError::Params(format!("`{}` is not a shape on page `{}`", short(&id), short(&page.name))));
                }
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        None if i == 0 => out.extend(s.sel.items.iter().filter(|id| page.shape(id).is_some()).cloned()),
        None => {}
    }
    if out.is_empty() {
        return Err(EngineError::Params("select a shape first (or pass `ids`)".into()));
    }
    out.truncate(MAX_IDS);
    Ok(out)
}

/// Put shapes on a layer (`on`) or take them off; reports the ids whose layers changed.
fn place(s: &mut Session, v: &Value, on: bool) -> CmdResult {
    let i = page_index(s, v)?;
    let li = layer_index(s.doc.pages.get(i).ok_or_else(|| EngineError::Failed("the document has no page".into()))?, v)?;
    let ids = shape_ids(s, v, i)?;
    let page = page_at(s, i)?;
    let lid = page.layers.get(li).map(|l| l.id.clone()).unwrap_or_default();
    let mut changed = Vec::new();
    for id in &ids {
        let Some(sh) = page.shape_mut(id) else { continue };
        let has = sh.layers.contains(&lid);
        if on && !has && sh.layers.len() < MAX_LAYERS {
            sh.layers.push(lid.clone());
            changed.push(id.clone());
        } else if !on && has {
            sh.layers.retain(|l| *l != lid);
            changed.push(id.clone());
        }
    }
    let pid = page.id.clone();
    prune_unselectable(s);
    with_sel(s, json!({"page": pid, "layer": lid, "changed": changed}))
}

fn assign(s: &mut Session, v: &Value) -> CmdResult {
    place(s, v, true)
}

fn unassign(s: &mut Session, v: &Value) -> CmdResult {
    place(s, v, false)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn layer_commands_default_to_the_active_page() {
        let mut s = Session::new(crate::Document::new());
        s.run("page.add", &json!({"name": "Second"})).unwrap();
        s.run("page.setActive", &json!({"page": "Second"})).unwrap();
        s.run("layer.add", &json!({"name": "Notes"})).unwrap();
        assert!(s.doc.pages[0].layers.is_empty(), "the first page is untouched");
        assert_eq!(s.doc.pages[1].layers.len(), 1, "the active page gets the layer");
    }
    use vizcraft_diagram::{Document, End, Page, Shape, ShapeKind};

    use crate::Session;

    /// A page with shapes `a` and `b` and a connector `c` between them, plus a second page.
    fn session() -> Session {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 300.0, 0.0, 100.0, 60.0));
        page.connectors.push(vizcraft_diagram::Connector::new("c", End::dynamic("a"), End::dynamic("b")));
        doc.pages.push(Page::new("p2", "Second"));
        doc.sanitize();
        Session::new(doc)
    }

    fn layers(s: &Session) -> Vec<String> {
        s.doc.page().unwrap().layers.iter().map(|l| l.name.clone()).collect()
    }

    fn on(s: &Session, id: &str) -> Vec<String> {
        s.doc.page().unwrap().shape(id).unwrap().layers.clone()
    }

    #[test]
    fn add_names_layers_uniquely_and_undoes() {
        let mut s = session();
        let r = s.run("layer.add", &json!({"name": " Walls "})).unwrap();
        assert_eq!(r["layer"]["id"], "layer1");
        assert_eq!(r["layer"]["name"], "Walls");
        assert_eq!(r["layer"]["visible"], true);
        assert!(s.run("layer.add", &json!({"name": "walls"})).is_err(), "duplicate names are refused, ignoring case");
        assert!(s.run("layer.add", &json!({"name": "  "})).is_err());
        assert!(s.run("layer.add", &json!({})).is_err());
        assert!(s.run("layer.add", &json!({"name": "X", "page": "nope"})).is_err());
        let r = s.run("layer.add", &json!({"name": "Walls", "page": "Second"})).unwrap();
        assert_eq!(r["page"], "p2", "another page has its own names");
        s.run("edit.undo", &json!({})).unwrap();
        assert!(s.doc.pages[1].layers.is_empty());
        s.run("edit.undo", &json!({})).unwrap();
        assert!(layers(&s).is_empty());
    }

    #[test]
    fn set_changes_flags_name_and_colour() {
        let mut s = session();
        s.run("layer.add", &json!({"name": "A"})).unwrap();
        s.run("layer.add", &json!({"name": "B"})).unwrap();
        let r = s.run("layer.set", &json!({"layer": "A", "visible": false, "print": false, "color": "#ff0000", "name": "Notes"})).unwrap();
        assert_eq!(r["layer"]["visible"], false);
        assert_eq!(r["layer"]["color"], "#ff0000");
        assert_eq!(layers(&s), vec!["Notes", "B"]);
        let l = &s.doc.page().unwrap().layers[0];
        assert!(!l.visible && !l.print && l.snap && l.glue && !l.locked);
        s.run("layer.set", &json!({"layer": "layer1", "color": null})).unwrap();
        assert_eq!(s.doc.page().unwrap().layers[0].color, None);
        assert!(s.run("layer.set", &json!({"layer": "Notes", "name": "b"})).is_err(), "renaming onto another layer's name");
        s.run("layer.set", &json!({"layer": "Notes", "name": "NOTES"})).unwrap();
        assert!(s.run("layer.set", &json!({"layer": "Nope", "visible": true})).is_err());
        assert!(s.run("layer.set", &json!({"layer": "B"})).is_err(), "nothing to change");
        assert!(s.run("layer.set", &json!({"layer": "B", "color": "red"})).is_err());
        assert!(s.run("layer.set", &json!({"layer": "B", "locked": "yes"})).is_err());
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(layers(&s), vec!["Notes", "B"]);
    }

    #[test]
    fn assign_and_unassign_validate_ids_and_undo() {
        let mut s = session();
        s.run("layer.add", &json!({"name": "A"})).unwrap();
        let r = s.run("layer.assign", &json!({"ids": ["a", "b"], "layer": "A"})).unwrap();
        assert_eq!(r["changed"], json!(["a", "b"]));
        assert_eq!(on(&s, "a"), vec!["layer1"]);
        let r = s.run("layer.assign", &json!({"ids": ["a"], "layer": "A"})).unwrap();
        assert_eq!(r["changed"], json!([]), "already on it");
        assert!(s.run("layer.assign", &json!({"ids": ["zz"], "layer": "A"})).is_err());
        assert!(s.run("layer.assign", &json!({"ids": ["c"], "layer": "A"})).is_err(), "connectors are not on layers");
        assert!(s.run("layer.assign", &json!({"ids": ["a"], "layer": "Nope"})).is_err());
        assert!(s.run("layer.assign", &json!({"layer": "A"})).is_err(), "nothing selected");
        s.run("edit.select", &json!({"ids": ["b"]})).unwrap();
        let r = s.run("layer.unassign", &json!({"layer": "A"})).unwrap();
        assert_eq!(r["changed"], json!(["b"]), "the selection by default");
        assert!(on(&s, "b").is_empty());
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(on(&s, "b"), vec!["layer1"]);
        s.run("edit.undo", &json!({})).unwrap();
        assert!(on(&s, "a").is_empty());
    }

    #[test]
    fn delete_keeps_shapes_by_default_or_deletes_them() {
        let mut s = session();
        s.run("layer.add", &json!({"name": "A"})).unwrap();
        s.run("layer.assign", &json!({"ids": ["a"], "layer": "A"})).unwrap();
        let r = s.run("layer.delete", &json!({"layer": "A"})).unwrap();
        assert_eq!(r["deleted"], json!([]));
        assert!(layers(&s).is_empty());
        assert!(on(&s, "a").is_empty(), "the shape left the layer");
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(on(&s, "a"), vec!["layer1"]);
        s.run("layer.set", &json!({"layer": "A", "locked": true})).unwrap();
        assert!(s.run("layer.delete", &json!({"layer": "A", "deleteShapes": true})).is_err(), "locked shapes are not deleted");
        s.run("layer.set", &json!({"layer": "A", "locked": false})).unwrap();
        let r = s.run("layer.delete", &json!({"layer": "A", "deleteShapes": true})).unwrap();
        assert_eq!(r["deleted"], json!(["a"]));
        let page = s.doc.page().unwrap();
        assert!(page.shape("a").is_none());
        assert!(page.connector("c").is_some(), "the connector keeps its other end and turns free here");
        assert!(s.run("layer.delete", &json!({"layer": "A"})).is_err());
        s.run("edit.undo", &json!({})).unwrap();
        assert!(s.doc.page().unwrap().shape("a").is_some());
    }

    #[test]
    fn locked_shapes_refuse_selection_and_changes() {
        let mut s = session();
        s.run("layer.add", &json!({"name": "A"})).unwrap();
        s.run("layer.assign", &json!({"ids": ["a"], "layer": "A"})).unwrap();
        s.run("edit.select", &json!({"ids": ["a", "b"]})).unwrap();
        s.run("layer.set", &json!({"layer": "A", "locked": true})).unwrap();
        assert_eq!(s.sel.items, vec!["b".to_string()], "locking drops the shape from the selection");
        assert!(s.run("edit.select", &json!({"ids": ["a"]})).is_err());
        s.run("edit.selectAll", &json!({})).unwrap();
        assert!(!s.sel.items.contains(&"a".to_string()));
        let before = s.doc.clone();
        for (id, p) in [
            ("shape.move", json!({"ids": ["a"], "dx": 10, "dy": 0})),
            ("shape.move", json!({"id": "a", "x": 10, "y": 0})),
            ("shape.resize", json!({"id": "a", "w": 10, "h": 10})),
            ("shape.rotate", json!({"ids": ["a"], "angle": 45})),
            ("shape.setText", json!({"id": "a", "text": "x"})),
            ("shape.setStyle", json!({"ids": ["a"], "bold": true})),
            ("shape.delete", json!({"ids": ["a"]})),
        ] {
            assert!(s.run(id, &p).is_err(), "{id} on a locked shape");
        }
        assert_eq!(s.doc, before);
        s.run("shape.move", &json!({"ids": ["b"], "dx": 10, "dy": 0})).unwrap();
        // Hidden shapes cannot be selected either.
        s.run("layer.set", &json!({"layer": "A", "locked": false, "visible": false})).unwrap();
        assert!(s.run("edit.select", &json!({"ids": ["a"]})).is_err());
        s.run("layer.set", &json!({"layer": "A", "visible": true})).unwrap();
        s.run("edit.select", &json!({"ids": ["a"]})).unwrap();
        s.run("shape.move", &json!({"dx": 5, "dy": 5})).unwrap();
        s.run("edit.delete", &json!({})).unwrap();
        assert!(s.doc.page().unwrap().shape("a").is_none());
    }

    #[test]
    fn glue_is_refused_on_a_no_glue_layer() {
        let mut s = session();
        s.run("layer.add", &json!({"name": "A"})).unwrap();
        s.run("layer.assign", &json!({"ids": ["b"], "layer": "A"})).unwrap();
        s.run("layer.set", &json!({"layer": "A", "glue": false})).unwrap();
        assert!(s.run("connector.glue", &json!({"id": "c", "end": "end", "shape": "b"})).is_err());
        assert!(s.run("connector.add", &json!({"from": {"shape": "a"}, "to": {"shape": "b"}})).is_err());
        s.run("connector.add", &json!({"from": {"shape": "a"}, "to": {"x": 500, "y": 500}})).unwrap();
        s.run("layer.set", &json!({"layer": "A", "glue": true})).unwrap();
        s.run("connector.glue", &json!({"id": "c", "end": "end", "shape": "b"})).unwrap();
    }
}
