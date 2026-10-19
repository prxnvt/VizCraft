//! The rules engine: checks declared by packs, run over a document.
//!
//! Live rules (`RuleMode::Live`) run after every command that changes the document and must stay
//! cheap (linear in the model and the views). Batch rules run on request over a scope. Each rule
//! declared by a pack the model uses runs once; its findings become [`Issue`]s carrying the rule's
//! id and severity, the element and relationship ids involved and the view items that draw them.
//! Results are sorted by severity, then rule id, then first subject, so equal documents give equal
//! lists.
//!
//! The document's conformance profile (`doc.conformance`, see [`profile`]) is read first: when it
//! is off nothing runs; when it lists rulesets only their rules run; otherwise every rule of every
//! pack in use runs. The packs in use are those listed in `doc.model.packs` plus the pack of every
//! element and relationship type present (and of their supertypes), looked up in the catalog
//! (unknown packs are skipped). Overrides change a rule's severity or switch it off;
//! suppressions drop issues by key.
//!
//! Every lookup a check needs (elements by id, relationships by source and by target, view items
//! by model id, element types and their supertypes) is indexed once per call ([`Facts`] is the
//! reasoner's part), so the live checks stay linear in the size of the model and its views; the
//! graph checks (`unreachable`, `forbiddenCycle`, `requiredPath`, `connected`) and the drawing
//! checks that compare shapes with each other are batch checks in the built-in packs.

use std::collections::{HashMap, HashSet};

use serde_json::json;
use vizcraft_diagram::{Conformance, Document, End};
use vizcraft_model::issue::Fix;
use vizcraft_model::{Element, IdScope, Issue, PropValue, Registry, Relationship};

use crate::infer::Facts;
use crate::schema::{Check, CompareOp, ElementType, Pack, PortDirection, PropKind, PropertySchema, RuleDecl, RuleMode};
use crate::{Catalog, legal};

/// Longest issue message, in bytes; longer messages are cut at a character boundary.
pub const MAX_MESSAGE: usize = 400;

/// Longest name, tag or text quoted inside a message, in bytes.
pub const MAX_LABEL: usize = 60;

/// Most issues one rule reports in one run, so a broken model cannot flood the Problems tab or an
/// agent's context; the first ones in model order are kept.
pub const MAX_PER_RULE: usize = 10_000;

/// Most element names listed in one message (a duplicate tag used many times).
const MAX_NAMED: usize = 5;

/// What a batch run looks at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// The whole project.
    #[default]
    All,
    /// The page at this index: elements drawn on it and its connectors.
    Page(usize),
    /// These elements, relationships, shapes or connectors (shapes and connectors stand for the
    /// model items they draw).
    Items(Vec<String>),
}

/// Live issues for `doc`: the live rules its conformance profile selects (see [`profile`]).
pub fn live(doc: &Document, cat: &Catalog) -> Vec<Issue> {
    run(doc, cat, RuleMode::Live)
}

/// Batch issues for `doc` within `scope`: the batch rules its conformance profile selects, with
/// findings outside the scope left out.
pub fn batch(doc: &Document, cat: &Catalog, scope: &Scope) -> Vec<Issue> {
    in_scope(doc, scope, run(doc, cat, RuleMode::Batch))
}

/// The issues that touch `scope`, in their original order. `Page(i)`: issues with a view item on
/// page `i` (a missing page keeps none). `Items(ids)`: issues whose subjects or items include one
/// of `ids`, where a shape id also stands for the element it draws and a connector id for its
/// relationship. Used by [`batch`] and by `validate.run` to filter live issues the same way.
pub fn in_scope(doc: &Document, scope: &Scope, issues: Vec<Issue>) -> Vec<Issue> {
    match scope {
        Scope::All => issues,
        Scope::Page(i) => {
            let Some(page) = doc.pages.get(*i) else { return Vec::new() };
            let on_page: HashSet<&str> = page.shapes.iter().map(|s| s.id.as_str()).chain(page.connectors.iter().map(|c| c.id.as_str())).collect();
            issues.into_iter().filter(|is| is.items.iter().any(|it| on_page.contains(it.as_str()))).collect()
        }
        Scope::Items(ids) => {
            let mut wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
            // A shape or connector named in the scope stands for the model item it draws.
            for page in &doc.pages {
                for s in &page.shapes {
                    if let Some(e) = s.element.as_deref()
                        && wanted.contains(s.id.as_str())
                    {
                        wanted.insert(e);
                    }
                }
                for c in &page.connectors {
                    if let Some(r) = c.relationship.as_deref()
                        && wanted.contains(c.id.as_str())
                    {
                        wanted.insert(r);
                    }
                }
            }
            issues.into_iter().filter(|is| is.subjects.iter().chain(is.items.iter()).any(|x| wanted.contains(x.as_str()))).collect()
        }
    }
}

/// Put issues in the stable order every result uses (severity, rule id, first subject, then the
/// issue key) and drop repeats of the same key, keeping the first.
pub fn sort(issues: &mut Vec<Issue>) {
    let mut seen = HashSet::new();
    issues.retain(|i| seen.insert(i.key()));
    issues.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.rule.cmp(&b.rule))
            .then_with(|| a.subjects.first().cmp(&b.subjects.first()))
            .then_with(|| a.key().cmp(&b.key()))
    });
}

// ---------------------------------------------------------------- the conformance profile

/// The ids of the packs `doc` uses: those recorded in the model, the packs of its element and
/// relationship types and of their supertypes, and the packs of page viewpoints.
pub fn packs_in_use(doc: &Document, cat: &Catalog) -> HashSet<String> {
    let model = &doc.model;
    let mut used: HashSet<String> = model.packs.iter().map(|p| p.id.clone()).collect();
    let mut types: HashSet<&str> = HashSet::new();
    for e in &model.elements {
        if types.insert(e.type_id.as_str()) {
            for t in crate::infer::chain_of(cat, &e.type_id) {
                used.insert(vizcraft_model::pack_of(t).to_string());
            }
        }
    }
    used.extend(model.relationships.iter().map(|r| r.pack().to_string()));
    used.extend(doc.pages.iter().filter_map(|p| p.view.as_ref()).map(|v| vizcraft_model::pack_of(&v.viewpoint).to_string()));
    used
}

/// The ruleset a document starting to use `pack` gets: its first ruleset, or the pack id (every
/// rule of the pack) when it declares none.
pub fn default_ruleset(pack: &Pack) -> String {
    pack.rulesets.first().map_or_else(|| pack.id.clone(), |r| r.id.clone())
}

/// The profile the rules run with. One the user set is used as it is. Until then the profile
/// follows the notation: enabled with the default ruleset of every pack in use that has rules,
/// added to whatever the sync already stored, so free drawing stays off and choosing a notation
/// brings its rules (documents saved before profiles existed check as they always did).
pub fn profile(doc: &Document, cat: &Catalog) -> Conformance {
    let stored = &doc.conformance;
    if stored.user_set {
        return stored.clone();
    }
    let mut p = stored.clone();
    let used = packs_in_use(doc, cat);
    for pack in cat.packs().iter().filter(|pk| !pk.rules.is_empty() && used.contains(&pk.id)) {
        let id = default_ruleset(pack);
        if !p.rulesets.contains(&id) {
            p.rulesets.push(id);
        }
    }
    p.enabled = p.enabled || !p.rulesets.is_empty();
    p.sanitize();
    p
}

/// Store the notation-following profile in the document (called by `bind::sync` after every
/// change): the first time a page gets a viewpoint or the model gains an element of a pack with
/// rules, the profile turns on with that pack's default ruleset. Does nothing once the user has
/// set the profile, and changes nothing when the stored profile is already current.
pub fn auto_enable(doc: &mut Document, cat: &Catalog) {
    if doc.conformance.user_set {
        return;
    }
    let p = profile(doc, cat);
    if p.enabled != doc.conformance.enabled || p.rulesets != doc.conformance.rulesets {
        doc.conformance.enabled = p.enabled;
        doc.conformance.rulesets = p.rulesets;
    }
}

/// The rules `profile` selects, each with the pack that declares it, in catalog order. Nothing
/// when the profile is off. With no rulesets: every rule of every pack in use. Otherwise the rules
/// the listed rulesets name (a pack id stands for all its rules); unknown ids are skipped.
/// Overrides are not applied here (see [`run`]).
pub fn selected<'c>(doc: &Document, cat: &'c Catalog, profile: &Conformance) -> Vec<(&'c Pack, &'c RuleDecl)> {
    if !profile.enabled {
        return Vec::new();
    }
    if profile.rulesets.is_empty() {
        let used = packs_in_use(doc, cat);
        return cat.packs().iter().filter(|p| used.contains(&p.id)).flat_map(|p| p.rules.iter().map(move |r| (p, r))).collect();
    }
    let mut wanted: HashSet<&str> = HashSet::new();
    for id in &profile.rulesets {
        if let Some(pack) = cat.pack(id) {
            wanted.extend(pack.rules.iter().map(|r| r.id.as_str()));
        } else if let Some(rs) = cat.ruleset(id) {
            wanted.extend(rs.rules.iter().map(String::as_str));
        }
    }
    cat.packs().iter().flat_map(|p| p.rules.iter().filter(|r| wanted.contains(r.id.as_str())).map(move |r| (p, r))).collect()
}

/// Every selected rule of `mode`, with overrides and suppressions applied, sorted.
fn run(doc: &Document, cat: &Catalog, mode: RuleMode) -> Vec<Issue> {
    let profile = profile(doc, cat);
    let rules = selected(doc, cat, &profile);
    if !rules.iter().any(|(_, r)| r.mode == mode) {
        return Vec::new();
    }
    let ctx = Ctx::new(doc, cat);
    let mut out = Vec::new();
    for (pack, rule) in rules {
        if rule.mode != mode || profile.is_disabled(&rule.id) {
            continue;
        }
        let mut found = Vec::new();
        ctx.check(pack, rule, &mut found);
        found.truncate(MAX_PER_RULE);
        // An override sets every finding's severity; without one a check may have reported a
        // lighter finding than its rule's (a foreign key of another type is a warning).
        let severity = profile.severity_of(&rule.id, rule.severity);
        if severity != rule.severity {
            for i in &mut found {
                i.severity = severity;
            }
        }
        out.extend(found);
    }
    if !profile.suppressions.is_empty() {
        let silenced: HashSet<&str> = profile.suppressions.iter().map(|s| s.key.as_str()).collect();
        out.retain(|i| !silenced.contains(i.key().as_str()));
    }
    sort(&mut out);
    out
}

/// One run's indexes over the document.
struct Ctx<'a> {
    doc: &'a Document,
    cat: &'a Catalog,
    /// Elements by id (the first wins if a damaged model repeats an id).
    elements: HashMap<&'a str, &'a Element>,
    /// Relationships by id (the first wins).
    relationships: HashMap<&'a str, &'a Relationship>,
    /// Relationships by source element id, in model order.
    out_by: HashMap<&'a str, Vec<&'a Relationship>>,
    /// Relationships by target element id, in model order.
    in_by: HashMap<&'a str, Vec<&'a Relationship>>,
    /// View items by model id: (page index, shape or connector id), pages in order, shapes then
    /// connectors in z order, as `vizcraft_diagram::views::views_of` lists them.
    views: HashMap<&'a str, Vec<(usize, &'a str)>>,
    /// Element types by qualified id, for every type present in the model.
    types: HashMap<&'a str, Option<&'a ElementType>>,
    /// Derived facts: type hierarchy, containment, inverse relationships.
    facts: Facts<'a>,
}

impl<'a> Ctx<'a> {
    fn new(doc: &'a Document, cat: &'a Catalog) -> Self {
        let model = &doc.model;
        let mut elements = HashMap::with_capacity(model.elements.len());
        let mut types = HashMap::new();
        for e in &model.elements {
            elements.entry(e.id.as_str()).or_insert(e);
            types.entry(e.type_id.as_str()).or_insert_with(|| cat.element_type(&e.type_id).map(|(_, t)| t));
        }
        let mut out_by: HashMap<&str, Vec<&Relationship>> = HashMap::new();
        let mut in_by: HashMap<&str, Vec<&Relationship>> = HashMap::new();
        let mut relationships = HashMap::with_capacity(model.relationships.len());
        for r in &model.relationships {
            relationships.entry(r.id.as_str()).or_insert(r);
            out_by.entry(r.source.as_str()).or_default().push(r);
            in_by.entry(r.target.as_str()).or_default().push(r);
        }
        let mut views: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
        for (pi, page) in doc.pages.iter().enumerate() {
            for s in &page.shapes {
                if let Some(e) = s.element.as_deref() {
                    views.entry(e).or_default().push((pi, s.id.as_str()));
                }
            }
            for c in &page.connectors {
                if let Some(r) = c.relationship.as_deref() {
                    views.entry(r).or_default().push((pi, c.id.as_str()));
                }
            }
        }
        Ctx { doc, cat, elements, relationships, out_by, in_by, views, types, facts: Facts::new(model, cat) }
    }

    /// Whether the rule's `types` list covers an element or relationship type: an empty list
    /// covers every element type of the pack (or a subtype of one) and every relationship type
    /// of the pack; otherwise the list names the type or a supertype, or holds `*`.
    fn covers(&self, pack: &Pack, types: &[String], type_id: &str) -> bool {
        if types.is_empty() {
            self.facts.of_pack(type_id, pack) || pack.relationship_type(type_id).is_some()
        } else {
            self.facts.lists(pack, types, type_id)
        }
    }

    /// Like [`Ctx::covers`] for checks about elements only.
    fn covers_elements(&self, pack: &Pack, types: &[String], type_id: &str) -> bool {
        if types.is_empty() { self.facts.of_pack(type_id, pack) } else { self.facts.lists(pack, types, type_id) }
    }

    /// Whether the connection table allows `rel` from `src` to `tgt`, for the types or any of
    /// their supertypes.
    fn is_legal(&self, src: &'a str, rel: &str, tgt: &'a str) -> bool {
        let (sc, tc) = (self.facts.chain(src), self.facts.chain(tgt));
        sc.iter().any(|s| tc.iter().any(|t| legal::is_legal(self.cat, s, rel, t)))
    }

    fn element_type(&self, type_id: &str) -> Option<&'a ElementType> {
        self.types.get(type_id).copied().flatten()
    }

    fn out_of(&self, id: &str) -> &[&'a Relationship] {
        self.out_by.get(id).map_or(&[], Vec::as_slice)
    }

    fn in_of(&self, id: &str) -> &[&'a Relationship] {
        self.in_by.get(id).map_or(&[], Vec::as_slice)
    }

    /// Run one rule.
    fn check(&self, pack: &'a Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
        match &rule.check {
            Check::IllegalRelationship => self.illegal(pack, rule, out),
            Check::Multiplicity => self.multiplicity(pack, rule, out),
            Check::RequiredProperty { types } => self.properties(pack, rule, types, true, out),
            Check::PropertyType { types } => self.properties(pack, rule, types, false, out),
            Check::DuplicateIdentifier { scope } => self.duplicates(rule, scope, out),
            Check::UnmatchedPair { scope } => self.unmatched(rule, scope, out),
            Check::ViewpointType => self.viewpoint(pack, rule, out),
            Check::DanglingConnector => self.dangling(pack, rule, out),
            Check::MinOutgoing { types, relationship, min } => self.min_count(pack, rule, types, relationship, *min, true, out),
            Check::MinIncoming { types, relationship, min } => self.min_count(pack, rule, types, relationship, *min, false, out),
            Check::ExclusiveDirections { types, relationship } => self.exclusive(pack, rule, types, relationship, out),
            Check::Unreachable { from, relationship, types } => self.unreachable(pack, rule, from, relationship, types, out),
            Check::ForbiddenCycle { relationship } => self.cycles(rule, relationship, out),
            Check::RequiredPath { from, to, relationship } => self.required_path(pack, rule, from, to, relationship, out),
            Check::Connected { types, relationship } => self.connected(pack, rule, types, relationship, out),
            Check::Containment { owners, children } => self.containment(pack, rule, owners, children, out),
            Check::PortDirection { relationship } => self.port_direction(pack, rule, relationship, out),
            Check::PortFanIn { relationship, max } => self.port_fan_in(pack, rule, relationship, *max, out),
            Check::UniqueProperty { types, property, within } => self.unique_property(pack, rule, types, property, within, out),
            Check::PropertyCompare { types, property, op, other } => self.compare(pack, rule, types, property, *op, other.as_ref(), out),
            Check::NameFormat { types, pattern } => self.name_format(pack, rule, types, pattern, out),
            Check::CrossesShape | Check::UngluedEnd | Check::Overlap | Check::OffBoard | Check::TextOverflow => {
                crate::drawing::run(self.doc, rule, out)
            }
            Check::DocumentFields { .. } | Check::RevisionSequence | Check::BalloonRows | Check::CrossReferences | Check::StaleTag => {
                crate::sheet_pass::check(self.doc, self.cat, pack, rule, out)
            }
            Check::NestingAllowed { .. } | Check::Labelled { .. } | Check::EndsOnPorts { .. } => {
                crate::nesting::check(self.doc, self.cat, pack, rule, out)
            }
            Check::EndMarkerMatchesProperty { .. } | Check::MultiplicitySyntax { .. } | Check::MaxPerOwner { .. } | Check::UniqueMembers { .. } => {
                crate::rules_uml::check(self.doc, self.cat, pack, rule, out)
            }
            Check::MessageOrder | Check::RepliesMatched { .. } => crate::sequence_rules::check(self.doc, rule, out),
            Check::PrimaryKey { .. } | Check::ForeignKey { .. } => crate::er_rules::check(self.doc, self.cat, pack, rule, out),
            Check::Unknown => {}
        }
    }

    // ---------------------------------------------------------------- checks

    /// Relationships of this pack that no connection-table row allows. A relationship type the
    /// pack does not define, or an end element of unknown type, is illegal too.
    fn illegal(&self, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
        for r in self.doc.model.relationships.iter().filter(|r| r.pack() == pack.id) {
            let src = self.elements.get(r.source.as_str());
            let tgt = self.elements.get(r.target.as_str());
            let why = if pack.relationship_type(&r.type_id).is_none() {
                Some("is not a relationship type of this notation")
            } else {
                match (src, tgt) {
                    (Some(s), Some(t)) if self.element_type(&s.type_id).is_none() || self.element_type(&t.type_id).is_none() => {
                        Some("joins an element of a type this notation does not know")
                    }
                    (Some(s), Some(t)) if !self.is_legal(&s.type_id, &r.type_id, &t.type_id) => Some("is not allowed"),
                    (Some(_), Some(_)) => None,
                    _ => Some("has an end that is not an element"),
                }
            };
            if let Some(why) = why {
                let what = format!("{} from {} to {} {why}.", self.rel_type_label(&r.type_id), self.el_label(&r.source), self.el_label(&r.target));
                out.push(self.issue(rule, what, vec![r.id.clone(), r.source.clone(), r.target.clone()]));
            }
        }
    }

    /// Connection-table `outMax` and `inMax`, per row.
    fn multiplicity(&self, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
        for row in &pack.connections {
            if row.out_max.is_none() && row.in_max.is_none() {
                continue;
            }
            let rel_label = self.rel_type_label(&row.relationship);
            for e in &self.doc.model.elements {
                if let Some(max) = row.out_max
                    && self.facts.lists(pack, &row.source, &e.type_id)
                {
                    let hits: Vec<&Relationship> = self
                        .out_of(&e.id)
                        .iter()
                        .copied()
                        .filter(|r| {
                            r.type_id == row.relationship
                                && self.elements.get(r.target.as_str()).is_some_and(|t| self.facts.lists(pack, &row.target, &t.type_id))
                        })
                        .collect();
                    if count_over(hits.len(), max) {
                        let what = format!(
                            "{} has {} outgoing {rel_label} relationships of this kind; at most {max} {}.",
                            self.el_label(&e.id),
                            hits.len(),
                            allowed(max)
                        );
                        out.push(self.issue(rule, what, subjects_with(&e.id, &hits)));
                    }
                }
                if let Some(max) = row.in_max
                    && self.facts.lists(pack, &row.target, &e.type_id)
                {
                    let hits: Vec<&Relationship> = self
                        .in_of(&e.id)
                        .iter()
                        .copied()
                        .filter(|r| {
                            r.type_id == row.relationship
                                && self.elements.get(r.source.as_str()).is_some_and(|s| self.facts.lists(pack, &row.source, &s.type_id))
                        })
                        .collect();
                    if count_over(hits.len(), max) {
                        let what = format!(
                            "{} has {} incoming {rel_label} relationships of this kind; at most {max} {}.",
                            self.el_label(&e.id),
                            hits.len(),
                            allowed(max)
                        );
                        out.push(self.issue(rule, what, subjects_with(&e.id, &hits)));
                    }
                }
            }
        }
    }

    /// `requiredProperty` (`required` true) and `propertyType` (false) over elements and
    /// relationships whose type `types` covers.
    fn properties(&self, pack: &Pack, rule: &RuleDecl, types: &[String], required: bool, out: &mut Vec<Issue>) {
        for e in self.doc.model.elements.iter().filter(|e| self.facts.of_pack(&e.type_id, pack) && self.covers(pack, types, &e.type_id)) {
            if self.element_type(&e.type_id).is_none() {
                continue;
            }
            let schema = self.facts.properties(&e.type_id);
            self.check_props(rule, &e.id, &schema, &e.properties, required, true, out);
        }
        for r in self.doc.model.relationships.iter().filter(|r| r.pack() == pack.id && self.covers(pack, types, &r.type_id)) {
            let Some(t) = pack.relationship_type(&r.type_id) else { continue };
            let schema: Vec<&PropertySchema> = t.properties.iter().collect();
            self.check_props(rule, &r.id, &schema, &r.properties, required, false, out);
        }
    }

    /// `element` says whether `id` is an element, which the fixes (`element.setProperty`) need.
    #[allow(clippy::too_many_arguments)]
    fn check_props(
        &self,
        rule: &RuleDecl,
        id: &str,
        schema: &[&PropertySchema],
        props: &std::collections::BTreeMap<String, PropValue>,
        required: bool,
        element: bool,
        out: &mut Vec<Issue>,
    ) {
        // All findings about one item go in one issue: the issue key (rule, subjects, items) must
        // tell issues apart, and two issues about the same item would share it.
        let mut found: Vec<String> = Vec::new();
        let mut fixes: Vec<Fix> = Vec::new();
        for &p in schema {
            let v = props.get(&p.key);
            if required {
                let missing = match v {
                    None => true,
                    Some(PropValue::Text(t)) => t.trim().is_empty(),
                    Some(_) => false,
                };
                if p.required && missing {
                    found.push(prop_label(p));
                    if element && let Some(d) = &p.default {
                        let label = format!("Set {} to {}", prop_label(p), quote(&d.display()));
                        fixes.push(Fix::new(label, "element.setProperty", json!({"id": id, "key": p.key, "value": d})));
                    }
                }
            } else if let Some(why) = v.and_then(|v| misfit(p, v)) {
                found.push(format!("{} {why}", prop_label(p)));
            }
        }
        if found.is_empty() {
            return;
        }
        let what = if required {
            format!("{} has no {}.", self.item_label(id), found.join(", "))
        } else {
            format!("{}: {}.", self.item_label(id), found.join("; "))
        };
        let mut issue = self.issue(rule, what, vec![id.to_string()]);
        for f in fixes {
            issue = issue.with_fix(f);
        }
        out.push(issue);
    }

    /// One issue per tag used too often in the scope, with every element using it.
    fn duplicates(&self, rule: &RuleDecl, scope: &str, out: &mut Vec<Issue>) {
        let Some(sc) = self.scope(scope) else { return };
        for d in Registry::new(&self.doc.model, std::slice::from_ref(sc)).duplicates() {
            let names = self.name_list(&d.elements);
            let what = format!("Tag {} is used by {} elements: {names}.", quote(&d.tag), d.elements.len());
            out.push(self.issue(rule, what, d.elements));
        }
    }

    /// One issue per paired tag with no partner.
    fn unmatched(&self, rule: &RuleDecl, scope: &str, out: &mut Vec<Issue>) {
        let Some(sc) = self.scope(scope) else { return };
        for u in Registry::new(&self.doc.model, std::slice::from_ref(sc)).unmatched() {
            let what = format!("{} has no partner with tag {}.", self.el_label(&u.element), quote(&u.tag));
            out.push(self.issue(rule, what, vec![u.element]));
        }
    }

    fn scope(&self, id: &str) -> Option<&'a IdScope> {
        self.cat.packs().iter().flat_map(|p| p.identifiers.iter()).find(|s| s.id == id)
    }

    /// Elements and relationships drawn on a page whose viewpoint (one of this pack's) does not
    /// allow their type; one issue per item per page, pointing at that page's views only.
    fn viewpoint(&self, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
        for (pi, page) in self.doc.pages.iter().enumerate() {
            let Some(view) = &page.view else { continue };
            if vizcraft_model::pack_of(&view.viewpoint) != pack.id {
                continue;
            }
            let Some(vp) = pack.viewpoint(&view.viewpoint) else { continue };
            let vp_name = if vp.label.is_empty() { vizcraft_model::local_of(&vp.id) } else { vp.label.as_str() };
            // Shapes of one element on the page, grouped in z order of the first.
            let mut order: Vec<&str> = Vec::new();
            let mut items: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
            for s in &page.shapes {
                let Some(e) = s.element.as_deref().and_then(|e| self.elements.get(e)) else { continue };
                if vp.types.is_empty() || self.facts.lists(pack, &vp.types, &e.type_id) {
                    continue;
                }
                let list = items.entry(e.id.as_str()).or_default();
                if list.is_empty() {
                    order.push(e.id.as_str());
                }
                list.push((pi, s.id.as_str()));
            }
            for c in &page.connectors {
                let Some(rel) = c.relationship.as_deref().and_then(|r| self.relationships.get(r)) else { continue };
                let r = rel.id.as_str();
                if vp.relationships.is_empty() || vp.relationships.contains(&rel.type_id) {
                    continue;
                }
                let list = items.entry(r).or_default();
                if list.is_empty() {
                    order.push(r);
                }
                list.push((pi, c.id.as_str()));
            }
            for id in order {
                let what = format!("{} is not allowed on {} {vp_name} page {}.", self.item_label(id), article(vp_name), quote(&page.name));
                let its = items.remove(id).unwrap_or_default();
                out.push(self.issue_at(rule, what, vec![id.to_string()], its));
            }
        }
    }

    /// Connectors with a free end on pages that show elements of this pack.
    fn dangling(&self, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
        for (pi, page) in self.doc.pages.iter().enumerate() {
            let shows_pack = page.shapes.iter().any(|s| s.element.as_deref().and_then(|e| self.elements.get(e)).is_some_and(|e| e.pack() == pack.id));
            if !shows_pack {
                continue;
            }
            for c in &page.connectors {
                let free = |e: &End| matches!(e, End::Free { .. });
                let which = match (free(&c.start), free(&c.end)) {
                    (true, true) => "both ends",
                    (true, false) => "a free start",
                    (false, true) => "a free end",
                    (false, false) => continue,
                };
                let label = if c.text.trim().is_empty() { c.id.clone() } else { quote(&c.text) };
                let what =
                    if which == "both ends" { format!("Connector {label} has both ends free.") } else { format!("Connector {label} has {which}.") };
                let subjects: Vec<String> = c.relationship.iter().filter(|r| self.relationships.contains_key(r.as_str())).cloned().collect();
                out.push(self.issue_at(rule, what, subjects, vec![(pi, c.id.as_str())]));
            }
        }
    }

    /// `minOutgoing` (`outgoing` true) and `minIncoming`.
    #[allow(clippy::too_many_arguments)]
    fn min_count(&self, pack: &Pack, rule: &RuleDecl, types: &[String], relationship: &str, min: u32, outgoing: bool, out: &mut Vec<Issue>) {
        let rel_label = if relationship.is_empty() { "".to_string() } else { format!("{} ", self.rel_type_label(relationship)) };
        for e in self.doc.model.elements.iter().filter(|e| self.covers_elements(pack, types, &e.type_id)) {
            let list = if outgoing { self.out_of(&e.id) } else { self.in_of(&e.id) };
            let n = list.iter().filter(|r| rel_matches(relationship, r)).count();
            if u64::try_from(n).unwrap_or(u64::MAX) < u64::from(min) {
                let dir = if outgoing { "outgoing" } else { "incoming" };
                let what = format!("{} has {n} {dir} {rel_label}relationships; at least {min} expected.", self.el_label(&e.id));
                out.push(self.issue(rule, what, vec![e.id.clone()]));
            }
        }
    }

    /// Elements with both incoming and outgoing relationships of the type.
    fn exclusive(&self, pack: &Pack, rule: &RuleDecl, types: &[String], relationship: &str, out: &mut Vec<Issue>) {
        let rel_label = if relationship.is_empty() { "".to_string() } else { format!("{} ", self.rel_type_label(relationship)) };
        for e in self.doc.model.elements.iter().filter(|e| self.covers_elements(pack, types, &e.type_id)) {
            let ins: Vec<&Relationship> = self.in_of(&e.id).iter().copied().filter(|r| rel_matches(relationship, r)).collect();
            let outs: Vec<&Relationship> = self.out_of(&e.id).iter().copied().filter(|r| rel_matches(relationship, r)).collect();
            if !ins.is_empty() && !outs.is_empty() {
                let what = format!("{} has both incoming and outgoing {rel_label}relationships.", self.el_label(&e.id));
                let mut subjects = subjects_with(&e.id, &ins);
                subjects.extend(outs.iter().map(|r| r.id.clone()));
                out.push(self.issue(rule, what, subjects));
            }
        }
    }

    /// Elements of this pack that no walk along the relationship type reaches from a start: an
    /// element of a `from` type with no incoming relationship of that type.
    fn unreachable(&self, pack: &Pack, rule: &RuleDecl, from: &[String], relationship: &str, types: &[String], out: &mut Vec<Issue>) {
        let model = &self.doc.model;
        // Followed through the reasoner, so inverse relationship types count backwards.
        let g = self.facts.graph(relationship);
        let mut has_in: HashSet<&str> = HashSet::new();
        for next in g.values().flatten() {
            has_in.insert(next.0);
        }
        let starts =
            model.elements.iter().filter(|e| self.covers_elements(pack, from, &e.type_id) && !has_in.contains(e.id.as_str())).map(|e| e.id.as_str());
        let reached = crate::infer::reach(&g, starts);
        let starts: Vec<String> = if from.is_empty() { Vec::new() } else { from.iter().map(|t| self.type_label(t)).collect() };
        let start = if starts.is_empty() { "a start".to_string() } else { format!("a starting {}", starts.join(" or ")) };
        let checked = |t: &str| if types.is_empty() { self.facts.of_pack(t, pack) } else { self.facts.lists(pack, types, t) };
        for e in model.elements.iter().filter(|e| checked(&e.type_id) && !reached.contains(e.id.as_str())) {
            let what = format!("{} cannot be reached from {start}.", self.el_label(&e.id));
            out.push(self.issue(rule, what, vec![e.id.clone()]));
        }
    }

    // ---------------------------------------------------------------- graph and structure checks

    /// One issue per cycle along the relationship type (a strongly connected group of two or
    /// more elements, or an element related to itself), with its elements in model order and
    /// then the relationships inside it.
    fn cycles(&self, rule: &RuleDecl, relationship: &str, out: &mut Vec<Issue>) {
        let g = self.facts.graph(relationship);
        let nodes: Vec<&str> = self.doc.model.elements.iter().map(|e| e.id.as_str()).collect();
        let rel_label = rel_phrase(self, relationship);
        for group in strongly_connected(&nodes, &g) {
            let members: HashSet<&str> = group.iter().copied().collect();
            let looped = group.len() > 1 || group.first().is_some_and(|n| g.get(n).is_some_and(|es| es.iter().any(|(t, _)| t == n)));
            if !looped {
                continue;
            }
            let mut subjects: Vec<String> = nodes.iter().filter(|n| members.contains(*n)).map(|n| n.to_string()).collect();
            let named = self.name_list(&subjects);
            let mut rels: Vec<&str> = Vec::new();
            for n in &group {
                for (t, id) in g.get(n).map_or(&[][..], Vec::as_slice) {
                    if members.contains(t) && !rels.contains(id) {
                        rels.push(id);
                    }
                }
            }
            let order: HashMap<&str, usize> = self.doc.model.relationships.iter().enumerate().map(|(i, r)| (r.id.as_str(), i)).collect();
            rels.sort_by_key(|r| order.get(r).copied().unwrap_or(usize::MAX));
            subjects.extend(rels.iter().map(|r| r.to_string()));
            let what = format!("{named} form a cycle of {rel_label}relationships.");
            out.push(self.issue(rule, what, subjects));
        }
    }

    /// Elements of a `from` type that cannot reach an element of a `to` type (an element of a
    /// `to` type reaches itself). Walks backwards from the targets once.
    #[allow(clippy::too_many_arguments)]
    fn required_path(&self, pack: &Pack, rule: &RuleDecl, from: &[String], to: &[String], relationship: &str, out: &mut Vec<Issue>) {
        let mut back: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for (a, b, id) in self.facts.edges(relationship) {
            back.entry(b).or_default().push((a, id));
        }
        let model = &self.doc.model;
        let targets = model.elements.iter().filter(|e| self.covers_elements(pack, to, &e.type_id)).map(|e| e.id.as_str());
        let reaches = crate::infer::reach(&back, targets);
        let goal: Vec<String> = to.iter().filter(|t| *t != "*").map(|t| self.type_label(t)).collect();
        let goal = if goal.is_empty() { "an element of this notation".to_string() } else { format!("a {}", goal.join(" or ")) };
        let rel_label = rel_phrase(self, relationship);
        for e in model.elements.iter().filter(|e| self.covers_elements(pack, from, &e.type_id) && !reaches.contains(e.id.as_str())) {
            let what = format!("{} has no path of {rel_label}relationships to {goal}.", self.el_label(&e.id));
            out.push(self.issue(rule, what, vec![e.id.clone()]));
        }
    }

    /// Islands among elements of these types along the relationship type, either direction: one
    /// issue per island except the largest (the first in model order when two are as large).
    fn connected(&self, pack: &Pack, rule: &RuleDecl, types: &[String], relationship: &str, out: &mut Vec<Issue>) {
        let nodes: Vec<&str> =
            self.doc.model.elements.iter().filter(|e| self.covers_elements(pack, types, &e.type_id)).map(|e| e.id.as_str()).collect();
        let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
        let mut parent: Vec<usize> = (0..nodes.len()).collect();
        for (a, b, _) in self.facts.edges(relationship) {
            if let (Some(&i), Some(&j)) = (index.get(a), index.get(b)) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj
                    && let Some(slot) = parent.get_mut(ri.max(rj))
                {
                    *slot = ri.min(rj);
                }
            }
        }
        let mut islands: Vec<Vec<usize>> = Vec::new();
        let mut slot_of: HashMap<usize, usize> = HashMap::new();
        for i in 0..nodes.len() {
            let root = find(&mut parent, i);
            let at = *slot_of.entry(root).or_insert_with(|| {
                islands.push(Vec::new());
                islands.len() - 1
            });
            if let Some(isl) = islands.get_mut(at) {
                isl.push(i);
            }
        }
        if islands.len() < 2 {
            return;
        }
        let keep = islands.iter().enumerate().max_by(|(ia, a), (ib, b)| a.len().cmp(&b.len()).then(ib.cmp(ia))).map_or(0, |(i, _)| i);
        let rel_label = rel_phrase(self, relationship);
        for (k, isl) in islands.iter().enumerate() {
            if k == keep {
                continue;
            }
            let subjects: Vec<String> = isl.iter().filter_map(|&i| nodes.get(i)).map(|n| n.to_string()).collect();
            let what = format!(
                "{} {} not joined to the rest by {rel_label}relationships.",
                self.name_list(&subjects),
                if subjects.len() == 1 { "is" } else { "are" }
            );
            out.push(self.issue(rule, what, subjects));
        }
    }

    /// Elements owned by an element of an `owners` type whose type `children` does not list.
    fn containment(&self, pack: &Pack, rule: &RuleDecl, owners: &[String], children: &[String], out: &mut Vec<Issue>) {
        for e in &self.doc.model.elements {
            let Some(owner) = e.owner.as_deref().and_then(|o| self.elements.get(o)) else { continue };
            if !self.covers_elements(pack, owners, &owner.type_id) {
                continue;
            }
            if !children.is_empty() && self.facts.lists(pack, children, &e.type_id) {
                continue;
            }
            let what = format!("{} may not contain {}.", self.el_label(&owner.id), self.el_label(&e.id));
            out.push(self.issue(rule, what, vec![e.id.clone(), owner.id.clone()]));
        }
    }

    /// Relationships that leave through an input port or arrive through an output port, or name
    /// a port their element's type does not have.
    fn port_direction(&self, pack: &Pack, rule: &RuleDecl, relationship: &str, out: &mut Vec<Issue>) {
        for r in self.doc.model.relationships.iter().filter(|r| rel_in_rule(pack, relationship, r)) {
            let mut found: Vec<String> = Vec::new();
            for (end, port, el, bad) in
                [("starts", &r.source_port, &r.source, PortDirection::In), ("ends", &r.target_port, &r.target, PortDirection::Out)]
            {
                let (Some(port), Some(e)) = (port.as_deref(), self.elements.get(el.as_str())) else { continue };
                let ports = self.facts.ports(&e.type_id);
                if ports.is_empty() {
                    continue;
                }
                match ports.iter().find(|p| p.id == port) {
                    None => found.push(format!("{end} at port {} that {} does not have", quote(port), self.el_label(&e.id))),
                    Some(p) if p.direction == bad => {
                        let kind = if bad == PortDirection::In { "input" } else { "output" };
                        found.push(format!("{end} at the {kind} port {} of {}", quote(port_name(p)), self.el_label(&e.id)))
                    }
                    Some(_) => {}
                }
            }
            if !found.is_empty() {
                let what = format!("{} {}.", capital(&self.item_label(&r.id)), found.join(" and "));
                out.push(self.issue(rule, what, vec![r.id.clone(), r.source.clone(), r.target.clone()]));
            }
        }
    }

    /// Ports that more than `max` relationships of the type end at.
    fn port_fan_in(&self, pack: &Pack, rule: &RuleDecl, relationship: &str, max: u32, out: &mut Vec<Issue>) {
        let mut order: Vec<(&str, &str)> = Vec::new();
        let mut by_port: HashMap<(&str, &str), Vec<&Relationship>> = HashMap::new();
        for r in self.doc.model.relationships.iter().filter(|r| rel_in_rule(pack, relationship, r)) {
            let Some(port) = r.target_port.as_deref() else { continue };
            let list = by_port.entry((r.target.as_str(), port)).or_default();
            if list.is_empty() {
                order.push((r.target.as_str(), port));
            }
            list.push(r);
        }
        for key in order {
            let Some(rels) = by_port.get(&key) else { continue };
            if count_over(rels.len(), max) {
                let what = format!("Port {} of {} has {} drivers; at most {max} {}.", quote(key.1), self.el_label(key.0), rels.len(), allowed(max));
                out.push(self.issue(rule, what, subjects_with(key.0, rels)));
            }
        }
    }

    /// Elements of these types sharing a value of `property` under one owner.
    #[allow(clippy::too_many_arguments)]
    fn unique_property(&self, pack: &Pack, rule: &RuleDecl, types: &[String], property: &str, within: &[String], out: &mut Vec<Issue>) {
        let mut order: Vec<(Option<&str>, String)> = Vec::new();
        let mut groups: HashMap<(Option<&str>, String), Vec<String>> = HashMap::new();
        for e in self.doc.model.elements.iter().filter(|e| self.covers_elements(pack, types, &e.type_id)) {
            let Some(v) = e.properties.get(property) else { continue };
            if matches!(v, PropValue::Text(t) if t.trim().is_empty()) {
                continue;
            }
            let owner: Option<&str> = if within.is_empty() {
                e.owner.as_deref()
            } else {
                self.facts.ancestors(&e.id).into_iter().find(|a| self.elements.get(a).is_some_and(|o| self.facts.lists(pack, within, &o.type_id)))
            };
            let key = (owner, serde_json::to_string(v).unwrap_or_default());
            let list = groups.entry(key.clone()).or_default();
            if list.is_empty() {
                order.push(key);
            }
            list.push(e.id.clone());
        }
        for key in order {
            let Some(ids) = groups.remove(&key) else { continue };
            if ids.len() < 2 {
                continue;
            }
            let scope = match key.0 {
                Some(o) => format!(" in {}", self.el_label(o)),
                None => String::new(),
            };
            let shown = ids
                .first()
                .and_then(|id| self.elements.get(id.as_str()))
                .and_then(|e| e.properties.get(property))
                .map(|v| quote(&v.display()))
                .unwrap_or_default();
            let what = format!("{} {} share {property} {shown}{scope}: {}.", ids.len(), "elements", self.name_list(&ids));
            out.push(self.issue(rule, what, ids));
        }
    }

    /// Elements whose `property` does not compare as `op` against `other`.
    #[allow(clippy::too_many_arguments)]
    fn compare(
        &self,
        pack: &Pack,
        rule: &RuleDecl,
        types: &[String],
        property: &str,
        op: CompareOp,
        other: Option<&PropValue>,
        out: &mut Vec<Issue>,
    ) {
        let Some(other) = other else { return };
        for e in self.doc.model.elements.iter().filter(|e| self.covers_elements(pack, types, &e.type_id)) {
            let Some(left) = e.properties.get(property) else { continue };
            // Text naming a property of the type (or one the element has) is that property.
            let (right, right_name) = match other {
                PropValue::Text(key) if e.properties.contains_key(key) || self.facts.properties(&e.type_id).iter().any(|p| &p.key == key) => {
                    match e.properties.get(key) {
                        Some(v) => (v, Some(key.as_str())),
                        None => continue,
                    }
                }
                v => (v, None),
            };
            let Some(holds) = compare_values(left, op, right) else { continue };
            if holds {
                continue;
            }
            let rhs = match right_name {
                Some(k) => format!("{k} ({})", right.display()),
                None => quote(&right.display()),
            };
            let what = format!("{}: {property} is {}, which should be {} {rhs}.", self.el_label(&e.id), quote(&left.display()), op.symbol());
            out.push(self.issue(rule, what, vec![e.id.clone()]));
        }
    }

    /// Named elements of these types whose name does not match the glob.
    fn name_format(&self, pack: &Pack, rule: &RuleDecl, types: &[String], pattern: &str, out: &mut Vec<Issue>) {
        let Some(glob) = crate::glob::Glob::parse(pattern) else { return };
        for e in self.doc.model.elements.iter().filter(|e| self.covers_elements(pack, types, &e.type_id)) {
            let name = e.name.trim();
            if name.is_empty() || glob.matches(name) {
                continue;
            }
            let what = format!("{} does not match the name pattern {}.", self.el_label(&e.id), quote(pattern));
            out.push(self.issue(rule, what, vec![e.id.clone()]));
        }
    }

    // ---------------------------------------------------------------- issues and labels

    /// An issue about `subjects`, pointing at every view item that draws them.
    fn issue(&self, rule: &RuleDecl, what: String, subjects: Vec<String>) -> Issue {
        let mut items: Vec<(usize, &str)> = Vec::new();
        for s in &subjects {
            if let Some(v) = self.views.get(s.as_str()) {
                items.extend(v.iter().copied());
            }
        }
        self.issue_at(rule, what, subjects, items)
    }

    /// An issue about `subjects` pointing at exactly `items`; the page is set when every item is
    /// on one page.
    fn issue_at(&self, rule: &RuleDecl, what: String, subjects: Vec<String>, items: Vec<(usize, &str)>) -> Issue {
        let mut seen = HashSet::new();
        let items: Vec<(usize, &str)> = items.into_iter().filter(|(_, id)| seen.insert(*id)).collect();
        let page = match items.first() {
            Some((p, _)) if items.iter().all(|(q, _)| q == p) => self.doc.pages.get(*p).map(|pg| pg.id.clone()),
            _ => None,
        };
        let mut issue = Issue::new(rule.id.clone(), rule.severity, message(&rule.description, &what))
            .with_subjects(subjects)
            .with_items(items.into_iter().map(|(_, id)| id.to_string()).collect());
        issue.page = page;
        issue
    }

    /// An element as a message names it: its type and its name, else tag, else id.
    fn el_label(&self, id: &str) -> String {
        match self.elements.get(id) {
            Some(e) => format!("{} {}", self.type_label(&e.type_id), handle(e)),
            None => quote(id),
        }
    }

    /// An element or relationship as a message names it.
    fn item_label(&self, id: &str) -> String {
        if self.elements.contains_key(id) {
            return self.el_label(id);
        }
        match self.relationships.get(id) {
            Some(r) => format!("{} from {} to {}", self.rel_type_label(&r.type_id), self.el_label(&r.source), self.el_label(&r.target)),
            None => quote(id),
        }
    }

    /// Up to [`MAX_NAMED`] elements by name, then how many more.
    fn name_list(&self, ids: &[String]) -> String {
        let mut parts: Vec<String> = ids
            .iter()
            .take(MAX_NAMED)
            .map(|id| self.elements.get(id.as_str()).map_or_else(|| quote(id), |e| self.type_label(&e.type_id) + " " + &handle(e)))
            .collect();
        if ids.len() > MAX_NAMED {
            parts.push(format!("and {} more", ids.len().saturating_sub(MAX_NAMED)));
        }
        parts.join(", ")
    }

    fn type_label(&self, type_id: &str) -> String {
        match self.element_type(type_id).or_else(|| self.cat.element_type(type_id).map(|(_, t)| t)) {
            Some(t) if !t.label.is_empty() => t.label.clone(),
            _ => cut(vizcraft_model::local_of(type_id), MAX_LABEL),
        }
    }

    fn rel_type_label(&self, type_id: &str) -> String {
        match self.cat.relationship_type(type_id) {
            Some((_, t)) if !t.label.is_empty() => t.label.clone(),
            _ => cut(vizcraft_model::local_of(type_id), MAX_LABEL),
        }
    }
}

/// "Flow " for a named relationship type, nothing for any relationship.
fn rel_phrase(ctx: &Ctx<'_>, relationship: &str) -> String {
    if relationship.is_empty() { String::new() } else { format!("{} ", ctx.rel_type_label(relationship)) }
}

/// Whether `r` is a relationship the rule is about: of type `want`, or with no type named, of
/// this pack.
fn rel_in_rule(pack: &Pack, want: &str, r: &Relationship) -> bool {
    if want.is_empty() { r.pack() == pack.id } else { r.type_id == want }
}

fn port_name(p: &crate::schema::Port) -> &str {
    if p.label.is_empty() { &p.id } else { &p.label }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The root of `i` in a union-find forest, halving paths on the way.
fn find(parent: &mut [usize], mut i: usize) -> usize {
    let mut steps = 0usize;
    while let Some(&p) = parent.get(i) {
        if p == i || steps > parent.len() {
            break;
        }
        let gp = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(i) {
            *slot = gp;
        }
        i = p;
        steps += 1;
    }
    i
}

/// Strongly connected groups of `g` over `nodes` (Tarjan's algorithm, iterative so a long chain
/// cannot overflow the stack), each in the order found.
fn strongly_connected<'a>(nodes: &[&'a str], g: &HashMap<&'a str, Vec<(&'a str, &'a str)>>) -> Vec<Vec<&'a str>> {
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut low: HashMap<&str, usize> = HashMap::new();
    let mut on_stack: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut out: Vec<Vec<&str>> = Vec::new();
    let mut next = 0usize;
    let empty: Vec<(&str, &str)> = Vec::new();
    for &root in nodes {
        if index.contains_key(root) {
            continue;
        }
        index.insert(root, next);
        low.insert(root, next);
        next += 1;
        stack.push(root);
        on_stack.insert(root);
        let mut work: Vec<(&str, usize)> = vec![(root, 0)];
        while let Some(&(v, i)) = work.last() {
            let edges = g.get(v).unwrap_or(&empty);
            if let Some(&(w, _)) = edges.get(i) {
                if let Some(top) = work.last_mut() {
                    top.1 = i + 1;
                }
                if !index.contains_key(w) {
                    index.insert(w, next);
                    low.insert(w, next);
                    next += 1;
                    stack.push(w);
                    on_stack.insert(w);
                    work.push((w, 0));
                } else if on_stack.contains(w) {
                    let iw = index.get(w).copied().unwrap_or(usize::MAX);
                    if let Some(lv) = low.get_mut(v) {
                        *lv = (*lv).min(iw);
                    }
                }
                continue;
            }
            work.pop();
            let lv = low.get(v).copied().unwrap_or(usize::MAX);
            if let Some(&(u, _)) = work.last()
                && let Some(lu) = low.get_mut(u)
            {
                *lu = (*lu).min(lv);
            }
            if Some(&lv) == index.get(v) {
                let mut group = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack.remove(w);
                    group.push(w);
                    if w == v {
                        break;
                    }
                }
                out.push(group);
            }
        }
    }
    out
}

/// Whether `left op right` holds: numbers by value, text by character order, flags for equal
/// and not equal only; `None` when the values cannot be compared.
fn compare_values(left: &PropValue, op: CompareOp, right: &PropValue) -> Option<bool> {
    use std::cmp::Ordering;
    let ord: Ordering = match (left, right) {
        (PropValue::Number(a), PropValue::Number(b)) => a.partial_cmp(b)?,
        (PropValue::Text(a), PropValue::Text(b)) => a.cmp(b),
        (PropValue::Bool(a), PropValue::Bool(b)) => match op {
            CompareOp::Eq => return Some(a == b),
            CompareOp::Ne => return Some(a != b),
            _ => return None,
        },
        _ => return None,
    };
    Some(match op {
        CompareOp::Lt => ord == Ordering::Less,
        CompareOp::Le => ord != Ordering::Greater,
        CompareOp::Eq => ord == Ordering::Equal,
        CompareOp::Ge => ord != Ordering::Less,
        CompareOp::Gt => ord == Ordering::Greater,
        CompareOp::Ne => ord != Ordering::Equal,
    })
}

/// An empty relationship type in a rule counts every relationship.
fn rel_matches(want: &str, r: &Relationship) -> bool {
    want.is_empty() || r.type_id == want
}

/// Whether `n` is more than `max`.
fn count_over(n: usize, max: u32) -> bool {
    u64::try_from(n).unwrap_or(u64::MAX) > u64::from(max)
}

fn allowed(n: u32) -> &'static str {
    if n == 1 { "is allowed" } else { "are allowed" }
}

fn subjects_with(first: &str, rels: &[&Relationship]) -> Vec<String> {
    std::iter::once(first.to_string()).chain(rels.iter().map(|r| r.id.clone())).collect()
}

/// Why `v` does not fit `p`, or `None` when it fits.
fn misfit(p: &PropertySchema, v: &PropValue) -> Option<String> {
    let shown = quote(&v.display());
    match (p.kind, v) {
        (PropKind::Text, PropValue::Text(_)) | (PropKind::Number, PropValue::Number(_)) | (PropKind::Bool, PropValue::Bool(_)) => None,
        (PropKind::Integer, PropValue::Number(n)) if n.is_finite() && n.fract() == 0.0 => None,
        (PropKind::Enum, PropValue::Text(t)) if p.values.is_empty() || p.values.contains(t) => None,
        (PropKind::Enum, PropValue::Text(_)) => {
            let values: Vec<String> = p.values.iter().take(MAX_NAMED).map(|x| quote(x)).collect();
            let more = if p.values.len() > MAX_NAMED { ", ..." } else { "" };
            Some(format!("is {shown}, which is not one of {}{more}", values.join(", ")))
        }
        (PropKind::Text, _) => Some(format!("should be text, not {shown}")),
        (PropKind::Number, _) => Some(format!("should be a number, not {shown}")),
        (PropKind::Integer, _) => Some(format!("should be a whole number, not {shown}")),
        (PropKind::Bool, _) => Some(format!("should be true or false, not {shown}")),
        (PropKind::Enum, _) => Some(format!("should be one of its listed values, not {shown}")),
    }
}

fn prop_label(p: &PropertySchema) -> String {
    cut(if p.label.is_empty() { &p.key } else { &p.label }, MAX_LABEL)
}

/// How a message names an element after its type: the quoted name, else the tag, else the id.
fn handle(e: &Element) -> String {
    if !e.name.trim().is_empty() {
        quote(e.name.trim())
    } else if !e.tag.is_empty() {
        cut(&e.tag, MAX_LABEL)
    } else {
        cut(&e.id, MAX_LABEL)
    }
}

/// `text` in double quotes, cut to [`MAX_LABEL`] and on one line.
fn quote(text: &str) -> String {
    let one_line: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    format!("\"{}\"", cut(&one_line, MAX_LABEL))
}

/// `text` cut to at most `max` bytes at a character boundary, with `...` when cut.
pub(crate) fn cut(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max.saturating_sub(3);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", text.get(..end).unwrap_or_default())
}

/// The rule's description followed by the specific finding, cut to [`MAX_MESSAGE`].
pub(crate) fn message(description: &str, what: &str) -> String {
    let d = description.trim();
    let full = if d.is_empty() { what.to_string() } else { format!("{d} {what}") };
    cut(&full, MAX_MESSAGE)
}

fn article(word: &str) -> &'static str {
    match word.chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

#[cfg(test)]
#[path = "test_doc.rs"]
pub(crate) mod test_doc;

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
