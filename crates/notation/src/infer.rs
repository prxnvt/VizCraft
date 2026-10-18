//! The reasoner: facts derived from the model and the packs before any rule runs.
//!
//! A [`Facts`] index is built once per rules run. It answers what the plain model does not say
//! directly:
//!
//! - the type hierarchy, from `ElementType.extends` (a Pump is an Equipment, so a rule or a
//!   connection row that lists Equipment covers a Pump, and a Pump inherits the properties and
//!   ports it does not declare);
//! - transitive containment (A inside B inside C means A is inside C);
//! - inverse relationships, from `RelationshipType.inverse_of` (A `hasPart` B is also B `partOf`
//!   A wherever a rule follows `partOf`);
//! - reachability along a relationship type, inverses included.
//!
//! Every walk is bounded: supertype chains stop at [`MAX_CHAIN`] and on a repeat, containment
//! walks at [`MAX_DEPTH`], and graph walks visit each element once.

use std::collections::{HashMap, HashSet, VecDeque};

use vizcraft_model::{Element, Model, Relationship};

use crate::Catalog;
use crate::schema::{ElementType, Pack, Port, PropertySchema};

/// Longest supertype chain followed (a type and its ancestors).
pub const MAX_CHAIN: usize = 32;

/// Deepest containment walked from one element.
pub const MAX_DEPTH: usize = 256;

/// One followed relationship: from, to, and the relationship's id.
pub type Edge<'a> = (&'a str, &'a str, &'a str);

/// Facts about one model under one catalog.
pub struct Facts<'a> {
    cat: &'a Catalog,
    model: &'a Model,
    /// Every element type present in the model, with its supertypes: `[type, super, ...]`.
    chains: HashMap<&'a str, Vec<&'a str>>,
    /// Elements by id (the first wins in a damaged model).
    elements: HashMap<&'a str, &'a Element>,
}

impl<'a> Facts<'a> {
    /// Index `model` under `cat`.
    pub fn new(model: &'a Model, cat: &'a Catalog) -> Self {
        let mut chains = HashMap::new();
        let mut elements = HashMap::with_capacity(model.elements.len());
        for e in &model.elements {
            elements.entry(e.id.as_str()).or_insert(e);
            chains.entry(e.type_id.as_str()).or_insert_with(|| chain_of(cat, e.type_id.as_str()));
        }
        Facts { cat, model, chains, elements }
    }

    /// `type_id` and its supertypes, nearest first. A type not in the model is looked up now.
    pub fn chain(&self, type_id: &'a str) -> Vec<&'a str> {
        match self.chains.get(type_id) {
            Some(c) => c.clone(),
            None => chain_of(self.cat, type_id),
        }
    }

    /// Calls `f` on `type_id` and each supertype, nearest first, until it returns `true`.
    fn any_in_chain(&self, type_id: &str, mut f: impl FnMut(&str) -> bool) -> bool {
        match self.chains.get(type_id) {
            Some(c) => c.iter().any(|t| f(t)),
            None => chain_of(self.cat, type_id).iter().any(|t| f(t)),
        }
    }

    /// Whether `type_id` is `ancestor` or one of its subtypes.
    pub fn is_a(&self, type_id: &str, ancestor: &str) -> bool {
        self.any_in_chain(type_id, |t| t == ancestor)
    }

    /// Whether `list` covers `type_id` in `pack`: it names the type or a supertype, or holds `*`
    /// and the type or a supertype is an element type of `pack`.
    pub fn lists(&self, pack: &Pack, list: &[String], type_id: &str) -> bool {
        let star = list.iter().any(|t| t == "*");
        self.any_in_chain(type_id, |t| list.iter().any(|l| l == t) || (star && pack.element_type(t).is_some()))
    }

    /// Whether `type_id`, or a supertype, is an element type of `pack`.
    pub fn of_pack(&self, type_id: &str, pack: &Pack) -> bool {
        self.any_in_chain(type_id, |t| pack.element_type(t).is_some())
    }

    /// The element type declared for `type_id`, if any pack in the catalog declares it.
    pub fn element_type(&self, type_id: &str) -> Option<&'a ElementType> {
        self.cat.element_type(type_id).map(|(_, t)| t)
    }

    /// The property schema of `type_id`: its own properties, then those of each supertype that
    /// it does not redeclare.
    pub fn properties(&self, type_id: &str) -> Vec<&'a PropertySchema> {
        let mut out: Vec<&PropertySchema> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        self.any_in_chain(type_id, |t| {
            if let Some(et) = self.element_type(t) {
                for p in &et.properties {
                    if seen.insert(p.key.as_str()) {
                        out.push(p);
                    }
                }
            }
            false
        });
        out
    }

    /// The ports of `type_id`: those of the nearest type in its chain that declares any.
    pub fn ports(&self, type_id: &str) -> &'a [Port] {
        let mut found: &[Port] = &[];
        self.any_in_chain(type_id, |t| match self.element_type(t) {
            Some(et) if !et.ports.is_empty() => {
                found = &et.ports;
                true
            }
            _ => false,
        });
        found
    }

    /// The element with this id.
    pub fn element(&self, id: &str) -> Option<&'a Element> {
        self.elements.get(id).copied()
    }

    /// The owners of `id`, nearest first, bounded by [`MAX_DEPTH`] and stopping on a repeat.
    pub fn ancestors(&self, id: &str) -> Vec<&'a str> {
        let mut out: Vec<&str> = Vec::new();
        let mut at = self.element(id).and_then(|e| e.owner.as_deref());
        while let Some(o) = at {
            let Some(owner) = self.element(o) else { break };
            if out.len() >= MAX_DEPTH || out.contains(&owner.id.as_str()) || owner.id == id {
                break;
            }
            out.push(owner.id.as_str());
            at = owner.owner.as_deref();
        }
        out
    }

    /// Whether `id` is inside `ancestor`, at any depth.
    pub fn is_inside(&self, id: &str, ancestor: &str) -> bool {
        self.ancestors(id).contains(&ancestor)
    }

    /// The relationship types that count as `rel` followed backwards: those declared its
    /// inverse, and the one it declares itself the inverse of.
    fn inverses(&self, rel: &str) -> Vec<&'a str> {
        let mut out: Vec<&str> = Vec::new();
        if let Some((_, t)) = self.cat.relationship_type(rel)
            && let Some(inv) = t.inverse_of.as_deref()
        {
            out.push(inv);
        }
        for p in self.cat.packs() {
            for t in &p.relationship_types {
                if t.inverse_of.as_deref() == Some(rel) && !out.contains(&t.id.as_str()) {
                    out.push(t.id.as_str());
                }
            }
        }
        out
    }

    /// Every relationship that counts as one of type `rel` (empty: every relationship), as
    /// `(from, to, id)`: those of the type forwards, inverse ones backwards, in model order.
    pub fn edges(&self, rel: &str) -> Vec<Edge<'a>> {
        let inverses = if rel.is_empty() { Vec::new() } else { self.inverses(rel) };
        let mut out = Vec::new();
        for r in &self.model.relationships {
            if rel.is_empty() || r.type_id == rel {
                out.push((r.source.as_str(), r.target.as_str(), r.id.as_str()));
            } else if inverses.contains(&r.type_id.as_str()) {
                out.push((r.target.as_str(), r.source.as_str(), r.id.as_str()));
            }
        }
        out
    }

    /// The edges of [`Facts::edges`] by their `from` element, with both ends elements.
    pub fn graph(&self, rel: &str) -> HashMap<&'a str, Vec<(&'a str, &'a str)>> {
        let mut g: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for (a, b, id) in self.edges(rel) {
            if self.elements.contains_key(a) && self.elements.contains_key(b) {
                g.entry(a).or_default().push((b, id));
            }
        }
        g
    }

    /// Every element reachable from `starts` along `rel` (the starts included).
    pub fn reachable(&self, starts: impl IntoIterator<Item = &'a str>, rel: &str) -> HashSet<&'a str> {
        let g = self.graph(rel);
        reach(&g, starts)
    }

    /// Relationships of the model, for checks that walk them directly.
    pub fn relationships(&self) -> &'a [Relationship] {
        &self.model.relationships
    }
}

/// Breadth-first walk of `g` from `starts`.
pub fn reach<'a>(g: &HashMap<&'a str, Vec<(&'a str, &'a str)>>, starts: impl IntoIterator<Item = &'a str>) -> HashSet<&'a str> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    for s in starts {
        if seen.insert(s) {
            queue.push_back(s);
        }
    }
    while let Some(at) = queue.pop_front() {
        for (next, _) in g.get(at).map_or(&[][..], Vec::as_slice) {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    seen
}

/// `type_id` and its supertypes by `extends`, bounded and without repeats.
pub(crate) fn chain_of<'a>(cat: &'a Catalog, type_id: &'a str) -> Vec<&'a str> {
    let mut out = vec![type_id];
    let mut at = type_id;
    while out.len() < MAX_CHAIN {
        let Some(next) = cat.element_type(at).and_then(|(_, t)| t.extends.as_deref()) else { break };
        if out.contains(&next) {
            break;
        }
        out.push(next);
        at = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> Catalog {
        let pack: Pack = serde_json::from_value(serde_json::json!({
            "id": "p", "version": "1.0.0",
            "elementTypes": [
                {"id": "p.Equipment", "properties": [{"key": "tag", "required": true}, {"key": "size", "kind": "number"}],
                 "ports": [{"id": "in", "direction": "in"}]},
                {"id": "p.Pump", "extends": "p.Equipment", "properties": [{"key": "size", "kind": "integer"}]},
                {"id": "p.Loop1", "extends": "p.Loop2"},
                {"id": "p.Loop2", "extends": "p.Loop1"}
            ],
            "relationshipTypes": [{"id": "p.partOf"}, {"id": "p.hasPart", "inverseOf": "p.partOf"}]
        }))
        .unwrap();
        Catalog::new(vec![pack])
    }

    #[test]
    fn hierarchy_inherits_and_survives_cycles() {
        let cat = cat();
        let mut m = Model::new();
        m.add_element(Element::new("a", "p.Pump")).unwrap();
        m.add_element(Element::new("l", "p.Loop1")).unwrap();
        let f = Facts::new(&m, &cat);
        assert_eq!(f.chain("p.Pump"), ["p.Pump", "p.Equipment"]);
        assert!(f.is_a("p.Pump", "p.Equipment") && !f.is_a("p.Equipment", "p.Pump"));
        assert_eq!(f.chain("p.Loop1"), ["p.Loop1", "p.Loop2"]);
        let pack = cat.pack("p").unwrap();
        assert!(f.lists(pack, &["p.Equipment".into()], "p.Pump"));
        assert!(f.lists(pack, &["*".into()], "p.Pump"));
        assert!(!f.lists(pack, &["p.Loop1".into()], "p.Pump"));
        // The subtype's own property wins; the rest are inherited.
        let props: Vec<(&str, crate::PropKind)> = f.properties("p.Pump").iter().map(|p| (p.key.as_str(), p.kind)).collect();
        assert_eq!(props, [("size", crate::PropKind::Integer), ("tag", crate::PropKind::Text)]);
        assert_eq!(f.ports("p.Pump").len(), 1);
    }

    #[test]
    fn containment_and_inverse_reachability() {
        let cat = cat();
        let mut m = Model::new();
        for id in ["a", "b", "c", "d"] {
            m.add_element(Element::new(id, "p.Equipment")).unwrap();
        }
        m.set_owner("b", Some("a")).unwrap();
        m.set_owner("c", Some("b")).unwrap();
        m.add_relationship(Relationship::new("r1", "p.partOf", "a", "b")).unwrap();
        m.add_relationship(Relationship::new("r2", "p.hasPart", "d", "c")).unwrap();
        let f = Facts::new(&m, &cat);
        assert_eq!(f.ancestors("c"), ["b", "a"]);
        assert!(f.is_inside("c", "a") && !f.is_inside("a", "c"));
        // d hasPart c is c partOf d.
        let edges: Vec<(&str, &str)> = f.edges("p.partOf").iter().map(|(a, b, _)| (*a, *b)).collect();
        assert_eq!(edges, [("a", "b"), ("c", "d")]);
        let r = f.reachable(["c"], "p.partOf");
        assert!(r.contains("d") && !r.contains("a"));
        // Following hasPart, partOf counts backwards.
        assert!(f.reachable(["b"], "p.hasPart").contains("a"));
    }
}
