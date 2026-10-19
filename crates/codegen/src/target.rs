//! Targets: what a generator emits, what it can express, and the model it sees.
//!
//! A [`Target`] declares an id, a label, whether it writes one file (then hand-written code goes in
//! protected regions, see [`crate::regions`]) and a construct-support matrix: for each
//! [`Construct`], whether it is [`Support::Supported`], [`Support::Flattened`] (expressed in a
//! simpler form, only when the spec opts in with `options.flatten = true`) or
//! [`Support::Unsupported`]. The planner raises an issue for every construct the model uses that the
//! target leaves out, so a target is honest about what it drops.
//!
//! [`ModelView`] is the model cut to the spec's scope, elements sorted by type, tag then id and
//! relationships by type then id, so the same model always generates the same bytes.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use vizcraft_model::{Element, Model, Relationship};
use vizcraft_notation::Catalog;

use crate::CodegenError;
use crate::names::{NameRules, Names};
use crate::spec::Scope;

/// A kind of model construct a target may or may not express.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Construct {
    /// Typed elements.
    Element,
    /// Property values on elements and relationships.
    Property,
    /// Typed relationships between elements.
    Relationship,
    /// An element owned by another element.
    Containment,
    /// A relationship end attached to a named port.
    Port,
}

impl Construct {
    /// Every construct, in matrix order.
    pub const ALL: [Construct; 5] = [Construct::Element, Construct::Property, Construct::Relationship, Construct::Containment, Construct::Port];

    /// The name in results and params.
    pub fn name(self) -> &'static str {
        match self {
            Construct::Element => "element",
            Construct::Property => "property",
            Construct::Relationship => "relationship",
            Construct::Containment => "containment",
            Construct::Port => "port",
        }
    }

    /// Plural words for messages.
    pub fn plural(self) -> &'static str {
        match self {
            Construct::Element => "elements",
            Construct::Property => "property values",
            Construct::Relationship => "relationships",
            Construct::Containment => "owned elements",
            Construct::Port => "port ends",
        }
    }
}

/// How well a target expresses a construct.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Support {
    /// Expressed in full.
    Supported,
    /// Expressed in a simpler form, only when the spec sets `options.flatten`.
    Flattened,
    /// Not expressed; the planner raises an issue when the model uses it.
    Unsupported,
}

impl Support {
    /// The name in results.
    pub fn name(self) -> &'static str {
        match self {
            Support::Supported => "supported",
            Support::Flattened => "flattened",
            Support::Unsupported => "unsupported",
        }
    }
}

/// Target options from the spec, with the shared `flatten` switch read out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    /// The spec opted in to flattening constructs the target marks [`Support::Flattened`].
    pub flatten: bool,
    /// Every option as given.
    pub values: Map<String, Value>,
}

impl Options {
    /// Options from the spec's table.
    pub fn new(values: Map<String, Value>) -> Self {
        let flatten = values.get("flatten").and_then(Value::as_bool).unwrap_or(false);
        Options { flatten, values }
    }

    /// A string option.
    pub fn str(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(Value::as_str)
    }

    /// A boolean option.
    pub fn bool(&self, key: &str) -> Option<bool> {
        self.values.get(key).and_then(Value::as_bool)
    }
}

/// One file a target emits: a path relative to the output folder and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// Relative path, `/`-separated.
    pub path: String,
    /// The file's text; newlines are normalised by the writer.
    pub text: String,
}

/// A code or document generator.
pub trait Target: Sync {
    /// Stable id (`markdown-catalog`).
    fn id(&self) -> &'static str;
    /// Name for people.
    fn label(&self) -> &'static str;
    /// The target writes one file, so hand-written parts go in protected regions.
    fn single_file(&self) -> bool;
    /// The construct-support matrix; a construct left out is [`Support::Unsupported`].
    fn support(&self) -> Vec<(Construct, Support)>;
    /// The comment syntax regions are written in: opening and closing text (`("//", "")`).
    fn comment(&self) -> (&'static str, &'static str);
    /// How identifiers are made for this target's language.
    fn name_rules(&self) -> NameRules;
    /// Generate the files for `view`. Names come from `names`, never from the model directly, so
    /// a rename in the model changes nothing until the rename map is updated.
    fn generate(&self, view: &ModelView, names: &Names, options: &Options) -> Result<Vec<Output>, CodegenError>;
}

/// How `target` supports `c`.
pub fn support_of(target: &dyn Target, c: Construct) -> Support {
    target.support().into_iter().find(|(k, _)| *k == c).map(|(_, s)| s).unwrap_or(Support::Unsupported)
}

/// Whether `target` writes `c` given the options: supported, or flattened with the opt-in.
pub fn emits(target: &dyn Target, c: Construct, options: &Options) -> bool {
    match support_of(target, c) {
        Support::Supported => true,
        Support::Flattened => options.flatten,
        Support::Unsupported => false,
    }
}

static MARKDOWN: crate::targets::markdown::MarkdownCatalog = crate::targets::markdown::MarkdownCatalog;

static TARGETS: [&dyn Target; 1] = [&MARKDOWN];

/// The built-in targets, in menu order.
pub fn targets() -> &'static [&'static dyn Target] {
    &TARGETS
}

/// The built-in target with this id.
pub fn target(id: &str) -> Option<&'static dyn Target> {
    TARGETS.iter().copied().find(|t| t.id() == id)
}

/// Most subjects an issue about a construct names.
const MAX_SUBJECTS: usize = 20;

/// Deepest `extends` chain followed when collecting declared properties.
const MAX_EXTENDS: usize = 16;

/// The model cut to a scope, in a stable order.
pub struct ModelView<'a> {
    /// The whole model, for lookups (owners outside the scope, names).
    pub model: &'a Model,
    /// The notation catalog, for type labels and declared properties.
    pub catalog: &'a Catalog,
    /// Elements in scope, sorted by type, tag then id.
    pub elements: Vec<&'a Element>,
    /// Relationships whose ends are both in scope, sorted by type then id.
    pub relationships: Vec<&'a Relationship>,
    /// Ids of `elements`, for constant-time membership.
    ids: std::collections::HashSet<&'a str>,
}

impl<'a> ModelView<'a> {
    /// The part of `model` that `scope` covers.
    pub fn new(model: &'a Model, catalog: &'a Catalog, scope: &Scope) -> Self {
        let pass = |e: &Element| {
            (scope.packs.is_empty() || scope.packs.iter().any(|p| p == e.pack()))
                && (scope.types.is_empty() || scope.types.contains(&e.type_id))
                && (scope.elements.is_empty() || scope.elements.contains(&e.id))
        };
        let mut elements: Vec<&Element> = model.elements.iter().filter(|e| pass(e)).collect();
        elements.sort_by(|a, b| (&a.type_id, &a.tag, &a.id).cmp(&(&b.type_id, &b.tag, &b.id)));
        let ids: std::collections::HashSet<&'a str> = elements.iter().map(|e| e.id.as_str()).collect();
        let mut relationships: Vec<&Relationship> =
            model.relationships.iter().filter(|r| ids.contains(r.source.as_str()) && ids.contains(r.target.as_str())).collect();
        relationships.sort_by(|a, b| (&a.type_id, &a.id).cmp(&(&b.type_id, &b.id)));
        ModelView { model, catalog, elements, relationships, ids }
    }

    /// Whether the element `id` is in the view.
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    /// The element `id`, if it is in the view.
    pub fn element(&self, id: &str) -> Option<&'a Element> {
        self.elements.iter().copied().find(|e| e.id == id)
    }

    /// The element type ids of the view, in element order, once each.
    pub fn element_types(&self) -> Vec<&'a str> {
        let mut out: Vec<&str> = Vec::new();
        for e in &self.elements {
            if out.last() != Some(&e.type_id.as_str()) {
                out.push(&e.type_id);
            }
        }
        out
    }

    /// The label the type's pack gives it, or the part of the id after the pack prefix.
    pub fn type_label(&self, type_id: &str) -> String {
        let fallback = || type_id.split_once('.').map(|(_, t)| t).unwrap_or(type_id).to_string();
        if let Some((_, t)) = self.catalog.element_type(type_id) {
            return if t.label.is_empty() { fallback() } else { t.label.clone() };
        }
        if let Some((_, t)) = self.catalog.relationship_type(type_id) {
            return if t.label.is_empty() { fallback() } else { t.label.clone() };
        }
        fallback()
    }

    /// The pack's name for a type id's prefix, or the prefix itself.
    pub fn pack_label(&self, type_id: &str) -> String {
        let id = type_id.split_once('.').map(|(p, _)| p).unwrap_or(type_id);
        self.catalog.pack(id).map(|p| p.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| id.to_string())
    }

    /// Declared properties of an element type as (key, label), the type's own first, then the
    /// ones it inherits, once each.
    pub fn declared_properties(&self, type_id: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut next = Some(type_id.to_string());
        for _ in 0..MAX_EXTENDS {
            let Some(id) = next.take() else { break };
            let Some((_, t)) = self.catalog.element_type(&id) else { break };
            for p in &t.properties {
                if !out.iter().any(|(k, _)| *k == p.key) {
                    let label = if p.label.is_empty() { p.key.clone() } else { p.label.clone() };
                    out.push((p.key.clone(), label));
                }
            }
            next = t.extends.clone();
        }
        out
    }

    /// Ids of the elements or relationships that use construct `c`.
    pub fn uses(&self, c: Construct) -> Vec<String> {
        match c {
            Construct::Element => self.elements.iter().map(|e| e.id.clone()).collect(),
            Construct::Property => self
                .elements
                .iter()
                .filter(|e| !e.properties.is_empty())
                .map(|e| e.id.clone())
                .chain(self.relationships.iter().filter(|r| !r.properties.is_empty()).map(|r| r.id.clone()))
                .collect(),
            Construct::Relationship => self.relationships.iter().map(|r| r.id.clone()).collect(),
            Construct::Containment => {
                self.elements.iter().filter(|e| e.owner.as_deref().is_some_and(|o| self.contains(o))).map(|e| e.id.clone()).collect()
            }
            Construct::Port => {
                self.relationships.iter().filter(|r| r.source_port.is_some() || r.target_port.is_some()).map(|r| r.id.clone()).collect()
            }
        }
    }

    /// Issues for the constructs the view uses that `target` leaves out with these options.
    pub fn support_issues(&self, target: &dyn Target, options: &Options) -> Vec<vizcraft_model::Issue> {
        let mut out = Vec::new();
        for c in Construct::ALL {
            let used = self.uses(c);
            if used.is_empty() {
                continue;
            }
            let n = used.len();
            let subjects: Vec<String> = used.into_iter().take(MAX_SUBJECTS).collect();
            match support_of(target, c) {
                Support::Supported => {}
                Support::Flattened if options.flatten => {}
                Support::Flattened => out.push(crate::issue(
                    "flattened",
                    vizcraft_model::Severity::Warning,
                    format!(
                        "{} writes {} only in a flattened form; {n} left out (set options.flatten to true to write them)",
                        target.label(),
                        c.plural()
                    ),
                    subjects,
                )),
                Support::Unsupported => out.push(crate::issue(
                    "unsupported",
                    vizcraft_model::Severity::Warning,
                    format!("{} cannot express {}; {n} left out", target.label(), c.plural()),
                    subjects,
                )),
            }
        }
        out
    }
}
