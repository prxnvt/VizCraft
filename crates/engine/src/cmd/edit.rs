//! Edit menu commands: undo, redo, selection, delete and the clipboard.
//!
//! The clipboard holds a JSON fragment, `{"vizcraft": "clip", "shapes": [...], "connectors":
//! [...], "elements"?: [...], "relationships"?: [...]}`, with shapes and connectors in the
//! document's own form and the model items the copied shapes and connectors draw in the model's
//! form (left out when there are none). `edit.copy` and `edit.cut`
//! store it in `Session::clipboard` and return it as `{"text"}` for the system clipboard;
//! `edit.paste` takes `{"text"}` from the host or falls back to the internal copy.

use std::collections::HashMap;

use serde_json::{Value, json};
use vizcraft_diagram::{Connector, Document, End, Shape, resolve_ends};
use vizcraft_model::{Element, MAX_KEY, MAX_TEXT, Relationship};

use super::model::{auto_tag, cat, clean_props, cut as cut_text};

use super::{MAX_IDS, delete_items, has_selection, ids_param, opt_bool, opt_str, page_mut, prune_selection, sel_result, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// How far a paste lands from the copied items, so the copy is visible beside the original.
pub const PASTE_OFFSET: f64 = 20.0;

/// Largest clipboard text `edit.paste` reads; beyond this it refuses instead of parsing.
pub const MAX_CLIP_BYTES: usize = 64 * 1024 * 1024;

/// Return all edit commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("edit.undo", "Undo", "Home > Undo", |s, _| {
            s.undo();
            prune_selection(s);
            sel_result(s)
        })
        .key("Mod+Z")
        .when(|s| if s.can_undo() { None } else { Some("nothing to undo") }),
        CommandSpec::new("edit.redo", "Redo", "Home > Undo", |s, _| {
            s.redo();
            prune_selection(s);
            sel_result(s)
        })
        .key("Mod+Shift+Z / Mod+Y")
        .when(|s| if s.can_redo() { None } else { Some("nothing to redo") }),
        CommandSpec::new("edit.cut", "Cut", "Home > Clipboard", cut).key("Mod+X").when(has_selection),
        CommandSpec::new("edit.copy", "Copy", "Home > Clipboard", copy).key("Mod+C").when(has_selection),
        CommandSpec::new("edit.paste", "Paste", "Home > Clipboard", paste)
            .key("Mod+V")
            .params(r#"{"text"?: string} (default: the internal clipboard)"#),
        CommandSpec::new("edit.delete", "Delete", "Home > Editing", |s, _| {
            let ids = s.sel.items.clone();
            delete_items(s, &ids)
        })
        // Bare keys: the UI key map treats Delete and Backspace as shortcuts only while no text
        // field has the keyboard, so typing in a field still deletes characters.
        .key("Delete / Backspace")
        .when(has_selection),
        CommandSpec::new("edit.select", "Select", "Home > Editing", select_ids).params(r#"{"ids": [string], "add"?: bool}"#),
        CommandSpec::new("edit.selectAll", "Select All", "Home > Editing", select_all)
            .key("Mod+A")
            .when(|s| if s.doc.page().is_some_and(|p| !p.shapes.is_empty() || !p.connectors.is_empty()) { None } else { Some("the page is empty") }),
        CommandSpec::new("edit.selectNone", "Select None", "Home > Editing", |s, _| {
            s.sel.clear();
            sel_result(s)
        }),
        CommandSpec::new("edit.setTitle", "Document Title", "Properties", |s, v| {
            s.doc.title = p::req_str(v, "title")?.to_string();
            Ok(json!({"title": s.doc.title}))
        })
        .params(r#"{"title": string}"#),
    ]
}

/// Select exactly `ids`, or add them to the selection with `add: true`. Every id must name a
/// shape or connector on the page; an unknown id is an error and the selection stays as it was,
/// so an agent learns about a typo instead of acting on a partial selection.
fn select_ids(s: &mut Session, v: &Value) -> CmdResult {
    let ids = ids_param(v)?.ok_or_else(|| EngineError::Params("`ids` (array of strings) is required".into()))?;
    let add = opt_bool(v, "add")?.unwrap_or(false);
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    if let Some(bad) = ids.iter().find(|id| !page.contains_id(id)) {
        return Err(EngineError::Params(format!("`{}` is not a shape or connector on this page", short(bad))));
    }
    let mut out = if add { s.sel.items.clone() } else { Vec::new() };
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    let primary = if add { s.sel.primary.clone().filter(|p| out.contains(p)) } else { None };
    select(s, out);
    if primary.is_some() {
        s.sel.primary = primary;
    }
    sel_result(s)
}

/// Select every shape and connector on the page, shapes first.
fn select_all(s: &mut Session, _: &Value) -> CmdResult {
    let ids: Vec<String> =
        s.doc.page().map(|p| p.shapes.iter().map(|x| x.id.clone()).chain(p.connectors.iter().map(|c| c.id.clone())).collect()).unwrap_or_default();
    select(s, ids);
    sel_result(s)
}

/// The clipboard fragment for the selection. Connectors come only when selected; an end glued to
/// a shape that is not copied becomes a free end where it is drawn now, so the fragment stands
/// on its own.
fn clip_of_selection(s: &Session) -> (Value, usize, usize) {
    let Some(page) = s.doc.page() else { return (json!({"vizcraft": "clip", "shapes": [], "connectors": []}), 0, 0) };
    let shapes: Vec<&Shape> = page.shapes.iter().filter(|x| s.sel.contains(&x.id)).collect();
    let copied = |id: Option<&str>| id.is_some_and(|i| shapes.iter().any(|x| x.id == i));
    let mut connectors: Vec<Connector> = Vec::new();
    for c in page.connectors.iter().filter(|c| s.sel.contains(&c.id)) {
        let mut c = c.clone();
        let ends = resolve_ends(page, &c);
        if c.start.shape().is_some() && !copied(c.start.shape()) {
            c.start = End::free(ends.start.pos);
        }
        if c.end.shape().is_some() && !copied(c.end.shape()) {
            c.end = End::free(ends.end.pos);
        }
        connectors.push(c);
    }
    let (ns, nc) = (shapes.len(), connectors.len());
    let mut clip = json!({"vizcraft": "clip", "shapes": shapes, "connectors": connectors});
    // The model items drawn by the copied items travel with them, so a paste makes copies of the
    // elements instead of extra views of the originals.
    let model = &s.doc.model;
    let mut elements: Vec<&Element> = Vec::new();
    for e in shapes.iter().filter_map(|x| x.element.as_deref()).filter_map(|e| model.element(e)) {
        if !elements.iter().any(|x| x.id == e.id) {
            elements.push(e);
        }
    }
    let mut relationships: Vec<&Relationship> = Vec::new();
    for r in connectors.iter().filter_map(|c| c.relationship.as_deref()).filter_map(|r| model.relationship(r)) {
        if !relationships.iter().any(|x| x.id == r.id) {
            relationships.push(r);
        }
    }
    if let Some(o) = clip.as_object_mut() {
        if !elements.is_empty() {
            o.insert("elements".into(), json!(elements));
        }
        if !relationships.is_empty() {
            o.insert("relationships".into(), json!(relationships));
        }
    }
    (clip, ns, nc)
}

fn copy(s: &mut Session, _: &Value) -> CmdResult {
    if let Some(why) = has_selection(s) {
        return Err(EngineError::Params(why.into()));
    }
    let (clip, shapes, connectors) = clip_of_selection(s);
    let text = serde_json::to_string(&clip).map_err(|e| EngineError::Failed(format!("copy: {e}")))?;
    s.clipboard = text.clone();
    Ok(json!({"text": text, "shapes": shapes, "connectors": connectors}))
}

fn cut(s: &mut Session, v: &Value) -> CmdResult {
    let copied = copy(s, v)?;
    let ids = s.sel.items.clone();
    let mut out = delete_items(s, &ids)?;
    if let (Some(o), Some(text)) = (out.as_object_mut(), copied.get("text")) {
        o.insert("text".into(), text.clone());
    }
    Ok(out)
}

/// Paste a clip: fresh ids, glue between pasted items remapped to the new ids, everything offset
/// by [`PASTE_OFFSET`], and the pasted items selected. A connector end glued to a shape outside
/// the clip keeps its glue when that shape is on the page and is dropped otherwise (only a
/// hand-made clip can do that; `edit.copy` frees such ends).
fn paste(s: &mut Session, v: &Value) -> CmdResult {
    let text = match opt_str(v, "text")? {
        Some(t) => t.to_string(),
        None => s.clipboard.clone(),
    };
    if text.trim().is_empty() {
        return Err(EngineError::Params("the clipboard is empty".into()));
    }
    if text.len() > MAX_CLIP_BYTES {
        return Err(EngineError::Params("the clipboard text is too large to paste".into()));
    }
    let not_clip = || EngineError::Params("the clipboard does not hold VizCraft shapes".into());
    let clip: Value = serde_json::from_str(&text).map_err(|_| not_clip())?;
    if clip.get("vizcraft").and_then(Value::as_str) != Some("clip") {
        return Err(not_clip());
    }
    let list = |k: &str| clip.get(k).and_then(Value::as_array).map(|a| a.iter().take(MAX_IDS).cloned().collect::<Vec<_>>()).unwrap_or_default();
    let shapes: Vec<Shape> = list("shapes").into_iter().filter_map(|x| serde_json::from_value(x).ok()).collect();
    let connectors: Vec<Connector> = list("connectors").into_iter().filter_map(|x| serde_json::from_value(x).ok()).collect();
    let mut clip_elements: HashMap<String, Element> = HashMap::new();
    for e in list("elements").into_iter().filter_map(|x| serde_json::from_value::<Element>(x).ok()) {
        clip_elements.entry(e.id.clone()).or_insert(e);
    }
    let mut clip_relationships: HashMap<String, Relationship> = HashMap::new();
    for r in list("relationships").into_iter().filter_map(|x| serde_json::from_value::<Relationship>(x).ok()) {
        clip_relationships.entry(r.id.clone()).or_insert(r);
    }

    let mut map: HashMap<String, String> = HashMap::new();
    let mut elements = ModelCopies::default();
    let mut new_shapes = Vec::new();
    for mut sh in shapes {
        let id = s.doc.next_id("s");
        map.insert(std::mem::take(&mut sh.id), id.clone());
        sh.id = id;
        sh.x += PASTE_OFFSET;
        sh.y += PASTE_OFFSET;
        // A shape of an element the clip carries draws a new copy of it; one whose element the
        // clip does not carry (a hand-made clip, or a reference into another document) is free.
        sh.element = sh.element.take().and_then(|e| elements.element(&mut s.doc, &clip_elements, &e));
        if let Some(e) = sh.element.as_deref().and_then(|e| s.doc.model.element(e)) {
            sh.text = e.name.clone();
        }
        sh.sanitize();
        new_shapes.push(sh);
    }
    elements.finish(&mut s.doc, &clip_elements);
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let mut kept = Vec::new();
    'conns: for mut c in connectors {
        for e in [&mut c.start, &mut c.end] {
            match e {
                End::Glued { shape, point } => {
                    let target = match map.get(shape.as_str()) {
                        Some(new) => new_shapes.iter().find(|x| &x.id == new),
                        None => page.shape(shape),
                    };
                    let Some(target) = target else { continue 'conns };
                    if point.is_some_and(|i| i >= target.connection_points().len()) {
                        *point = None;
                    }
                    *shape = target.id.clone();
                }
                End::Free { x, y } => {
                    *x += PASTE_OFFSET;
                    *y += PASTE_OFFSET;
                }
            }
        }
        kept.push(c);
    }
    let mut new_connectors = Vec::new();
    for mut c in kept {
        c.id = s.doc.next_id("c");
        c.relationship = c.relationship.take().and_then(|r| elements.relationship(&mut s.doc, &clip_relationships, &r));
        if let Some(r) = c.relationship.as_deref().and_then(|r| s.doc.model.relationship(r)) {
            c.text = r.name.clone();
        }
        c.sanitize();
        new_connectors.push(c);
    }
    if new_shapes.is_empty() && new_connectors.is_empty() {
        return Err(EngineError::Params("the clipboard has nothing to paste".into()));
    }
    let ids: Vec<String> = new_shapes.iter().map(|x| x.id.clone()).chain(new_connectors.iter().map(|c| c.id.clone())).collect();
    let page = page_mut(s)?;
    page.shapes.extend(new_shapes);
    page.connectors.extend(new_connectors);
    select(s, ids.clone());
    with_sel(s, json!({"pasted": ids}))
}

/// The model copies one paste makes: each element and relationship of the clip is copied at most
/// once (fresh id, same type, name and properties), however many pasted items draw it.
#[derive(Default)]
struct ModelCopies {
    /// Clip element id to the new element's id.
    elements: HashMap<String, String>,
    /// Clip relationship id to the new relationship's id.
    relationships: HashMap<String, String>,
    /// Clip element ids in the order their copies were made.
    order: Vec<String>,
}

impl ModelCopies {
    /// The copy of clip element `old`, made on first use; `None` when the clip does not carry it
    /// or the model cannot take another element (the view is pasted as free drawing).
    fn element(&mut self, doc: &mut Document, clip: &HashMap<String, Element>, old: &str) -> Option<String> {
        if let Some(new) = self.elements.get(old) {
            return Some(new.clone());
        }
        let src = clip.get(old)?;
        if src.type_id.is_empty() || src.type_id.len() > MAX_KEY {
            return None;
        }
        let id = doc.next_id("e");
        let mut e = Element::new(id.clone(), src.type_id.clone());
        e.name = cut_text(&src.name, MAX_TEXT);
        e.properties = clean_props(&src.properties);
        doc.model.add_element(e).ok()?;
        if let Some(p) = cat().pack_ref(vizcraft_model::pack_of(&src.type_id)) {
            doc.model.use_pack(p);
        }
        self.elements.insert(old.to_string(), id.clone());
        self.order.push(old.to_string());
        Some(id)
    }

    /// Once every copy exists: owners (a copied owner maps to its copy, an owner still in the
    /// model is kept, anything else goes to the root) and tags (the next auto number when the
    /// scope numbers automatically, else none), in the order the copies were made so the numbers
    /// follow paste order.
    fn finish(&mut self, doc: &mut Document, clip: &HashMap<String, Element>) {
        for old in &self.order {
            let (Some(new), Some(src)) = (self.elements.get(old), clip.get(old)) else { continue };
            let owner = src.owner.as_ref().and_then(|o| self.elements.get(o).cloned().or_else(|| doc.model.element(o).map(|_| o.clone())));
            if let Some(o) = owner {
                // A cycle (only a hand-made clip can hold one) leaves the copy at the root.
                let _ = doc.model.set_owner(new, Some(&o));
            }
            let tag = doc.model.element(new).and_then(|e| auto_tag(doc, e).ok().flatten()).unwrap_or_default();
            if let Some(e) = doc.model.element_mut(new) {
                e.tag = tag;
            }
        }
    }

    /// The copy of clip relationship `old`, made on first use when both of its ends were copied;
    /// `None` otherwise (the connector is pasted as free drawing).
    fn relationship(&mut self, doc: &mut Document, clip: &HashMap<String, Relationship>, old: &str) -> Option<String> {
        if let Some(new) = self.relationships.get(old) {
            return Some(new.clone());
        }
        let src = clip.get(old)?;
        let source = self.elements.get(&src.source)?.clone();
        let target = self.elements.get(&src.target)?.clone();
        if src.type_id.is_empty() || src.type_id.len() > MAX_KEY {
            return None;
        }
        let id = doc.next_id("r");
        let mut r = Relationship::new(id.clone(), src.type_id.clone(), source, target);
        r.name = cut_text(&src.name, MAX_TEXT);
        r.properties = clean_props(&src.properties);
        r.source_port = src.source_port.as_ref().map(|p| cut_text(p, MAX_KEY)).filter(|p| !p.is_empty());
        r.target_port = src.target_port.as_ref().map(|p| cut_text(p, MAX_KEY)).filter(|p| !p.is_empty());
        doc.model.add_relationship(r).ok()?;
        if let Some(p) = cat().pack_ref(vizcraft_model::pack_of(&src.type_id)) {
            doc.model.use_pack(p);
        }
        self.relationships.insert(old.to_string(), id.clone());
        Some(id)
    }
}
