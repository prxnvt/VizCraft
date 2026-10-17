//! The rules engine: checks declared by packs, run over a document.
//!
//! Live rules (`RuleMode::Live`) run after every command that changes the document and must stay
//! cheap (linear in the model and the views). Batch rules run on request over a scope. Each rule
//! declared by a pack the model uses runs once; its findings become [`Issue`]s carrying the rule's
//! id and severity, the element and relationship ids involved and the view items that draw them.
//! Results are sorted by severity, then rule id, then first subject, so equal documents give equal
//! lists.
//!
//! The packs whose rules run are those listed in `doc.model.packs` plus the pack of every element
//! and relationship type present, looked up in the catalog (unknown packs are skipped). Every
//! lookup a check needs (elements by id, relationships by source and by target, view items by
//! model id, element types) is indexed once per call, so the live checks stay linear in the size
//! of the model and its views; only the batch `unreachable` check walks the graph.

use std::collections::{HashMap, HashSet, VecDeque};

use vizcraft_diagram::{Document, End};
use vizcraft_model::{Element, IdScope, Issue, PropValue, Registry, Relationship};

use crate::schema::{Check, ElementType, Pack, PropKind, PropertySchema, RuleDecl, RuleMode};
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

/// Live issues for `doc`: every live rule of every pack the model uses.
pub fn live(doc: &Document, cat: &Catalog) -> Vec<Issue> {
    run(doc, cat, RuleMode::Live)
}

/// Batch issues for `doc` within `scope`: every batch rule of every pack the model uses, with
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

/// Every rule of `mode` from every pack in use, sorted.
fn run(doc: &Document, cat: &Catalog, mode: RuleMode) -> Vec<Issue> {
    let ctx = Ctx::new(doc, cat);
    let mut out = Vec::new();
    for pack in ctx.packs() {
        for rule in pack.rules.iter().filter(|r| r.mode == mode) {
            let mut found = Vec::new();
            ctx.check(pack, rule, &mut found);
            found.truncate(MAX_PER_RULE);
            out.extend(found);
        }
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
        Ctx { doc, cat, elements, relationships, out_by, in_by, views, types }
    }

    /// The packs whose rules run, in catalog order.
    fn packs(&self) -> Vec<&'a Pack> {
        let model = &self.doc.model;
        let mut used: HashSet<&str> = model.packs.iter().map(|p| p.id.as_str()).collect();
        used.extend(model.elements.iter().map(|e| e.pack()));
        used.extend(model.relationships.iter().map(|r| r.pack()));
        self.cat.packs().iter().filter(|p| used.contains(p.id.as_str())).collect()
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
            Check::Unreachable { from, relationship } => self.unreachable(pack, rule, from, relationship, out),
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
                    (Some(s), Some(t)) if !legal::is_legal(self.cat, &s.type_id, &r.type_id, &t.type_id) => Some("is not allowed"),
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
                    && pack.lists(&row.source, &e.type_id)
                {
                    let hits: Vec<&Relationship> = self
                        .out_of(&e.id)
                        .iter()
                        .copied()
                        .filter(|r| {
                            r.type_id == row.relationship && self.elements.get(r.target.as_str()).is_some_and(|t| pack.lists(&row.target, &t.type_id))
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
                    && pack.lists(&row.target, &e.type_id)
                {
                    let hits: Vec<&Relationship> = self
                        .in_of(&e.id)
                        .iter()
                        .copied()
                        .filter(|r| {
                            r.type_id == row.relationship && self.elements.get(r.source.as_str()).is_some_and(|s| pack.lists(&row.source, &s.type_id))
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
        for e in self.doc.model.elements.iter().filter(|e| e.pack() == pack.id && covers(pack, types, &e.type_id)) {
            let Some(t) = self.element_type(&e.type_id) else { continue };
            self.check_props(rule, &e.id, &t.properties, &e.properties, required, out);
        }
        for r in self.doc.model.relationships.iter().filter(|r| r.pack() == pack.id && covers(pack, types, &r.type_id)) {
            let Some(t) = pack.relationship_type(&r.type_id) else { continue };
            self.check_props(rule, &r.id, &t.properties, &r.properties, required, out);
        }
    }

    fn check_props(
        &self,
        rule: &RuleDecl,
        id: &str,
        schema: &[PropertySchema],
        props: &std::collections::BTreeMap<String, PropValue>,
        required: bool,
        out: &mut Vec<Issue>,
    ) {
        // All findings about one item go in one issue: the issue key (rule, subjects, items) must
        // tell issues apart, and two issues about the same item would share it.
        let mut found: Vec<String> = Vec::new();
        for p in schema {
            let v = props.get(&p.key);
            if required {
                let missing = match v {
                    None => true,
                    Some(PropValue::Text(t)) => t.trim().is_empty(),
                    Some(_) => false,
                };
                if p.required && missing {
                    found.push(prop_label(p));
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
        out.push(self.issue(rule, what, vec![id.to_string()]));
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
                if vp.types.is_empty() || pack.lists(&vp.types, &e.type_id) {
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
        for e in self.doc.model.elements.iter().filter(|e| covers_elements(pack, types, &e.type_id)) {
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
        for e in self.doc.model.elements.iter().filter(|e| covers_elements(pack, types, &e.type_id)) {
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
    fn unreachable(&self, pack: &Pack, rule: &RuleDecl, from: &[String], relationship: &str, out: &mut Vec<Issue>) {
        let model = &self.doc.model;
        let mut reached: HashSet<&str> = HashSet::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        for e in model.elements.iter().filter(|e| covers_elements(pack, from, &e.type_id)) {
            if !self.in_of(&e.id).iter().any(|r| rel_matches(relationship, r)) && reached.insert(e.id.as_str()) {
                queue.push_back(e.id.as_str());
            }
        }
        while let Some(id) = queue.pop_front() {
            for r in self.out_of(id).iter().filter(|r| rel_matches(relationship, r)) {
                if self.elements.contains_key(r.target.as_str()) && reached.insert(r.target.as_str()) {
                    queue.push_back(r.target.as_str());
                }
            }
        }
        let starts: Vec<String> = if from.is_empty() { Vec::new() } else { from.iter().map(|t| self.type_label(t)).collect() };
        let start = if starts.is_empty() { "a start".to_string() } else { format!("a starting {}", starts.join(" or ")) };
        for e in model.elements.iter().filter(|e| e.pack() == pack.id && !reached.contains(e.id.as_str())) {
            let what = format!("{} cannot be reached from {start}.", self.el_label(&e.id));
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

/// Whether the rule's `types` list covers an element or relationship type: an empty list covers
/// every element and relationship type of the pack; otherwise `Pack::lists` (`*` for every
/// element type of the pack, or the exact id).
fn covers(pack: &Pack, types: &[String], type_id: &str) -> bool {
    if types.is_empty() { pack.element_type(type_id).is_some() || pack.relationship_type(type_id).is_some() } else { pack.lists(types, type_id) }
}

/// Like [`covers`] for checks about elements only: an empty list is every element type of the pack.
fn covers_elements(pack: &Pack, types: &[String], type_id: &str) -> bool {
    if types.is_empty() { pack.element_type(type_id).is_some() } else { pack.lists(types, type_id) }
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
fn cut(text: &str, max: usize) -> String {
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
fn message(description: &str, what: &str) -> String {
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
