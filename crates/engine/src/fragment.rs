//! Selection fragments: a self-contained piece of a page (shapes, the connectors among them with
//! their glue, and the model items they draw) that can be taken out of one document and put into
//! another with fresh ids.
//!
//! The clipboard is a fragment in JSON form, `{"vizcraft": "clip", "shapes": [...], "connectors":
//! [...], "elements"?: [...], "relationships"?: [...]}`, with shapes and connectors in the
//! document's own form and the model items in the model's form (left out when there are none).
//! `edit.copy`, `edit.cut`, `edit.paste`, `edit.duplicate` and `edit.pasteSpecial` are built on
//! it.

use std::collections::HashMap;

use serde_json::{Value, json};
use vizcraft_diagram::{Connector, Document, End, Shape, resolve_ends};
use vizcraft_model::{Element, MAX_KEY, MAX_TEXT, Relationship};

use crate::cmd::MAX_IDS;
use crate::cmd::model::{auto_tag, cat, clean_props, cut as cut_text};
use crate::{EngineError, Session};

/// The marker a fragment's JSON form carries in its `vizcraft` key.
pub const CLIP_MARKER: &str = "clip";

/// A self-contained piece of a page. Connector ends glued to shapes inside the fragment keep their
/// glue; [`Fragment::extract`] frees every other glued end where it is drawn now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fragment {
    /// The shapes, in page order.
    pub shapes: Vec<Shape>,
    /// The connectors, in page order.
    pub connectors: Vec<Connector>,
    /// The model elements the shapes draw, each once.
    pub elements: Vec<Element>,
    /// The model relationships the connectors draw, each once.
    pub relationships: Vec<Relationship>,
}

impl Fragment {
    /// The fragment of the shapes and connectors on the current page whose ids `wanted` accepts.
    /// Connectors come only when chosen; an end glued to a shape that is not chosen becomes a free
    /// end where it is drawn now, so the fragment stands on its own.
    pub fn extract(doc: &Document, wanted: impl Fn(&str) -> bool) -> Fragment {
        let Some(page) = doc.page() else { return Fragment::default() };
        let shapes: Vec<Shape> = page.shapes.iter().filter(|x| wanted(&x.id)).cloned().collect();
        let inside = |id: Option<&str>| id.is_some_and(|i| shapes.iter().any(|x| x.id == i));
        let mut connectors = Vec::new();
        for c in page.connectors.iter().filter(|c| wanted(&c.id)) {
            let mut c = c.clone();
            let ends = resolve_ends(page, &c);
            if c.start.shape().is_some() && !inside(c.start.shape()) {
                c.start = End::free(ends.start.pos);
            }
            if c.end.shape().is_some() && !inside(c.end.shape()) {
                c.end = End::free(ends.end.pos);
            }
            connectors.push(c);
        }
        // The model items drawn by the chosen items travel with them, so an insert makes copies of
        // the elements instead of extra views of the originals.
        let model = &doc.model;
        let mut elements: Vec<Element> = Vec::new();
        for e in shapes.iter().filter_map(|x| x.element.as_deref()).filter_map(|e| model.element(e)) {
            if !elements.iter().any(|x| x.id == e.id) {
                elements.push(e.clone());
            }
        }
        let mut relationships: Vec<Relationship> = Vec::new();
        for r in connectors.iter().filter_map(|c| c.relationship.as_deref()).filter_map(|r| model.relationship(r)) {
            if !relationships.iter().any(|x| x.id == r.id) {
                relationships.push(r.clone());
            }
        }
        Fragment { shapes, connectors, elements, relationships }
    }

    /// The fragment of the session's selection.
    pub fn of_selection(s: &Session) -> Fragment {
        Fragment::of_items(&s.doc, &s.sel.items)
    }

    /// The fragment of these shapes and connectors on the current page; a group brings its
    /// members.
    pub fn of_items(doc: &Document, ids: &[String]) -> Fragment {
        let ids: std::collections::HashSet<String> =
            doc.page().map(|p| vizcraft_diagram::arrange::with_descendants(p, ids)).unwrap_or_default().into_iter().collect();
        Fragment::extract(doc, |id| ids.contains(id))
    }

    /// True when there is nothing to insert.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty() && self.connectors.is_empty()
    }

    /// The clipboard JSON form.
    pub fn to_json(&self) -> Value {
        let mut v = json!({"vizcraft": CLIP_MARKER, "shapes": self.shapes, "connectors": self.connectors});
        if let Some(o) = v.as_object_mut() {
            if !self.elements.is_empty() {
                o.insert("elements".into(), json!(self.elements));
            }
            if !self.relationships.is_empty() {
                o.insert("relationships".into(), json!(self.relationships));
            }
        }
        v
    }

    /// Read the clipboard JSON form. Entries that do not parse are skipped and each list is cut
    /// at [`MAX_IDS`], because the text may come from anywhere.
    pub fn from_json(v: &Value) -> Result<Fragment, EngineError> {
        if v.get("vizcraft").and_then(Value::as_str) != Some(CLIP_MARKER) {
            return Err(EngineError::Params("the clipboard does not hold VizCraft shapes".into()));
        }
        fn list<T: serde::de::DeserializeOwned>(v: &Value, k: &str) -> Vec<T> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().take(MAX_IDS).filter_map(|x| serde_json::from_value(x.clone()).ok()).collect())
                .unwrap_or_default()
        }
        Ok(Fragment {
            shapes: list(v, "shapes"),
            connectors: list(v, "connectors"),
            elements: list(v, "elements"),
            relationships: list(v, "relationships"),
        })
    }

    /// Insert a copy on the document's current page: fresh ids for every shape, connector, element
    /// and relationship; glue between items of the fragment remapped to the new ids; everything
    /// moved by `(dx, dy)`. A connector end glued to a shape outside the fragment keeps its glue
    /// when that shape is on the page, and the connector is dropped otherwise (only a hand-made
    /// fragment can do that). Returns the new shape ids, then the new connector ids.
    pub fn insert(&self, doc: &mut Document, dx: f64, dy: f64) -> Result<Vec<String>, EngineError> {
        if doc.page().is_none() {
            return Err(EngineError::Failed("the document has no page".into()));
        }
        let (dx, dy) = (finite(dx), finite(dy));
        let mut clip_elements: HashMap<String, &Element> = HashMap::new();
        for e in &self.elements {
            clip_elements.entry(e.id.clone()).or_insert(e);
        }
        let mut clip_relationships: HashMap<String, &Relationship> = HashMap::new();
        for r in &self.relationships {
            clip_relationships.entry(r.id.clone()).or_insert(r);
        }

        let mut map: HashMap<String, String> = HashMap::new();
        let mut copies = ModelCopies::default();
        let mut new_shapes = Vec::new();
        for sh in self.shapes.iter().take(MAX_IDS) {
            let mut sh = sh.clone();
            let id = doc.next_id("s");
            map.insert(std::mem::take(&mut sh.id), id.clone());
            sh.id = id;
            sh.x += dx;
            sh.y += dy;
            // A shape of an element the fragment carries draws a new copy of it; one whose element
            // the fragment does not carry (a hand-made clip, or a reference into another document)
            // is free drawing.
            sh.element = sh.element.take().and_then(|e| copies.element(doc, &clip_elements, &e));
            if let Some(e) = sh.element.as_deref().and_then(|e| doc.model.element(e)) {
                sh.text = e.name.clone();
            }
            sh.sanitize();
            new_shapes.push(sh);
        }
        // A parent group or container copied along keeps its members; a parent left behind does
        // not own the copy, so the link is cleared rather than pointing at the original.
        for sh in &mut new_shapes {
            sh.parent = sh.parent.as_deref().and_then(|p| map.get(p)).cloned();
            // A slot lists its members; the copies of those that came along replace them, and a member
            // left behind drops out rather than naming a shape the copy does not own.
            for comp in &mut sh.compartments {
                comp.members = comp.members.iter().filter_map(|m| map.get(m).cloned()).collect();
            }
            // A callout aims at the copy of its target when the target came along, else at the
            // original if it is on the page the copy lands on, else at nothing.
            if let Some(t) = sh.container.as_mut().and_then(|c| c.target.as_mut()) {
                match map.get(t.as_str()) {
                    Some(new) => *t = new.clone(),
                    None if doc.page().is_some_and(|p| p.shape(t).is_some()) => {}
                    None => {
                        if let Some(c) = sh.container.as_mut() {
                            c.target = None;
                        }
                    }
                }
            }
        }
        copies.finish(doc, &clip_elements);
        let page = doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        let mut kept = Vec::new();
        'conns: for c in self.connectors.iter().take(MAX_IDS) {
            let mut c = c.clone();
            for e in [&mut c.start, &mut c.end] {
                match e {
                    End::Glued { shape, point, .. } => {
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
                        *x += dx;
                        *y += dy;
                    }
                }
            }
            kept.push(c);
        }
        let mut new_connectors = Vec::new();
        for mut c in kept {
            c.id = doc.next_id("c");
            c.relationship = c.relationship.take().and_then(|r| copies.relationship(doc, &clip_relationships, &r));
            if let Some(r) = c.relationship.as_deref().and_then(|r| doc.model.relationship(r)) {
                c.text = r.name.clone();
            }
            c.sanitize();
            new_connectors.push(c);
        }
        let ids: Vec<String> = new_shapes.iter().map(|x| x.id.clone()).chain(new_connectors.iter().map(|c| c.id.clone())).collect();
        let page = doc.page_mut().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        page.shapes.extend(new_shapes);
        page.connectors.extend(new_connectors);
        Ok(ids)
    }
}

/// An offset that cannot make a coordinate infinite: non-finite becomes zero, the rest is clamped
/// to the largest move a command accepts.
fn finite(d: f64) -> f64 {
    if d.is_finite() { d.clamp(-crate::cmd::MAX_DELTA, crate::cmd::MAX_DELTA) } else { 0.0 }
}

/// The model copies one insert makes: each element and relationship of the fragment is copied at
/// most once (fresh id, same type, name and properties), however many inserted items draw it.
#[derive(Default)]
struct ModelCopies {
    /// Fragment element id to the new element's id.
    elements: HashMap<String, String>,
    /// Fragment relationship id to the new relationship's id.
    relationships: HashMap<String, String>,
    /// Fragment element ids in the order their copies were made.
    order: Vec<String>,
}

impl ModelCopies {
    /// The copy of fragment element `old`, made on first use; `None` when the fragment does not
    /// carry it or the model cannot take another element (the view is inserted as free drawing).
    fn element(&mut self, doc: &mut Document, clip: &HashMap<String, &Element>, old: &str) -> Option<String> {
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
    /// follow insert order.
    fn finish(&mut self, doc: &mut Document, clip: &HashMap<String, &Element>) {
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

    /// The copy of fragment relationship `old`, made on first use when both of its ends were
    /// copied; `None` otherwise (the connector is inserted as free drawing).
    fn relationship(&mut self, doc: &mut Document, clip: &HashMap<String, &Relationship>, old: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::Document;

    use crate::Session;

    #[test]
    fn a_pasted_slot_keeps_its_members() {
        let mut s = Session::new(Document::new());
        for (id, x) in [("k", 0), ("m", 20)] {
            s.run("shape.add", &json!({"stencil": "basic.rectangle", "id": id, "x": x, "y": 20, "w": 120, "h": 80})).unwrap();
        }
        s.run("compartment.add", &json!({"id": "k", "kind": "slot", "compartmentId": "slot", "members": ["m"]})).unwrap();
        s.run("edit.select", &json!({"ids": ["k"]})).unwrap();
        s.run("edit.copy", &json!({})).unwrap();
        let r = s.run("edit.paste", &json!({})).unwrap();
        let pasted: Vec<String> = r["pasted"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let page = s.doc.page().unwrap();
        let owner = pasted.iter().filter_map(|id| page.shape(id)).find(|sh| !sh.compartments.is_empty()).expect("the pasted slot");
        let members = &owner.compartments.iter().find(|c| c.id == "slot").unwrap().members;
        assert_eq!(members.len(), 1, "{members:?}");
        assert!(members[0] != "m" && pasted.contains(&members[0]), "the member is the pasted copy, not the original: {members:?}");
        assert_eq!(page.shape(&members[0]).unwrap().parent.as_deref(), Some(owner.id.as_str()));
        // The original slot is untouched.
        assert_eq!(page.shape("k").unwrap().compartments[0].members, ["m"]);
    }
}
