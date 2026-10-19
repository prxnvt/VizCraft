//! Re-import sync: the pure diff between the model and a model freshly read from a source file,
//! and the apply that brings the first in line with the second.
//!
//! [`diff`] matches every incoming element to at most one existing element, trying in order:
//!
//! 1. the record of the last import from the same source (incoming id to the model id it became);
//! 2. the same id;
//! 3. the same non-empty tag;
//! 4. the same name within the same type; an owned element (an ER column) only under the element
//!    its owner matched, so the `id` columns of two tables never cross-match.
//!
//! A matched element whose name, tag or properties differ is `updated`, else `kept`; an incoming
//! element with no match is `added`; an element the record names that nothing matched is
//! `removed`. Removed elements are reported, never deleted here: the caller flags them. An
//! element's type is never changed by a re-import (its views were drawn for the old type).
//!
//! Relationships are matched by type and mapped ends; one the record names that the source no
//! longer has is removed, since its connectors follow the relationships.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::{Element, MAX_ELEMENTS, MAX_PROPERTIES, MAX_RELATIONSHIPS, Model, ModelError, Relationship};

/// How an incoming element found its existing one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchedBy {
    /// The record of the last import from the same source.
    Record,
    /// The same id.
    Id,
    /// The same tag.
    Tag,
    /// The same name within the same type.
    Name,
}

/// What changes for relationships.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipChanges {
    /// Incoming relationship ids with no existing match.
    pub added: Vec<String>,
    /// Existing relationship ids matched by an incoming one.
    pub kept: Vec<String>,
    /// Existing relationship ids the record names that nothing matched.
    pub removed: Vec<String>,
    /// Incoming relationship id to the existing id it matched.
    pub map: BTreeMap<String, String>,
}

/// What a re-import changes: element ids by outcome and the matches.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    /// Incoming element ids with no existing match, in incoming order.
    pub added: Vec<String>,
    /// Existing element ids whose name, tag or properties change.
    pub updated: Vec<String>,
    /// Existing element ids the record names that no incoming element matched, in model order.
    pub removed: Vec<String>,
    /// Existing element ids matched with nothing to change.
    pub kept: Vec<String>,
    /// Incoming element id to the existing id it matched.
    pub map: BTreeMap<String, String>,
    /// How each matched incoming element was matched.
    pub how: BTreeMap<String, MatchedBy>,
    /// The relationships.
    pub relationships: RelationshipChanges,
}

impl Changes {
    /// The four counts: added, updated, removed, kept.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        (self.added.len(), self.updated.len(), self.removed.len(), self.kept.len())
    }
}

/// Whether applying `inc` over `cur` would change it: a different name, a new non-empty tag, or
/// a property the incoming element sets to another value.
fn differs(cur: &Element, inc: &Element) -> bool {
    cur.name != inc.name || (!inc.tag.is_empty() && cur.tag != inc.tag) || inc.properties.iter().any(|(k, v)| cur.properties.get(k) != Some(v))
}

/// Match `incoming` against `existing`. `known` is the record of the last import from the same
/// source (incoming id to model id) and `known_relationships` the relationship ids it made.
pub fn diff(existing: &Model, incoming: &Model, known: &BTreeMap<String, String>, known_relationships: &[String]) -> Changes {
    let mut ch = Changes::default();
    let mut taken: HashSet<&str> = HashSet::new();
    let elements: Vec<&Element> = incoming.elements.iter().take(MAX_ELEMENTS).collect();
    let mut by_tag: HashMap<&str, Vec<&Element>> = HashMap::new();
    let mut by_name: HashMap<(&str, &str), Vec<&Element>> = HashMap::new();
    for e in &existing.elements {
        if !e.tag.is_empty() {
            by_tag.entry(e.tag.as_str()).or_default().push(e);
        }
        if !e.name.trim().is_empty() {
            by_name.entry((e.type_id.as_str(), e.name.trim())).or_default().push(e);
        }
    }
    // Each pass gives the elements still unmatched their best remaining candidate, so a record
    // or id match is never taken away by a weaker tag or name match.
    for pass in [MatchedBy::Record, MatchedBy::Id, MatchedBy::Tag, MatchedBy::Name] {
        for inc in &elements {
            if ch.map.contains_key(&inc.id) {
                continue;
            }
            let free = |e: &&Element| !taken.contains(e.id.as_str());
            let found: Option<&Element> = match pass {
                MatchedBy::Record => known.get(&inc.id).and_then(|id| existing.element(id)).filter(free),
                MatchedBy::Id => existing.element(&inc.id).filter(free),
                MatchedBy::Tag if !inc.tag.is_empty() => by_tag.get(inc.tag.as_str()).and_then(|v| v.iter().copied().find(free)),
                MatchedBy::Name if !inc.name.trim().is_empty() => {
                    // An owned element matches only under its owner's match (owners come first).
                    let owner = inc.owner.as_ref().map(|o| ch.map.get(o));
                    let under = |e: &&Element| match &owner {
                        None => true,
                        Some(mapped) => mapped.is_some() && e.owner.as_ref() == *mapped,
                    };
                    by_name.get(&(inc.type_id.as_str(), inc.name.trim())).and_then(|v| v.iter().copied().find(|e| free(e) && under(e)))
                }
                MatchedBy::Tag | MatchedBy::Name => None,
            };
            if let Some(e) = found {
                taken.insert(e.id.as_str());
                ch.map.insert(inc.id.clone(), e.id.clone());
                ch.how.insert(inc.id.clone(), pass);
            }
        }
    }
    for inc in &elements {
        match ch.map.get(&inc.id).and_then(|id| existing.element(id)) {
            Some(cur) if differs(cur, inc) => ch.updated.push(cur.id.clone()),
            Some(cur) => ch.kept.push(cur.id.clone()),
            None => ch.added.push(inc.id.clone()),
        }
    }
    let recorded: HashSet<&str> = known.values().map(String::as_str).collect();
    ch.removed =
        existing.elements.iter().filter(|e| recorded.contains(e.id.as_str()) && !taken.contains(e.id.as_str())).map(|e| e.id.clone()).collect();

    // Relationships: same type between the mapped ends. Ends that are added keep their incoming
    // id here; they cannot match an existing relationship anyway.
    let mut rel_taken: HashSet<&str> = HashSet::new();
    let mut by_ends: HashMap<(&str, &str, &str), Vec<&Relationship>> = HashMap::new();
    for r in &existing.relationships {
        by_ends.entry((r.type_id.as_str(), r.source.as_str(), r.target.as_str())).or_default().push(r);
    }
    for inc in incoming.relationships.iter().take(MAX_RELATIONSHIPS) {
        let (Some(s), Some(t)) = (ch.map.get(&inc.source), ch.map.get(&inc.target)) else {
            ch.relationships.added.push(inc.id.clone());
            continue;
        };
        let found = by_ends.get(&(inc.type_id.as_str(), s.as_str(), t.as_str())).and_then(|v| v.iter().find(|r| !rel_taken.contains(r.id.as_str())));
        match found {
            Some(r) => {
                rel_taken.insert(r.id.as_str());
                ch.relationships.kept.push(r.id.clone());
                ch.relationships.map.insert(inc.id.clone(), r.id.clone());
            }
            None => ch.relationships.added.push(inc.id.clone()),
        }
    }
    ch.relationships.removed =
        known_relationships.iter().filter(|id| existing.relationship(id).is_some() && !rel_taken.contains(id.as_str())).cloned().collect();
    ch
}

/// What [`apply`] did: every incoming id to the model id it now is.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    /// Incoming element id to model element id, matched and added.
    pub elements: BTreeMap<String, String>,
    /// Incoming relationship id to model relationship id, matched and added.
    pub relationships: BTreeMap<String, String>,
    /// Ids of the relationships removed.
    pub removed_relationships: Vec<String>,
}

/// Apply `changes` (from [`diff`] over the same two models) to `model`: matched elements take the
/// incoming name, a non-empty incoming tag and the incoming properties; added elements and
/// relationships go in under the ids `ids` gives them (incoming id to new id; an incoming id
/// missing from it is used as it is); relationships the source no longer has are removed.
/// Removed elements stay. Owners follow the mapping, and an owner that maps to nothing is dropped.
pub fn apply(model: &mut Model, incoming: &Model, changes: &Changes, ids: &BTreeMap<String, String>) -> Result<Applied, ModelError> {
    let mut out = Applied { elements: changes.map.clone(), ..Applied::default() };
    let new_id = |id: &str| ids.get(id).cloned().unwrap_or_else(|| id.to_string());
    let added: HashSet<&str> = changes.added.iter().map(String::as_str).collect();
    for inc in incoming.elements.iter().filter(|e| added.contains(e.id.as_str())) {
        out.elements.insert(inc.id.clone(), new_id(&inc.id));
    }
    for inc in incoming.elements.iter().take(MAX_ELEMENTS) {
        if let Some(cur) = changes.map.get(&inc.id).and_then(|id| model.element_mut(id)) {
            cur.name = inc.name.clone();
            if !inc.tag.is_empty() {
                cur.tag = inc.tag.clone();
            }
            for (k, v) in &inc.properties {
                if cur.properties.len() < MAX_PROPERTIES || cur.properties.contains_key(k) {
                    cur.properties.insert(k.clone(), v.clone());
                }
            }
        }
    }
    // Owners may come later in the incoming order, so elements go in first and owners after.
    for inc in incoming.elements.iter().filter(|e| added.contains(e.id.as_str())) {
        let mut e = inc.clone();
        e.id = new_id(&inc.id);
        e.owner = None;
        model.add_element(e)?;
    }
    for inc in incoming.elements.iter().take(MAX_ELEMENTS) {
        let Some(owner) = inc.owner.as_ref().and_then(|o| out.elements.get(o)) else { continue };
        let Some(id) = out.elements.get(&inc.id) else { continue };
        // A cycle or a missing owner leaves the element where it is.
        let _ = model.set_owner(id, Some(owner));
    }
    let gone: Vec<String> = changes.relationships.removed.clone();
    let removed = model.remove(&gone);
    out.removed_relationships = removed.relationships;
    out.relationships = changes.relationships.map.clone();
    for inc in incoming.relationships.iter().take(MAX_RELATIONSHIPS) {
        if let Some(cur) = changes.relationships.map.get(&inc.id).and_then(|id| model.relationship_mut(id)) {
            cur.name = inc.name.clone();
        }
    }
    let rel_added: HashSet<&str> = changes.relationships.added.iter().map(String::as_str).collect();
    for inc in incoming.relationships.iter().filter(|r| rel_added.contains(r.id.as_str())) {
        let (Some(s), Some(t)) = (out.elements.get(&inc.source), out.elements.get(&inc.target)) else { continue };
        let mut r = inc.clone();
        r.id = new_id(&inc.id);
        r.source = s.clone();
        r.target = t.clone();
        out.relationships.insert(inc.id.clone(), r.id.clone());
        model.add_relationship(r)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PropValue;

    fn el(id: &str, ty: &str, name: &str) -> Element {
        let mut e = Element::new(id, ty);
        e.name = name.into();
        e
    }

    fn model(els: Vec<Element>, rels: Vec<Relationship>) -> Model {
        let mut m = Model::new();
        m.elements = els;
        m.relationships = rels;
        m
    }

    #[test]
    fn matches_by_record_id_tag_then_name() {
        let mut tagged = el("e9", "t.A", "Pump");
        tagged.tag = "P-1".into();
        let cur = model(vec![el("e1", "t.A", "One"), el("B", "t.A", "Bee"), tagged, el("e4", "t.A", "Named")], vec![]);
        let mut inc_tagged = el("x", "t.A", "Pump two");
        inc_tagged.tag = "P-1".into();
        let inc = model(vec![el("A", "t.A", "One"), el("B", "t.A", "Bee"), inc_tagged, el("y", "t.A", "Named"), el("z", "t.A", "New")], vec![]);
        let known = BTreeMap::from([("A".to_string(), "e1".to_string())]);
        let ch = diff(&cur, &inc, &known, &[]);
        assert_eq!(ch.map["A"], "e1");
        assert_eq!(ch.how["A"], MatchedBy::Record);
        assert_eq!(ch.how["B"], MatchedBy::Id);
        assert_eq!((ch.map["x"].as_str(), ch.how["x"]), ("e9", MatchedBy::Tag));
        assert_eq!((ch.map["y"].as_str(), ch.how["y"]), ("e4", MatchedBy::Name));
        assert_eq!(ch.added, vec!["z".to_string()]);
        assert_eq!(ch.updated, vec!["e9".to_string()]);
        assert_eq!(ch.kept.len(), 3);
        assert!(ch.removed.is_empty());
    }

    #[test]
    fn names_match_within_the_type_only_and_each_existing_element_once() {
        let cur = model(vec![el("e1", "t.A", "Same")], vec![]);
        let inc = model(vec![el("p", "t.B", "Same"), el("q", "t.A", "Same"), el("r", "t.A", "Same")], vec![]);
        let ch = diff(&cur, &inc, &BTreeMap::new(), &[]);
        assert_eq!(ch.map.len(), 1);
        assert_eq!(ch.map["q"], "e1");
        assert_eq!(ch.added, vec!["p".to_string(), "r".to_string()]);
    }

    #[test]
    fn owned_elements_match_by_name_only_under_their_owners_match() {
        let owned = |id: &str, name: &str, owner: &str| Element { owner: Some(owner.into()), ..el(id, "t.Col", name) };
        let cur = model(vec![el("T1", "t.Table", "users"), el("T2", "t.Table", "orders"), owned("c1", "id", "T1"), owned("c2", "id", "T2")], vec![]);
        let inc = model(vec![el("a", "t.Table", "orders"), owned("a.id", "id", "a"), el("b", "t.Table", "carts"), owned("b.id", "id", "b")], vec![]);
        let ch = diff(&cur, &inc, &BTreeMap::new(), &[]);
        assert_eq!(ch.map["a"], "T2");
        assert_eq!(ch.map["a.id"], "c2", "the orders id column, not the users one");
        assert!(!ch.map.contains_key("b.id"), "a column of an unmatched table is new");
        assert_eq!(ch.added, vec!["b".to_string(), "b.id".to_string()]);
    }

    #[test]
    fn recorded_elements_absent_from_the_source_are_removed_and_stay() {
        let cur = model(vec![el("A", "t.A", "a"), el("B", "t.A", "b"), el("C", "t.A", "c")], vec![Relationship::new("r1", "t.F", "A", "B")]);
        let inc = model(vec![el("A", "t.A", "a"), el("B", "t.A", "bee")], vec![]);
        let known: BTreeMap<String, String> = ["A", "B", "C"].iter().map(|k| (k.to_string(), k.to_string())).collect();
        let ch = diff(&cur, &inc, &known, &["r1".to_string()]);
        assert_eq!(ch.removed, vec!["C".to_string()]);
        assert_eq!(ch.relationships.removed, vec!["r1".to_string()]);
        let mut m = cur.clone();
        let out = apply(&mut m, &inc, &ch, &BTreeMap::new()).unwrap();
        assert!(m.element("C").is_some(), "a removed element is flagged, not deleted");
        assert_eq!(m.element("B").unwrap().name, "bee");
        assert!(m.relationships.is_empty());
        assert_eq!(out.removed_relationships, vec!["r1".to_string()]);
    }

    #[test]
    fn apply_adds_under_the_given_ids_and_maps_relationship_ends() {
        let cur = model(vec![el("A", "t.A", "a")], vec![]);
        let mut inc_b = el("B", "t.A", "b");
        inc_b.properties.insert("k".into(), PropValue::Text("v".into()));
        let inc =
            model(vec![el("A1", "t.A", "a"), inc_b], vec![Relationship::new("x", "t.F", "A1", "B"), Relationship::new("y", "t.F", "B", "nope")]);
        let known = BTreeMap::from([("A1".to_string(), "A".to_string())]);
        let ch = diff(&cur, &inc, &known, &[]);
        let ids = BTreeMap::from([("B".to_string(), "e7".to_string()), ("x".to_string(), "r8".to_string())]);
        let mut m = cur.clone();
        let out = apply(&mut m, &inc, &ch, &ids).unwrap();
        assert_eq!(out.elements["B"], "e7");
        assert_eq!(m.element("e7").unwrap().properties["k"], PropValue::Text("v".into()));
        let r = m.relationship("r8").unwrap();
        assert_eq!((r.source.as_str(), r.target.as_str()), ("A", "e7"));
        assert_eq!(m.relationships.len(), 1, "a relationship to an unknown end is skipped");
        // Applying the same import again changes nothing.
        let known: BTreeMap<String, String> = out.elements.clone();
        let again = diff(&m, &inc, &known, &out.relationships.values().cloned().collect::<Vec<_>>());
        assert!(again.added.is_empty() && again.updated.is_empty() && again.removed.is_empty());
        assert_eq!(again.relationships.kept, vec!["r8".to_string()]);
    }
}
