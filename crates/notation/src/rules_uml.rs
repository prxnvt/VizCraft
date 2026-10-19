//! The checks first needed by the UML pack, kept general so other packs use them too: ends that
//! must show a property (navigability, crow's foot cardinality), multiplicity syntax, at most n
//! elements per owner, and unique member names. [`check`] is the rules engine's one dispatch arm
//! for them.

use std::collections::HashMap;

use serde_json::json;
use vizcraft_diagram::Document;
use vizcraft_model::issue::Fix;
use vizcraft_model::{Element, Issue, PropValue};

use crate::infer::Facts;
use crate::rules::{cut, message};
use crate::schema::{Check, EndMarker, Pack, RuleDecl};
use crate::uml_text::{MemberKind, parse_member, parse_multiplicity, rows};
use crate::{Catalog, uml_text};

/// Longest name quoted in a message, in bytes.
const MAX_LABEL: usize = 60;

/// Run one of the checks of this module; other checks are ignored.
pub(crate) fn check(doc: &Document, cat: &Catalog, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let ctx = Ctx::new(doc, cat);
    match &rule.check {
        Check::EndMarkerMatchesProperty { relationship, property, markers } => ctx.end_markers(rule, relationship, property, markers, out),
        Check::MultiplicitySyntax { types, properties, members } => ctx.multiplicities(pack, rule, types, properties, members, out),
        Check::MaxPerOwner { types, within, max } => ctx.max_per_owner(pack, rule, types, within, *max, out),
        Check::UniqueMembers { types, property } => ctx.unique_members(pack, rule, types, property, out),
        _ => {}
    }
}

struct Ctx<'a> {
    doc: &'a Document,
    cat: &'a Catalog,
    facts: Facts<'a>,
    /// View items by model id: (page index, shape or connector id).
    views: HashMap<&'a str, Vec<(usize, &'a str)>>,
}

impl<'a> Ctx<'a> {
    fn new(doc: &'a Document, cat: &'a Catalog) -> Self {
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
        Ctx { doc, cat, facts: Facts::new(&doc.model, cat), views }
    }

    fn covers(&self, pack: &Pack, types: &[String], type_id: &str) -> bool {
        if types.is_empty() { self.facts.of_pack(type_id, pack) } else { self.facts.lists(pack, types, type_id) }
    }

    fn issue(&self, rule: &RuleDecl, what: &str, subjects: Vec<String>, items: Option<Vec<(usize, &str)>>) -> Issue {
        let items = items.unwrap_or_else(|| subjects.iter().flat_map(|s| self.views.get(s.as_str()).cloned().unwrap_or_default()).collect());
        let page = match items.first() {
            Some((p, _)) if items.iter().all(|(q, _)| q == p) => self.doc.pages.get(*p).map(|pg| pg.id.clone()),
            _ => None,
        };
        let mut i = Issue::new(rule.id.clone(), rule.severity, message(&rule.description, what))
            .with_subjects(subjects)
            .with_items(items.into_iter().map(|(_, id)| id.to_string()).collect());
        i.page = page;
        i
    }

    fn label(&self, e: &Element) -> String {
        let ty = self.cat.element_type(&e.type_id).map(|(_, t)| t.label.clone()).filter(|l| !l.is_empty());
        let ty = ty.unwrap_or_else(|| vizcraft_model::local_of(&e.type_id).to_string());
        let handle = if e.name.trim().is_empty() { e.id.clone() } else { format!("\"{}\"", one_line(e.name.trim())) };
        format!("{} {}", cut(&ty, MAX_LABEL), handle)
    }

    /// Relationships whose connector ends do not show their property's marker.
    fn end_markers(&self, rule: &RuleDecl, relationship: &str, property: &str, markers: &[EndMarker], out: &mut Vec<Issue>) {
        let default = self.cat.relationship_type(relationship).and_then(|(_, t)| t.property(property)).and_then(|p| p.default.clone());
        for r in self.doc.model.relationships.iter().filter(|r| r.type_id == relationship) {
            let Some(value) = r.prop(property).or(default.as_ref()).map(PropValue::display) else { continue };
            let Some(m) = markers.iter().find(|m| m.value == value) else { continue };
            for (pi, page) in self.doc.pages.iter().enumerate() {
                for c in page.connectors.iter().filter(|c| c.relationship.as_deref() == Some(r.id.as_str())) {
                    let start_ok = m.start.as_deref().is_none_or(|a| c.start_arrow.name() == a);
                    let end_ok = m.end.as_deref().is_none_or(|a| c.end_arrow.name() == a);
                    if start_ok && end_ok {
                        continue;
                    }
                    let what = format!("{property} is \"{}\", so the ends should be drawn to match.", one_line(&value));
                    let mut params = json!({"ids": [c.id]});
                    if let Some(a) = &m.start {
                        params["start"] = json!(a);
                    }
                    if let Some(a) = &m.end {
                        params["end"] = json!(a);
                    }
                    let issue = self.issue(rule, &what, vec![r.id.clone()], Some(vec![(pi, c.id.as_str())]));
                    out.push(issue.with_fix(Fix::new(format!("Draw the ends for {property} {}", one_line(&value)), "connector.setArrows", params)));
                }
            }
        }
    }

    /// Multiplicities that do not read, in properties and member rows.
    fn multiplicities(&self, pack: &Pack, rule: &RuleDecl, types: &[String], properties: &[String], members: &[String], out: &mut Vec<Issue>) {
        let rel_covered = |t: &str| if types.is_empty() { pack.relationship_type(t).is_some() } else { types.iter().any(|x| x == t) };
        let found = |id: &str, label: String, props: &std::collections::BTreeMap<String, PropValue>, out: &mut Vec<Issue>| {
            for k in properties {
                let Some(PropValue::Text(t)) = props.get(k) else { continue };
                if t.trim().is_empty() {
                    continue;
                }
                if let Err(why) = parse_multiplicity(t) {
                    let what = format!("{label}: {k} \"{}\" is not a multiplicity: {why}.", one_line(cut(t, MAX_LABEL).as_str()));
                    out.push(self.issue(rule, &what, vec![id.to_string()], None));
                }
            }
            for k in members {
                let Some(PropValue::Text(t)) = props.get(k) else { continue };
                for row in rows(t).take(vizcraft_diagram::compartment::MAX_ROWS) {
                    let m = parse_member(row);
                    if m.multiplicity.is_empty() {
                        continue;
                    }
                    if let Err(why) = parse_multiplicity(&m.multiplicity) {
                        let what = format!(
                            "{label}: member {} has multiplicity \"{}\", which does not read: {why}.",
                            quote(&m.name),
                            one_line(&m.multiplicity)
                        );
                        out.push(self.issue(rule, &what, vec![id.to_string()], None));
                    }
                }
            }
        };
        for e in self.doc.model.elements.iter().filter(|e| self.covers(pack, types, &e.type_id)) {
            found(&e.id, self.label(e), &e.properties, out);
        }
        for r in self.doc.model.relationships.iter().filter(|r| rel_covered(&r.type_id)) {
            let label = self.cat.relationship_type(&r.type_id).map(|(_, t)| t.label.clone()).unwrap_or_else(|| r.type_id.clone());
            found(&r.id, cut(&label, MAX_LABEL), &r.properties, out);
        }
    }

    /// More than `max` elements of these types under one owner.
    fn max_per_owner(&self, pack: &Pack, rule: &RuleDecl, types: &[String], within: &[String], max: u32, out: &mut Vec<Issue>) {
        let mut order: Vec<String> = Vec::new();
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for e in self.doc.model.elements.iter().filter(|e| self.facts.lists(pack, types, &e.type_id)) {
            let owner =
                self.facts.ancestors(&e.id).into_iter().find(|a| self.facts.element(a).is_some_and(|o| self.facts.lists(pack, within, &o.type_id)));
            let key = match owner {
                Some(o) => format!("e:{o}"),
                None => match self.views.get(e.id.as_str()).and_then(|v| v.first()) {
                    Some((pi, _)) => format!("p:{pi}"),
                    None => "none".to_string(),
                },
            };
            let list = groups.entry(key.clone()).or_default();
            if list.is_empty() {
                order.push(key);
            }
            list.push(e.id.clone());
        }
        for key in order {
            let Some(ids) = groups.remove(&key) else { continue };
            if u64::try_from(ids.len()).unwrap_or(u64::MAX) <= u64::from(max) {
                continue;
            }
            let place = match key.strip_prefix("e:").and_then(|o| self.facts.element(o)) {
                Some(o) => format!("in {}", self.label(o)),
                None => match key.strip_prefix("p:").and_then(|p| p.parse::<usize>().ok()).and_then(|p| self.doc.pages.get(p)) {
                    Some(p) => format!("on page \"{}\"", one_line(&cut(&p.name, MAX_LABEL))),
                    None => "outside any owner".to_string(),
                },
            };
            let names: Vec<String> = ids.iter().take(5).filter_map(|id| self.facts.element(id)).map(|e| self.label(e)).collect();
            let what = format!("{} elements {place}, at most {max} allowed: {}.", ids.len(), names.join(", "));
            out.push(self.issue(rule, &what, ids, None));
        }
    }

    /// Attribute names used twice in one element's member rows.
    fn unique_members(&self, pack: &Pack, rule: &RuleDecl, types: &[String], property: &str, out: &mut Vec<Issue>) {
        for e in self.doc.model.elements.iter().filter(|e| self.covers(pack, types, &e.type_id)) {
            let Some(PropValue::Text(t)) = e.properties.get(property) else { continue };
            let mut seen: HashMap<String, usize> = HashMap::new();
            let mut order: Vec<String> = Vec::new();
            for row in rows(t).take(vizcraft_diagram::compartment::MAX_ROWS) {
                let m: uml_text::Member = parse_member(row);
                if m.kind != MemberKind::Attribute || m.name.is_empty() {
                    continue;
                }
                let n = seen.entry(m.name.clone()).or_insert(0);
                *n = n.saturating_add(1);
                if *n == 2 {
                    order.push(m.name);
                }
            }
            for name in order {
                let what = format!("{} declares {} more than once.", self.label(e), quote(&name));
                out.push(self.issue(rule, &what, vec![e.id.clone()], None));
            }
        }
    }
}

fn one_line(t: &str) -> String {
    t.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

fn quote(t: &str) -> String {
    format!("\"{}\"", one_line(&cut(t, MAX_LABEL)))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::{Arrow, Document};
    use vizcraft_model::{Element, PropValue, Relationship};

    use crate::rules::test_doc::{glued, glyph_shape};
    use crate::schema::{Check, EndMarker, RuleDecl};

    fn pack() -> crate::Pack {
        crate::loader::load_json(
            &json!({
                "id": "rut", "version": "1.0.0", "name": "Rules test",
                "elementTypes": [{"id": "rut.Box", "properties": [{"key": "attributes"}, {"key": "size"}]}, {"id": "rut.Frame", "container": true}],
                "relationshipTypes": [{"id": "rut.Link", "properties": [
                    {"key": "nav", "kind": "enum", "values": ["none", "target"], "default": "none"}, {"key": "mult"}]}],
                "connections": [{"source": ["*"], "relationship": "rut.Link", "target": ["*"]}]
            })
            .to_string(),
        )
        .unwrap()
    }

    fn rule(check: Check) -> RuleDecl {
        RuleDecl { id: "rut.rule".into(), check, ..RuleDecl::default() }
    }

    fn run(doc: &Document, pack: &crate::Pack, check: Check) -> Vec<vizcraft_model::Issue> {
        let cat = crate::Catalog::new(vec![pack.clone()]);
        let mut out = Vec::new();
        super::check(doc, &cat, pack, &rule(check), &mut out);
        out
    }

    fn el(doc: &mut Document, id: &str, ty: &str, props: &[(&str, &str)]) {
        let mut e = Element::new(id, ty);
        e.name = id.to_uppercase();
        for (k, v) in props {
            e.properties.insert((*k).into(), PropValue::Text((*v).into()));
        }
        doc.model.add_element(e).unwrap();
    }

    #[test]
    fn end_markers_match_the_property_and_offer_set_arrows() {
        let p = pack();
        let mut d = Document::new();
        el(&mut d, "a", "rut.Box", &[]);
        el(&mut d, "b", "rut.Box", &[]);
        let mut r = Relationship::new("r1", "rut.Link", "a", "b");
        r.properties.insert("nav".into(), PropValue::Text("target".into()));
        d.model.add_relationship(r).unwrap();
        d.pages[0].shapes.push(glyph_shape("s_a", "rut.Box", 0.0, 0.0));
        d.pages[0].shapes.push(glyph_shape("s_b", "rut.Box", 300.0, 0.0));
        let mut c = glued("c1", "s_a", "s_b");
        c.relationship = Some("r1".into());
        c.end_arrow = Arrow::None;
        d.pages[0].connectors.push(c);
        let markers = vec![
            EndMarker { value: "none".into(), start: Some("none".into()), end: Some("none".into()) },
            EndMarker { value: "target".into(), start: None, end: Some("open".into()) },
        ];
        let check = Check::EndMarkerMatchesProperty { relationship: "rut.Link".into(), property: "nav".into(), markers };
        let found = run(&d, &p, check.clone());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].items, ["c1"]);
        assert_eq!(found[0].fixes[0].command, "connector.setArrows");
        assert_eq!(found[0].fixes[0].params, json!({"ids": ["c1"], "end": "open"}));
        d.pages[0].connectors[0].end_arrow = Arrow::Open;
        assert!(run(&d, &p, check.clone()).is_empty());
        // The default value counts when the property is missing.
        d.model.relationships[0].properties.clear();
        assert_eq!(run(&d, &p, check).len(), 1);
    }

    #[test]
    fn multiplicity_syntax_reads_properties_and_member_rows() {
        let p = pack();
        let mut d = Document::new();
        el(&mut d, "a", "rut.Box", &[("size", "5..2"), ("attributes", "+x: Int [0..1]\n+y: Int [..]\n+z: Int")]);
        el(&mut d, "b", "rut.Box", &[("size", "1..*")]);
        let mut r = Relationship::new("r1", "rut.Link", "a", "b");
        r.properties.insert("mult".into(), PropValue::Text("-1".into()));
        d.model.add_relationship(r).unwrap();
        let check = Check::MultiplicitySyntax { types: vec![], properties: vec!["size".into(), "mult".into()], members: vec!["attributes".into()] };
        let found = run(&d, &p, check);
        let subjects: Vec<&str> = found.iter().map(|i| i.subjects[0].as_str()).collect();
        assert_eq!(subjects, ["a", "a", "r1"], "{found:?}");
        assert!(found[0].message.contains("lower bound 5"), "{}", found[0].message);
        assert!(found[1].message.contains("\"y\""), "{}", found[1].message);
    }

    #[test]
    fn max_per_owner_groups_by_owner_then_page() {
        let p = pack();
        let mut d = Document::new();
        el(&mut d, "f", "rut.Frame", &[]);
        for id in ["a", "b", "c"] {
            el(&mut d, id, "rut.Box", &[]);
        }
        d.model.elements.iter_mut().filter(|e| e.id == "a" || e.id == "b").for_each(|e| e.owner = Some("f".into()));
        let check = Check::MaxPerOwner { types: vec!["rut.Box".into()], within: vec!["rut.Frame".into()], max: 1 };
        let found = run(&d, &p, check.clone());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].subjects, ["a", "b"]);
        // Unowned ones group by the page they are drawn on.
        d.model.elements.iter_mut().for_each(|e| e.owner = None);
        d.pages[0].shapes.push(glyph_shape("s_a", "rut.Box", 0.0, 0.0));
        d.pages[0].shapes[0].element = Some("a".into());
        d.pages[0].shapes.push(glyph_shape("s_b", "rut.Box", 0.0, 0.0));
        d.pages[0].shapes[1].element = Some("b".into());
        let found = run(&d, &p, check);
        assert_eq!(found.len(), 1, "a and b share the page; c is drawn nowhere");
        assert_eq!(found[0].subjects, ["a", "b"]);
    }

    #[test]
    fn unique_members_skips_operations() {
        let p = pack();
        let mut d = Document::new();
        el(&mut d, "a", "rut.Box", &[("attributes", "+x: Int\n-x: String\n+f()\n+f(a: Int)\n+y")]);
        let found = run(&d, &p, Check::UniqueMembers { types: vec![], property: "attributes".into() });
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("\"x\""));
    }
}
