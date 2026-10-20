//! The checks of the `mechanical` pack (kinematic diagrams, power transmissions, bond graphs and
//! control block diagrams) behind [`Check::Mechanism`], and the helpers the other wave 4 pack
//! modules share ([`crate::annotation_rules`], [`crate::rel_rules`], [`crate::idef0_rules`]).
//!
//! The analyses behind the rules are `vizcraft_analysis::mech`, so a rule and the analysis that
//! explains it never disagree. Kinematic checks run per page: the bodies and joints a page draws
//! are one mechanism. Messages state only the finding.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use vizcraft_analysis::Types;
use vizcraft_analysis::mech::{self as m, Scope};
use vizcraft_diagram::Document;
use vizcraft_model::{Element, Issue, Relationship};

use crate::Catalog;
use crate::infer::Facts;
use crate::rules::cut;
use crate::schema::{Check, Pack, RuleDecl};

/// The checks of [`Check::Mechanism`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MechTest {
    /// A joint that joins fewer than two bodies.
    JointLinks,
    /// A kinematic page with joints and no frame (ground).
    OneGround,
    /// A mechanism whose Kutzbach mobility is zero or less: a structure, or over-constrained.
    Mobility,
    /// A spatial pair (spherical, cylindrical, screw, planar) on a planar kinematic diagram.
    PlanarPair,
    /// A shaft carrying gears, pulleys, sprockets or wheels with no bearing, motor or load on it.
    Supported,
    /// Two ways round a transmission loop that give different speeds.
    RatioConsistent,
    /// A 0-junction whose bonds do not have exactly one stroke at it, or a transformer whose two
    /// strokes are not one at it and one away.
    EffortSetter,
    /// A 1-junction whose bonds do not have exactly one stroke away from it, or a gyrator whose
    /// strokes are not both at it or both away.
    FlowSetter,
    /// An effort source with its stroke at it, or a flow source with its stroke away.
    SourceCausality,
    /// A capacitor or inertia in derivative causality.
    DerivativeCausality,
    /// A signal into a summing junction with no `+` or `-` sign.
    SignedInputs,
    /// A feedback loop with no integrator, delay or dynamic block on it.
    AlgebraicLoop,
    /// A test this build does not know (from a newer pack); skipped.
    #[default]
    #[serde(other)]
    Unknown,
}

impl MechTest {
    /// Whether this build knows the test.
    pub fn known(self) -> bool {
        self != MechTest::Unknown
    }
}

// ---------------------------------------------------------------- shared helpers

/// The catalog's type knowledge as the analyses read it.
pub(crate) struct FactTypes<'a>(pub(crate) Facts<'a>);

impl Types for FactTypes<'_> {
    fn is_a(&self, ty: &str, ancestor: &str) -> bool {
        self.0.is_a(ty, ancestor)
    }
    fn inverses(&self, _rel: &str) -> Vec<String> {
        Vec::new()
    }
}

/// Views by model id: (page index, shape or connector id), in page order.
pub(crate) fn views(doc: &Document) -> HashMap<&str, Vec<(usize, &str)>> {
    let mut v: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        for s in &page.shapes {
            if let Some(e) = s.element.as_deref() {
                v.entry(e).or_default().push((pi, s.id.as_str()));
            }
        }
        for c in &page.connectors {
            if let Some(r) = c.relationship.as_deref() {
                v.entry(r).or_default().push((pi, c.id.as_str()));
            }
        }
    }
    v
}

/// The element ids page `pi` draws.
pub(crate) fn page_scope(doc: &Document, pi: usize) -> HashSet<String> {
    doc.pages.get(pi).map(|p| p.shapes.iter().filter_map(|s| s.element.clone()).collect()).unwrap_or_default()
}

/// An issue whose message is only the finding, about `subjects`, pointing at their views (or
/// only those on page `on`).
pub(crate) fn finding(
    doc: &Document,
    views: &HashMap<&str, Vec<(usize, &str)>>,
    rule: &RuleDecl,
    what: &str,
    subjects: Vec<String>,
    on: Option<usize>,
) -> Issue {
    let mut items: Vec<(usize, &str)> = Vec::new();
    let mut seen = HashSet::new();
    for s in &subjects {
        for &(p, id) in views.get(s.as_str()).map_or(&[][..], Vec::as_slice) {
            if on.is_none_or(|o| o == p) && seen.insert(id) {
                items.push((p, id));
            }
        }
    }
    let page = match (on, items.first()) {
        (Some(o), _) => doc.pages.get(o).map(|pg| pg.id.clone()),
        (None, Some((p, _))) if items.iter().all(|(q, _)| q == p) => doc.pages.get(*p).map(|pg| pg.id.clone()),
        _ => None,
    };
    let mut i = Issue::new(rule.id.clone(), rule.severity, cut(what, crate::rules::MAX_MESSAGE))
        .with_subjects(subjects)
        .with_items(items.into_iter().map(|(_, id)| id.to_string()).collect());
    i.page = page;
    i
}

/// How a message names an element: its name in quotes, else its tag, else its id.
pub(crate) fn named(e: &Element) -> String {
    let n = e.name.trim().replace(['\n', '\r'], " ");
    if !n.is_empty() {
        format!("\"{}\"", cut(&n, crate::rules::MAX_LABEL))
    } else if !e.tag.trim().is_empty() {
        cut(e.tag.trim(), crate::rules::MAX_LABEL)
    } else {
        cut(&e.id, crate::rules::MAX_LABEL)
    }
}

/// The type's label (`Joint`), from the catalog, else the local id.
pub(crate) fn type_label(cat: &Catalog, type_id: &str) -> String {
    match cat.element_type(type_id) {
        Some((_, t)) if !t.label.is_empty() => t.label.clone(),
        _ => vizcraft_model::local_of(type_id).to_string(),
    }
}

/// `Joint "B"`: the type's label and the element's handle.
pub(crate) fn label(cat: &Catalog, e: &Element) -> String {
    format!("{} {}", type_label(cat, &e.type_id), named(e))
}

/// The indices of the pages that draw an element of a type in `types`.
pub(crate) fn pages_drawing(doc: &Document, facts: &Facts<'_>, types: &[&str]) -> Vec<usize> {
    doc.pages
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.shapes
                .iter()
                .any(|s| s.element.as_deref().and_then(|e| facts.element(e)).is_some_and(|e| types.iter().any(|t| facts.is_a(&e.type_id, t))))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Run the check of one of the four wave 4 engineering packs.
pub(crate) fn dispatch(doc: &Document, cat: &Catalog, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
    match &rule.check {
        Check::Mechanism { test } => check(doc, cat, rule, *test, out),
        Check::Annotation { test } => crate::annotation_rules::check(doc, cat, pack, rule, *test, out),
        Check::Reliability { test } => crate::rel_rules::check(doc, cat, rule, *test, out),
        Check::Idef0 { test } => crate::idef0_rules::check(doc, cat, rule, *test, out),
        _ => {}
    }
}

// ---------------------------------------------------------------- the mechanical checks

use vizcraft_analysis::mech::types::*;

fn check(doc: &Document, cat: &Catalog, rule: &RuleDecl, test: MechTest, out: &mut Vec<Issue>) {
    let facts = Facts::new(&doc.model, cat);
    let ty = FactTypes(Facts::new(&doc.model, cat));
    let views = views(doc);
    let model = &doc.model;
    let elements: HashMap<&str, &Element> = model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let el = |id: &str| elements.get(id).copied();
    match test {
        MechTest::JointLinks | MechTest::OneGround | MechTest::Mobility | MechTest::PlanarPair => {
            for pi in pages_drawing(doc, &facts, &[JOINT]) {
                let scope: Scope = Some(page_scope(doc, pi));
                let mob = m::mobility(model, &ty, &scope, false);
                let joints: Vec<String> = mob.joints.iter().map(|j| j.id.clone()).collect();
                match test {
                    MechTest::JointLinks => {
                        for j in mob.joints.iter().filter(|j| j.bodies < 2) {
                            let Some(e) = el(&j.id) else { continue };
                            let what = match j.bodies {
                                0 => format!("{} joins no bodies; link it to two.", label(cat, e)),
                                _ => format!("{} joins only one body; a joint links two.", label(cat, e)),
                            };
                            out.push(finding(doc, &views, rule, &what, vec![j.id.clone()], Some(pi)));
                        }
                    }
                    MechTest::OneGround if !mob.grounded => {
                        out.push(finding(doc, &views, rule, "The mechanism has no frame: draw a ground at its fixed pivots.", joints, Some(pi)));
                    }
                    MechTest::Mobility if mob.m <= 0 && !joints.is_empty() => {
                        let what = if mob.m == 0 {
                            format!("The mechanism cannot move: mobility is 0 ({}).", mob.formula)
                        } else {
                            format!("The mechanism is over-constrained: mobility is {} ({}).", mob.m, mob.formula)
                        };
                        out.push(finding(doc, &views, rule, &what, joints, Some(pi)));
                    }
                    MechTest::PlanarPair => {
                        for j in mob.joints.iter().filter(|j| !m::planar_kind(&j.kind)) {
                            let Some(e) = el(&j.id) else { continue };
                            let what = format!(
                                "{} is a {} pair, which moves out of the plane; the planar count takes it by its freedom.",
                                label(cat, e),
                                j.kind
                            );
                            out.push(finding(doc, &views, rule, &what, vec![j.id.clone()], Some(pi)));
                        }
                    }
                    _ => {}
                }
            }
        }
        MechTest::Supported => supported(doc, cat, &ty, &views, rule, out),
        MechTest::RatioConsistent => {
            for c in m::ratio_conflicts(model, &ty, &None) {
                let name = el(&c.element).map_or_else(|| c.element.clone(), |e| label(cat, e));
                let what = format!(
                    "{name} turns at {} ({}) one way round the loop and {} ({}) the other.",
                    c.found.text(),
                    if c.found.sign() < 0 { "opposite" } else { "same direction" },
                    c.other.text(),
                    if c.other.sign() < 0 { "opposite" } else { "same direction" }
                );
                out.push(finding(doc, &views, rule, &what, vec![c.element.clone(), c.relationship.clone()], None));
            }
        }
        MechTest::EffortSetter | MechTest::FlowSetter | MechTest::SourceCausality | MechTest::DerivativeCausality => {
            causality(doc, cat, &ty, &views, rule, test, out);
        }
        MechTest::SignedInputs => {
            for r in model.relationships.iter().filter(|r| facts.is_a(&r.type_id, SIGNAL)) {
                let Some(t) = el(&r.target).filter(|t| facts.is_a(&t.type_id, SUMMING)) else { continue };
                let sign = m::text(&r.properties, "sign");
                if sign != "+" && sign != "-" {
                    let from = el(&r.source).map_or_else(|| r.source.clone(), |e| label(cat, e));
                    let what = format!("The signal from {from} into {} has no sign; mark it + or -.", label(cat, t));
                    out.push(finding(doc, &views, rule, &what, vec![r.id.clone(), t.id.clone()], None));
                }
            }
        }
        MechTest::AlgebraicLoop => {
            for (els, rels) in m::algebraic_loops(model, &ty, &None) {
                let names: Vec<String> = els.iter().filter_map(|id| el(id)).take(5).map(named).collect();
                let what = format!("The loop through {} has no integrator, delay or dynamic block: an algebraic loop.", names.join(", "));
                let mut subjects = els;
                subjects.extend(rels);
                out.push(finding(doc, &views, rule, &what, subjects, None));
            }
        }
        MechTest::Unknown => {}
    }
}

/// Shafts (components joined by shaft relationships) carrying a gear, pulley, sprocket or wheel
/// with no bearing, motor or load among them.
fn supported(doc: &Document, cat: &Catalog, ty: &FactTypes<'_>, views: &HashMap<&str, Vec<(usize, &str)>>, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let model = &doc.model;
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for r in model.relationships.iter().filter(|r| ty.is_a(&r.type_id, SHAFT)) {
        adj.entry(r.source.as_str()).or_default().push(r.target.as_str());
        adj.entry(r.target.as_str()).or_default().push(r.source.as_str());
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for e in &model.elements {
        if seen.contains(e.id.as_str()) || !adj.contains_key(e.id.as_str()) {
            continue;
        }
        let mut group: Vec<&str> = Vec::new();
        let mut stack = vec![e.id.as_str()];
        seen.insert(e.id.as_str());
        while let Some(x) = stack.pop() {
            group.push(x);
            for &n in adj.get(x).map_or(&[][..], Vec::as_slice) {
                if seen.insert(n) {
                    stack.push(n);
                }
            }
        }
        let members: Vec<&Element> = model.elements.iter().filter(|x| group.contains(&x.id.as_str())).collect();
        let carries = members.iter().any(|x| [GEAR, PULLEY, SPROCKET, FRICTION_WHEEL].iter().any(|t| ty.is_a(&x.type_id, t)));
        let held = members.iter().any(|x| [BEARING, MOTOR, LOAD].iter().any(|t| ty.is_a(&x.type_id, t)));
        if carries && !held {
            let names: Vec<String> = members.iter().take(4).map(|x| label(cat, x)).collect();
            let what = format!("The shaft carrying {} has no bearing.", names.join(", "));
            out.push(finding(doc, views, rule, &what, members.iter().map(|x| x.id.clone()).collect(), None));
        }
    }
}

/// The bond graph causality checks over the strokes the bonds carry; elements with an
/// unassigned bond are skipped (the stroke rule reports those).
fn causality(
    doc: &Document,
    cat: &Catalog,
    ty: &FactTypes<'_>,
    views: &HashMap<&str, Vec<(usize, &str)>>,
    rule: &RuleDecl,
    test: MechTest,
    out: &mut Vec<Issue>,
) {
    let model = &doc.model;
    let mut at: HashMap<&str, Vec<&Relationship>> = HashMap::new();
    for r in model.relationships.iter().filter(|r| ty.is_a(&r.type_id, BOND)) {
        at.entry(r.source.as_str()).or_default().push(r);
        if r.target != r.source {
            at.entry(r.target.as_str()).or_default().push(r);
        }
    }
    for e in &model.elements {
        let Some(role) = m::bond_role(ty, &e.type_id) else { continue };
        let bonds = at.get(e.id.as_str()).cloned().unwrap_or_default();
        if bonds.is_empty() {
            continue;
        }
        // For each bond: whether its stroke is at this element; None while any is unassigned.
        let Some(strokes) = bonds.iter().map(|b| m::stroke_of(b).map(|s| (b.target == e.id) == s)).collect::<Option<Vec<bool>>>() else {
            continue;
        };
        let at_count = strokes.iter().filter(|s| **s).count();
        let away = strokes.len() - at_count;
        let mut subjects = vec![e.id.clone()];
        subjects.extend(bonds.iter().map(|b| b.id.clone()));
        let what = match (test, role) {
            (MechTest::EffortSetter, m::BondRole::J0) if at_count != 1 => {
                Some(format!("{} has {at_count} bonds setting its effort; a 0-junction takes exactly one.", label(cat, e)))
            }
            (MechTest::EffortSetter, m::BondRole::Tf) if strokes.len() == 2 && at_count != 1 => Some(format!(
                "{} has both strokes on one side; a transformer passes causality through, one stroke at it and one away.",
                label(cat, e)
            )),
            (MechTest::FlowSetter, m::BondRole::J1) if away != 1 => {
                Some(format!("{} has {away} bonds setting its flow; a 1-junction takes exactly one.", label(cat, e)))
            }
            (MechTest::FlowSetter, m::BondRole::Gy) if strokes.len() == 2 && at_count == 1 => {
                Some(format!("{} has one stroke at it and one away; a gyrator takes both at it or both away.", label(cat, e)))
            }
            (MechTest::SourceCausality, m::BondRole::Effort) if at_count > 0 => {
                Some(format!("{} has its causal stroke at it; an effort source sets the effort, so the stroke goes at the far end.", label(cat, e)))
            }
            (MechTest::SourceCausality, m::BondRole::Flow) if away > 0 => {
                Some(format!("{} has its causal stroke away from it; a flow source takes the stroke at its own end.", label(cat, e)))
            }
            (MechTest::DerivativeCausality, m::BondRole::C) if at_count > 0 => {
                Some(format!("{} is in derivative causality (its stroke is at it); integral causality is preferred.", label(cat, e)))
            }
            (MechTest::DerivativeCausality, m::BondRole::I) if away > 0 => {
                Some(format!("{} is in derivative causality (its stroke is away from it); integral causality is preferred.", label(cat, e)))
            }
            _ => None,
        };
        if let Some(what) = what {
            out.push(finding(doc, views, rule, &what, subjects, None));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_tests_load_as_unknown() {
        let t: MechTest = serde_json::from_str("\"warpDrive\"").unwrap();
        assert_eq!(t, MechTest::Unknown);
        assert!(!t.known() && MechTest::Mobility.known());
        let c: Check = serde_json::from_str(r#"{"kind": "mechanism", "test": "mobility"}"#).unwrap();
        assert_eq!(c, Check::Mechanism { test: MechTest::Mobility });
    }
}
