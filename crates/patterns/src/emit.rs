//! What a pattern emits: model elements keyed by role (`split`, `branch.3`, `join`), the
//! relationships between roles, containment, diagram regions, the layout primitive with the roles
//! to lay out, and the name generated for each item. [`generate`] is pure: it reads the pattern,
//! its checked arguments and the notation catalog, and touches no document.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use vizcraft_layout::Direction;
use vizcraft_layout::Primitive;
use vizcraft_notation::Catalog;

use crate::schema::{Args, Pattern, quote};
use crate::{MAX_EMITTED, PatternError};

/// Longest role name.
pub const MAX_ROLE: usize = 64;

/// One element a pattern emits.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmittedElement {
    /// Its role in the pattern (`branch.2`); stable across regenerations with other arguments.
    pub role: String,
    /// Qualified element type (`flowchart.Process`).
    #[serde(rename = "type")]
    pub type_id: String,
    /// The name generated for it.
    pub name: String,
    /// The role of the element that owns it in the model (its type must be a container).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Whether its tag takes the pattern's tag prefix (when its type numbers tags).
    pub tagged: bool,
}

/// One relationship a pattern emits.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmittedRelationship {
    /// Its role (`flow.split.branch.2`).
    pub role: String,
    /// Qualified relationship type (`flowchart.Flow`).
    #[serde(rename = "type")]
    pub type_id: String,
    /// The source element's role.
    pub from: String,
    /// The target element's role.
    pub to: String,
    /// The name generated for it (a branch label), often empty.
    pub name: String,
}

/// A region of the drawing: a diagram container (no model element) holding the shapes of
/// `members`, for packs with no container type of their own.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    /// Its role (`region`).
    pub role: String,
    /// The container's heading.
    pub label: String,
    /// The element roles inside it.
    pub members: Vec<String>,
}

/// Everything one generation produces.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Emission {
    /// The pattern id.
    pub pattern: String,
    /// The elements, in creation order (the order the pipeline primitive reads its steps in).
    pub elements: Vec<EmittedElement>,
    /// The relationships, in creation order.
    pub relationships: Vec<EmittedRelationship>,
    /// The diagram regions.
    pub regions: Vec<Region>,
    /// How the shapes are laid out; roles name element roles (and region roles for `nested`).
    pub layout: Primitive<String>,
    /// The direction the layout flows in.
    pub direction: Direction,
    /// The prefix for the tags of elements marked `tagged`; empty leaves the pack's numbering.
    pub tag_prefix: String,
}

impl Emission {
    /// An empty emission for `pattern` laid out with `layout`.
    pub fn new(pattern: &str, layout: Primitive<String>, direction: Direction) -> Emission {
        Emission {
            pattern: pattern.to_string(),
            elements: Vec::new(),
            relationships: Vec::new(),
            regions: Vec::new(),
            layout,
            direction,
            tag_prefix: String::new(),
        }
    }

    /// Add an element whose tag takes the tag prefix; returns it to set an owner or untag it.
    pub fn element(&mut self, role: impl Into<String>, type_id: &str, name: impl Into<String>) -> Option<&mut EmittedElement> {
        self.elements.push(EmittedElement { role: role.into(), type_id: type_id.to_string(), name: name.into(), owner: None, tagged: true });
        self.elements.last_mut()
    }

    /// Add a relationship from role `from` to role `to`, its role `<prefix>.<from>.<to>`.
    pub fn relate(&mut self, prefix: &str, type_id: &str, from: &str, to: &str, name: impl Into<String>) {
        self.relationships.push(EmittedRelationship {
            role: format!("{prefix}.{from}.{to}"),
            type_id: type_id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            name: name.into(),
        });
    }

    /// The emitted element with this role.
    pub fn element_by_role(&self, role: &str) -> Option<&EmittedElement> {
        self.elements.iter().find(|e| e.role == role)
    }

    /// The emitted relationship with this role.
    pub fn relationship_by_role(&self, role: &str) -> Option<&EmittedRelationship> {
        self.relationships.iter().find(|r| r.role == role)
    }

    /// Role to generated name, for every element and relationship.
    pub fn generated(&self) -> BTreeMap<String, String> {
        self.elements
            .iter()
            .map(|e| (e.role.clone(), e.name.clone()))
            .chain(self.relationships.iter().map(|r| (r.role.clone(), r.name.clone())))
            .collect()
    }

    /// Check the emission against itself and the catalog: roles unique and short, every type
    /// known to the catalog, relationship ends, owners, region members and layout roles naming
    /// emitted roles, and the size within [`MAX_EMITTED`].
    pub fn validate(&self, cat: &Catalog) -> Result<(), PatternError> {
        let total = self.elements.len().saturating_add(self.relationships.len()).saturating_add(self.regions.len());
        if total > MAX_EMITTED {
            return Err(PatternError::Args(format!("{} would emit {total} items, more than {MAX_EMITTED}", self.pattern)));
        }
        let mut roles: HashSet<&str> = HashSet::new();
        let all = self
            .elements
            .iter()
            .map(|e| e.role.as_str())
            .chain(self.relationships.iter().map(|r| r.role.as_str()))
            .chain(self.regions.iter().map(|r| r.role.as_str()));
        for role in all {
            if role.is_empty() || role.len() > MAX_ROLE || !roles.insert(role) {
                return Err(PatternError::Pattern(format!("{} emits the role `{}` twice or out of shape", self.pattern, quote(role))));
            }
        }
        let elements: HashSet<&str> = self.elements.iter().map(|e| e.role.as_str()).collect();
        for e in &self.elements {
            if cat.element_type(&e.type_id).is_none() {
                return Err(PatternError::Pack(format!("{} needs the element type {}, which no loaded pack defines", self.pattern, e.type_id)));
            }
            if let Some(o) = &e.owner {
                let owner = self
                    .element_by_role(o)
                    .ok_or_else(|| PatternError::Pattern(format!("{}: {} is owned by no role `{o}`", self.pattern, e.role)))?;
                if !cat.element_type(&owner.type_id).is_some_and(|(_, t)| t.container) {
                    return Err(PatternError::Pattern(format!("{}: {} cannot own elements", self.pattern, owner.type_id)));
                }
            }
        }
        for r in &self.relationships {
            if cat.relationship_type(&r.type_id).is_none() {
                return Err(PatternError::Pack(format!("{} needs the relationship type {}, which no loaded pack defines", self.pattern, r.type_id)));
            }
            if !elements.contains(r.from.as_str()) || !elements.contains(r.to.as_str()) {
                return Err(PatternError::Pattern(format!("{}: {} joins a role that is not an element", self.pattern, r.role)));
            }
        }
        for g in &self.regions {
            if let Some(m) = g.members.iter().find(|m| !elements.contains(m.as_str())) {
                return Err(PatternError::Pattern(format!("{}: region {} holds no element role `{m}`", self.pattern, g.role)));
            }
        }
        if let Some(r) = self.layout.references().into_iter().find(|r| !roles.contains(r.as_str())) {
            return Err(PatternError::Pattern(format!("{}: the layout names no role `{}`", self.pattern, quote(r))));
        }
        Ok(())
    }
}

/// Generate `pattern` with checked `args` against `cat`: the pattern's generator builds the
/// emission, which is then validated ([`Emission::validate`]). An unknown pattern id or a pack
/// missing from the catalog is an error.
pub fn generate(pattern: &Pattern, args: &Args, cat: &Catalog) -> Result<Emission, PatternError> {
    let builtin = crate::builtin(&pattern.id).ok_or_else(|| PatternError::Unknown(quote(&pattern.id)))?;
    if cat.pack(&pattern.pack).is_none() {
        return Err(PatternError::Pack(format!("{} needs the {} pack, which is not loaded", pattern.id, pattern.pack)));
    }
    let mut e = (builtin.generate)(args)?;
    e.pattern = pattern.id.clone();
    e.validate(cat)?;
    Ok(e)
}
