//! The checks of the `reliability` pack (fault trees, reliability block diagrams, event trees and
//! FMEA worksheets) behind [`Check::Reliability`]. Messages state only the finding.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::json;
use vizcraft_analysis::mech::number;
use vizcraft_analysis::reliability::block_availability;
use vizcraft_analysis::reliability::types::*;
use vizcraft_diagram::Document;
use vizcraft_model::issue::Fix;
use vizcraft_model::{Element, Issue};

use crate::Catalog;
use crate::infer::Facts;
use crate::mech_rules::{finding, label, views};
use crate::schema::RuleDecl;

/// The checks of `Check::Reliability`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelTest {
    /// No top event, or more than one, or a top event feeding a gate.
    OneTopEvent,
    /// An inhibit gate without exactly one input event and one conditioning event.
    InhibitInputs,
    /// A voting gate or node whose k is not between 1 and its number of inputs.
    VotingK,
    /// A block with no availability, MTBF and MTTR, or failure and repair rates.
    BlockData,
    /// An event tree branch point without exactly one success and one failure branch.
    TwoBranches,
    /// An event tree path that ends somewhere other than an outcome.
    EndsInOutcome,
    /// A probability outside 0 to 1, or a negative frequency.
    ProbabilityRange,
    /// A severity, occurrence or detection rating that is not a whole number from 1 to 10.
    ScaleRange,
    /// A risk priority number that is not severity times occurrence times detection.
    RpnCurrent,
    /// A test this build does not know (from a newer pack); skipped.
    #[default]
    #[serde(other)]
    Unknown,
}

impl RelTest {
    /// Whether this build knows the test.
    pub fn known(self) -> bool {
        self != RelTest::Unknown
    }
}

/// The ratings an FMEA row carries, each 1 to 10.
pub const RATINGS: [&str; 3] = ["severity", "occurrence", "detection"];

/// The risk priority number of a failure mode, when its three ratings are set.
pub fn rpn_of(e: &Element) -> Option<f64> {
    RATINGS.iter().map(|k| number(&e.properties, k)).product()
}

pub(crate) fn check(doc: &Document, cat: &Catalog, rule: &RuleDecl, test: RelTest, out: &mut Vec<Issue>) {
    let facts = Facts::new(&doc.model, cat);
    let views = views(doc);
    let model = &doc.model;
    let is = |e: &Element, t: &str| facts.is_a(&e.type_id, t);
    let of = |t: &str| -> Vec<&Element> { model.elements.iter().filter(|e| facts.is_a(&e.type_id, t)).collect() };
    let mut incoming: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    let mut outgoing: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for r in &model.relationships {
        incoming.entry(r.target.as_str()).or_default().push((r.source.as_str(), r.type_id.as_str()));
        outgoing.entry(r.source.as_str()).or_default().push((r.target.as_str(), r.type_id.as_str()));
    }
    let count =
        |map: &HashMap<&str, Vec<(&str, &str)>>, id: &str, t: &str| map.get(id).map_or(0, |v| v.iter().filter(|(_, ty)| facts.is_a(ty, t)).count());
    let push = |out: &mut Vec<Issue>, what: String, subjects: Vec<String>| out.push(finding(doc, &views, rule, &what, subjects, None));
    match test {
        RelTest::OneTopEvent => {
            let tops = of(TOP);
            let gates_or_events = [AND, OR, XOR, PAND, INHIBIT, VOTING_GATE, BASIC, INTERMEDIATE];
            let has_tree = model.elements.iter().any(|e| gates_or_events.iter().any(|t| is(e, t)));
            if tops.is_empty() && has_tree {
                let any: Vec<String> =
                    model.elements.iter().filter(|e| gates_or_events.iter().any(|t| is(e, t))).take(1).map(|e| e.id.clone()).collect();
                push(out, "The fault tree has no top event.".into(), any);
            }
            if tops.len() > 1 {
                push(out, format!("The fault tree has {} top events; it analyses one.", tops.len()), tops.iter().map(|e| e.id.clone()).collect());
            }
            for t in &tops {
                if count(&outgoing, &t.id, INPUT) > 0 {
                    push(out, format!("{} feeds a gate; the top event is the root of the tree.", label(cat, t)), vec![t.id.clone()]);
                }
            }
        }
        RelTest::InhibitInputs => {
            for g in of(INHIBIT) {
                let ins = incoming.get(g.id.as_str()).map_or(&[][..], Vec::as_slice);
                let inputs: Vec<&str> = ins.iter().filter(|(_, t)| facts.is_a(t, INPUT)).map(|(s, _)| *s).collect();
                let conditions = inputs.iter().filter(|s| model.element(s).is_some_and(|e| is(e, CONDITIONING))).count();
                let events = inputs.len() - conditions;
                if events != 1 || conditions != 1 {
                    push(
                        out,
                        format!("{} has {events} input event(s) and {conditions} condition(s); an inhibit gate takes one of each.", label(cat, g)),
                        vec![g.id.clone()],
                    );
                }
            }
        }
        RelTest::VotingK => {
            for (t, via) in [(VOTING_GATE, INPUT), (VOTING, PATH)] {
                for g in of(t) {
                    let n = count(&incoming, &g.id, via);
                    let k = number(&g.properties, "k");
                    if k.is_none_or(|k| k.fract() != 0.0 || k < 1.0 || k > n as f64) {
                        let shown = k.map_or_else(|| "not set".into(), vizcraft_analysis::mech::fmt);
                        push(
                            out,
                            format!("{} has k {shown} with {n} input(s); k is a whole number from 1 to the number of inputs.", label(cat, g)),
                            vec![g.id.clone()],
                        );
                    }
                }
            }
        }
        RelTest::BlockData => {
            for b in of(BLOCK) {
                if block_availability(b).is_none() {
                    push(out, format!("{} has no availability, MTBF and MTTR, or failure and repair rates.", label(cat, b)), vec![b.id.clone()]);
                }
            }
        }
        RelTest::TwoBranches => {
            for b in of(BRANCH_POINT) {
                let (s, f) = (count(&outgoing, &b.id, SUCCESS), count(&outgoing, &b.id, FAILURE));
                if s != 1 || f != 1 {
                    push(
                        out,
                        format!("{} has {s} success and {f} failure branch(es); a branch point takes one of each.", label(cat, b)),
                        vec![b.id.clone()],
                    );
                }
            }
        }
        RelTest::EndsInOutcome => {
            for e in model.elements.iter().filter(|e| is(e, INITIATOR) || is(e, BRANCH_POINT)) {
                let branches = count(&outgoing, &e.id, SUCCESS) + count(&outgoing, &e.id, FAILURE);
                if branches == 0 {
                    push(out, format!("{} ends a path; every path of an event tree ends in an outcome.", label(cat, e)), vec![e.id.clone()]);
                }
            }
        }
        RelTest::ProbabilityRange => {
            for e in &model.elements {
                for key in ["probability", "failureProbability", "availability"] {
                    if let Some(p) = number(&e.properties, key).filter(|p| !(0.0..=1.0).contains(p)) {
                        push(out, format!("{} has {key} {}, outside 0 to 1.", label(cat, e), vizcraft_analysis::mech::fmt(p)), vec![e.id.clone()]);
                    }
                }
                if is(e, INITIATOR)
                    && let Some(f) = number(&e.properties, "frequency").filter(|f| *f < 0.0)
                {
                    push(out, format!("{} has a negative frequency, {}.", label(cat, e), vizcraft_analysis::mech::fmt(f)), vec![e.id.clone()]);
                }
            }
        }
        RelTest::ScaleRange => {
            for e in of(FAILURE_MODE) {
                for key in RATINGS {
                    let Some(v) = number(&e.properties, key) else { continue };
                    if v.fract() != 0.0 || !(1.0..=10.0).contains(&v) {
                        push(
                            out,
                            format!("{} has {key} {}; ratings are whole numbers from 1 to 10.", label(cat, e), vizcraft_analysis::mech::fmt(v)),
                            vec![e.id.clone()],
                        );
                    }
                }
            }
        }
        RelTest::RpnCurrent => {
            for e in of(FAILURE_MODE) {
                let Some(want) = rpn_of(e) else { continue };
                let have = number(&e.properties, "rpn");
                if have.is_none_or(|h| (h - want).abs() > 1e-9) {
                    let shown = have.map_or_else(|| "not set".into(), vizcraft_analysis::mech::fmt);
                    let what = format!(
                        "{} has RPN {shown}; severity times occurrence times detection is {}.",
                        label(cat, e),
                        vizcraft_analysis::mech::fmt(want)
                    );
                    let mut i = finding(doc, &views, rule, &what, vec![e.id.clone()], None);
                    i = i.with_fix(Fix::new("Recompute the RPN", "rel.rpn", json!({"ids": [e.id]})));
                    out.push(i);
                }
            }
        }
        RelTest::Unknown => {}
    }
}
