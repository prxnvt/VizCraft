//! The model types: elements, relationships, containment and views.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::serde_util::{lenient, lenient_map, lenient_vec};
use crate::{MAX_DEPTH, MAX_ELEMENTS, MAX_RELATIONSHIPS, SCHEMA_VERSION};

/// A property value: a flag, a number or text. Enumerations are text naming one of the values
/// the property's schema allows. On disk it is the plain JSON value (`true`, `4.5`, `"open"`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropValue {
    /// `true` or `false`.
    Bool(bool),
    /// A finite number.
    Number(f64),
    /// Text, or an enumeration value.
    Text(String),
}

impl PropValue {
    /// The text, when this is text.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PropValue::Text(t) => Some(t),
            _ => None,
        }
    }
    /// The number, when this is a number.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PropValue::Number(n) => Some(*n),
            _ => None,
        }
    }
    /// The flag, when this is a flag.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PropValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
    /// The value as display text (`true`, `4.5`, `open`), used when composing identifiers.
    pub fn display(&self) -> String {
        match self {
            PropValue::Bool(b) => b.to_string(),
            PropValue::Number(n) => n.to_string(),
            PropValue::Text(t) => t.clone(),
        }
    }
}

/// A notation pack the model uses, by id and the version it was built with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PackRef {
    /// Pack id, the type prefix (`flowchart`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Pack version (`1.0.0`).
    #[serde(deserialize_with = "lenient")]
    pub version: String,
}

/// A typed thing in the model: a process step, a pump, a container, a register.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Element {
    /// Stable id, unique across the whole document (shapes, connectors, pages and model items
    /// share one id space): caller-chosen or handed out by `Document::next_id` (`e7`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Pack-qualified type (`flowchart.Decision`).
    #[serde(rename = "type", deserialize_with = "lenient")]
    pub type_id: String,
    /// Human name; every view of the element shows it as its text.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Identifier from the identity registry (`S3`, `FIC-101`); empty when the element has none.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub tag: String,
    /// The owning element in the containment tree; `None` means the project root.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Typed properties, checked against the type's schema by the rules engine.
    #[serde(deserialize_with = "lenient_map", skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, PropValue>,
}

impl Element {
    /// An element of `type_id` with no name, tag, owner or properties.
    pub fn new(id: impl Into<String>, type_id: impl Into<String>) -> Self {
        Element { id: id.into(), type_id: type_id.into(), ..Element::default() }
    }
    /// The pack prefix of the type (`flowchart`).
    pub fn pack(&self) -> &str {
        crate::pack_of(&self.type_id)
    }
    /// The property `key`, if set.
    pub fn prop(&self, key: &str) -> Option<&PropValue> {
        self.properties.get(key)
    }
}

/// A typed, directed link between two elements: a flow, a signal, a dependency.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Relationship {
    /// Stable id in the document's shared id space (`r8`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Pack-qualified type (`flowchart.Flow`).
    #[serde(rename = "type", deserialize_with = "lenient")]
    pub type_id: String,
    /// Source element id.
    #[serde(deserialize_with = "lenient")]
    pub source: String,
    /// Target element id.
    #[serde(deserialize_with = "lenient")]
    pub target: String,
    /// Port on the source element, when its type declares ports.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub source_port: Option<String>,
    /// Port on the target element, when its type declares ports.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub target_port: Option<String>,
    /// Human name; every connector view shows it as its label.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Typed properties.
    #[serde(deserialize_with = "lenient_map", skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, PropValue>,
}

impl Relationship {
    /// A relationship of `type_id` from `source` to `target`.
    pub fn new(id: impl Into<String>, type_id: impl Into<String>, source: impl Into<String>, target: impl Into<String>) -> Self {
        Relationship { id: id.into(), type_id: type_id.into(), source: source.into(), target: target.into(), ..Relationship::default() }
    }
    /// The pack prefix of the type (`flowchart`).
    pub fn pack(&self) -> &str {
        crate::pack_of(&self.type_id)
    }
    /// The property `key`, if set.
    pub fn prop(&self, key: &str) -> Option<&PropValue> {
        self.properties.get(key)
    }
}

/// What a diagram page shows of the model: the viewpoint (a pack's named filter of allowed
/// element and relationship types) and the layout the page was last arranged with. A page
/// carries it as `Page.view`; the geometry is in the page's shapes, keyed by their element.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct View {
    /// Pack-qualified viewpoint id (`flowchart.Flowchart`); empty means any type is allowed.
    #[serde(deserialize_with = "lenient")]
    pub viewpoint: String,
    /// Layout algorithm last applied (`layered`), empty when arranged by hand.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub layout: String,
    /// Pack-qualified rendering variant the page draws its elements with (`er.chen`); empty is
    /// the pack's default drawing. Capped at [`View::MAX_VARIANT`] bytes when read.
    #[serde(deserialize_with = "capped_variant", skip_serializing_if = "String::is_empty")]
    pub variant: String,
}

impl View {
    /// Longest variant id kept; real ids are a few words.
    pub const MAX_VARIANT: usize = 256;
}

/// A variant id read leniently and cut to [`View::MAX_VARIANT`] bytes at a char boundary, so a
/// hostile file cannot make the page carry a huge id.
fn capped_variant<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let mut v: String = lenient(d)?;
    crate::repair::truncate(&mut v, View::MAX_VARIANT);
    Ok(v)
}

/// Why a model edit was refused.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// No element (or relationship) with this id.
    #[error("no element or relationship `{0}`")]
    Unknown(String),
    /// The id is already used by another model item.
    #[error("`{0}` is already used")]
    DuplicateId(String),
    /// The id is empty, too long or not plain.
    #[error("`{0}` is not a valid id")]
    InvalidId(String),
    /// The model is full.
    #[error("the model already holds the most {0} it can")]
    TooMany(&'static str),
    /// The move would make an element its own ancestor.
    #[error("`{0}` cannot be moved inside itself")]
    Cycle(String),
    /// An identifier format that does not parse; the text says why.
    #[error("bad identifier format: {0}")]
    Format(String),
}

/// What [`Model::remove`] took out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Removed {
    /// Element ids, in removal order.
    pub elements: Vec<String>,
    /// Relationship ids, in removal order.
    pub relationships: Vec<String>,
}

/// The semantic model of one document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Model {
    /// Schema version of this model section ([`SCHEMA_VERSION`] when written by this build).
    #[serde(deserialize_with = "lenient")]
    pub schema_version: u32,
    /// Notation packs the model uses.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<PackRef>,
    /// Elements in creation order.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<Element>,
    /// Relationships in creation order.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,
}

impl Default for Model {
    fn default() -> Self {
        Model::new()
    }
}

impl Model {
    /// An empty model at the current schema version.
    pub fn new() -> Self {
        Model { schema_version: SCHEMA_VERSION, packs: Vec::new(), elements: Vec::new(), relationships: Vec::new() }
    }

    /// True when the model has no packs, elements or relationships (a free-drawing document).
    pub fn is_empty(&self) -> bool {
        self.packs.is_empty() && self.elements.is_empty() && self.relationships.is_empty()
    }

    /// Most notation packs a model lists; a file with more keeps the first ones.
    pub const MAX_PACKS: usize = 256;

    /// Make the model safe to work on: cap counts ([`MAX_ELEMENTS`], [`MAX_RELATIONSHIPS`],
    /// `MAX_PROPERTIES`) and lengths (`MAX_ID`, `MAX_KEY`, `MAX_TAG`, `MAX_TEXT`), drop items with
    /// an empty or duplicate id, drop relationships whose ends are missing, clear owners that are
    /// missing, and break ownership cycles and chains deeper than `MAX_DEPTH` by moving the
    /// offending element to the root. Idempotent.
    pub fn sanitize(&mut self) {
        crate::repair::sanitize(self);
    }

    /// The element with this id.
    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    /// The element with this id, mutably.
    pub fn element_mut(&mut self, id: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    /// The relationship with this id.
    pub fn relationship(&self, id: &str) -> Option<&Relationship> {
        self.relationships.iter().find(|r| r.id == id)
    }

    /// The relationship with this id, mutably.
    pub fn relationship_mut(&mut self, id: &str) -> Option<&mut Relationship> {
        self.relationships.iter_mut().find(|r| r.id == id)
    }

    /// Whether an element or relationship has this id.
    pub fn contains_id(&self, id: &str) -> bool {
        self.element(id).is_some() || self.relationship(id).is_some()
    }

    /// Relationships whose source is `element`.
    pub fn outgoing<'a>(&'a self, element: &'a str) -> impl Iterator<Item = &'a Relationship> + 'a {
        self.relationships.iter().filter(move |r| r.source == element)
    }

    /// Relationships whose target is `element`.
    pub fn incoming<'a>(&'a self, element: &'a str) -> impl Iterator<Item = &'a Relationship> + 'a {
        self.relationships.iter().filter(move |r| r.target == element)
    }

    /// Elements directly owned by `owner` (`None`: the elements at the project root).
    pub fn children<'a>(&'a self, owner: Option<&'a str>) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements.iter().filter(move |e| e.owner.as_deref() == owner)
    }

    /// Whether the model uses the pack `id`.
    pub fn uses_pack(&self, id: &str) -> bool {
        self.packs.iter().any(|p| p.id == id)
    }

    /// Record that the model uses `pack` (no change when it is listed already).
    pub fn use_pack(&mut self, pack: PackRef) {
        if !self.uses_pack(&pack.id) {
            self.packs.push(pack);
        }
    }

    /// Add an element. Refused when its id is empty, longer than `MAX_ID` or already used by
    /// another model item, when its owner is missing, or when the model is full. The caller
    /// checks the id against the diagram's ids (`Document::contains_id`).
    pub fn add_element(&mut self, e: Element) -> Result<(), ModelError> {
        if e.id.is_empty() || e.id.len() > crate::MAX_ID {
            return Err(ModelError::InvalidId(e.id.chars().take(crate::MAX_ID).collect()));
        }
        if self.contains_id(&e.id) {
            return Err(ModelError::DuplicateId(e.id));
        }
        if let Some(owner) = &e.owner
            && self.element(owner).is_none()
        {
            return Err(ModelError::Unknown(owner.clone()));
        }
        if self.elements.len() >= MAX_ELEMENTS {
            return Err(ModelError::TooMany("elements"));
        }
        self.elements.push(e);
        Ok(())
    }

    /// Add a relationship. Refused when its id is empty, too long or already used, when either end
    /// is not an element, or when the model is full. Legality against a pack's connection table is
    /// not checked here: an illegal relationship is kept and reported by the rules engine.
    pub fn add_relationship(&mut self, r: Relationship) -> Result<(), ModelError> {
        if r.id.is_empty() || r.id.len() > crate::MAX_ID {
            return Err(ModelError::InvalidId(r.id.chars().take(crate::MAX_ID).collect()));
        }
        if self.contains_id(&r.id) {
            return Err(ModelError::DuplicateId(r.id));
        }
        for end in [&r.source, &r.target] {
            if self.element(end).is_none() {
                return Err(ModelError::Unknown(end.clone()));
            }
        }
        if self.relationships.len() >= MAX_RELATIONSHIPS {
            return Err(ModelError::TooMany("relationships"));
        }
        self.relationships.push(r);
        Ok(())
    }

    /// Ids of every element owned by `id`, directly or through other elements, parents before
    /// their children; bounded by `MAX_DEPTH` and safe on cyclic input.
    pub fn descendants(&self, id: &str) -> Vec<String> {
        let index = crate::repair::ChildIndex::new(self);
        index
            .subtree(self, &[id], Some(MAX_DEPTH))
            .into_iter()
            .skip(1)
            .map(|i| self.elements.get(i).map_or_else(String::new, |e| e.id.clone()))
            .collect()
    }

    /// Owners of `id` from its parent up to the root; bounded by `MAX_DEPTH` and safe on cyclic
    /// input.
    pub fn ancestors(&self, id: &str) -> Vec<String> {
        let by_id = crate::repair::by_id(self);
        let mut out = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        seen.insert(id);
        let mut cur = by_id.get(id).and_then(|&i| self.elements.get(i));
        while let Some(e) = cur {
            let Some(owner) = e.owner.as_deref() else { break };
            // A cycle or a dangling owner ends the walk: the first repeated or missing owner is
            // where the chain stops making sense.
            if out.len() >= MAX_DEPTH || !seen.insert(owner) {
                break;
            }
            let Some(next) = by_id.get(owner).and_then(|&i| self.elements.get(i)) else { break };
            out.push(next.id.clone());
            cur = Some(next);
        }
        out
    }

    /// Move `id` under `owner` (`None`: to the project root). Refused when either is missing or
    /// when `owner` is `id` or one of its descendants.
    pub fn set_owner(&mut self, id: &str, owner: Option<&str>) -> Result<(), ModelError> {
        let by_id = crate::repair::by_id(self);
        if !by_id.contains_key(id) {
            return Err(ModelError::Unknown(id.to_string()));
        }
        if let Some(o) = owner {
            if !by_id.contains_key(o) {
                return Err(ModelError::Unknown(o.to_string()));
            }
            // Walk up from the new owner rather than down from `id`: the walk is as long as one
            // chain, not the whole subtree, and it is not cut by the depth bound, so even an
            // unsanitized deep tree cannot slip a cycle past the check.
            let mut seen: HashSet<&str> = HashSet::new();
            let mut cur = Some(o);
            while let Some(c) = cur {
                if c == id {
                    return Err(ModelError::Cycle(id.to_string()));
                }
                if !seen.insert(c) {
                    break;
                }
                cur = by_id.get(c).and_then(|&i| self.elements.get(i)).and_then(|e| e.owner.as_deref());
            }
        }
        let owner = owner.map(str::to_string);
        let e = self.element_mut(id).ok_or_else(|| ModelError::Unknown(id.to_string()))?;
        e.owner = owner;
        Ok(())
    }

    /// Delete from the model: each element in `ids` with every element it owns (descendants), every
    /// relationship touching a removed element, and each relationship named in `ids`. Unknown ids
    /// are skipped. Removing view items that draw them is the caller's job (the diagram document).
    pub fn remove(&mut self, ids: &[String]) -> Removed {
        let mut out = Removed::default();
        // The cascade is not cut at the depth bound: an unsanitized deeper tree must not leave
        // owned elements behind pointing at a removed owner.
        let doomed: Vec<usize> = {
            let index = crate::repair::ChildIndex::new(self);
            let roots: Vec<&str> = ids.iter().map(String::as_str).collect();
            index.subtree(self, &roots, None)
        };
        let mut gone_idx = vec![false; self.elements.len()];
        for &i in &doomed {
            if let (Some(flag), Some(e)) = (gone_idx.get_mut(i), self.elements.get(i)) {
                *flag = true;
                out.elements.push(e.id.clone());
            }
        }
        let gone: HashSet<String> = out.elements.iter().cloned().collect();
        let named: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let mut flags = gone_idx.into_iter();
        self.elements.retain(|_| !flags.next().unwrap_or(false));
        for e in &mut self.elements {
            if e.owner.as_ref().is_some_and(|o| gone.contains(o)) {
                e.owner = None;
            }
        }
        let mut kept = Vec::with_capacity(self.relationships.len());
        for r in std::mem::take(&mut self.relationships) {
            if named.contains(r.id.as_str()) || gone.contains(&r.source) || gone.contains(&r.target) {
                out.relationships.push(r.id);
            } else {
                kept.push(r);
            }
        }
        self.relationships = kept;
        out
    }
}
